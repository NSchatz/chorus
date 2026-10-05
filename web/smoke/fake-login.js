// A fake forward-auth login for the browser smoke test, and nothing else: it
// stands where a household's reverse proxy and its login stand in front of
// chorus-server, so the test loads the app the way a signed-in browser does.
//
//   no session   a page request is redirected to /fake-login?rd=<path>; any
//                other request is refused with 401. Nothing reaches the server.
//   /fake-login  a form (GET) that takes any name and, posted, starts a session:
//                a cookie with a random token, then a redirect back to <rd>.
//   a session    the request is passed to chorus-server unchanged but for the
//                session cookie, which is removed, and a Remote-User header,
//                which is set. The Host header stays the browser's, as it does
//                behind a real proxy: the server compares a command's Origin
//                with it.
//
//   expired      `expire()` ends every session, as a login's lifetime does, and
//                ends the responses still open (an event stream outlives its
//                session only until it next reconnects). From then on a
//                request with no session is redirected to /fake-login whatever
//                it asks for, the way a forward-auth that does not look at
//                the Accept header answers: so the test sees both answers a
//                login gives a page's API call, the 401 and the redirect.
//
// It checks no password and keeps its sessions in memory: it is a test helper
// and is never served by chorus-server or shipped.

import { randomBytes } from "node:crypto";
import http from "node:http";

const COOKIE = "chorus_fake_session";
export const LOGIN_PATH = "/fake-login";

const LOGIN_PAGE = `<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>Sign in (fake)</title></head>
<body>
<h1>Sign in</h1>
<form method="post" action="${LOGIN_PATH}">
<input type="hidden" name="rd" value="{{rd}}">
<label>Name <input name="user" autocomplete="username" required></label>
<button type="submit">Sign in</button>
</form>
</body>
</html>
`;

// Where a sign-in may send the browser back to: a path on this origin only.
function safeReturn(rd) {
  return typeof rd === "string" && /^\/(?!\/)[\x21-\x7e]*$/.test(rd) && !rd.includes("\\") ? rd : "/app/";
}

function escapeAttribute(text) {
  return text.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
}

function sessionOf(request) {
  for (const part of (request.headers.cookie ?? "").split(";")) {
    const [name, value] = part.trim().split("=");
    if (name === COOKIE) return value;
  }
  return undefined;
}

function readBody(request, limit = 4096) {
  return new Promise((resolve, reject) => {
    let body = "";
    request.setEncoding("utf8");
    request.on("data", (chunk) => {
      body += chunk;
      if (body.length > limit) reject(new Error("the login form is larger than a login form"));
    });
    request.on("end", () => resolve(body));
    request.on("error", reject);
  });
}

// Start the fake login on a loopback port of its own choosing, in front of the
// chorus-server control listener at 127.0.0.1:<upstreamPort>. Resolves to
// { origin, users(), refused(), expire(), close() }: users() is the names
// signed in now, refused() the path of every request that came with no
// session (and so never reached the server).
export async function startFakeLogin(upstreamPort) {
  const sessions = new Map();
  const refused = [];
  // The responses being passed from the server, to end when the login expires.
  const open = new Set();
  let expired = false;

  const server = http.createServer(async (request, response) => {
    const url = new URL(request.url, "http://fake-login.invalid");
    try {
      if (url.pathname === LOGIN_PATH && request.method === "GET") {
        const page = LOGIN_PAGE.replace("{{rd}}", escapeAttribute(safeReturn(url.searchParams.get("rd"))));
        response.writeHead(200, { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store" });
        response.end(page);
        return;
      }
      if (url.pathname === LOGIN_PATH && request.method === "POST") {
        const form = new URLSearchParams(await readBody(request));
        const user = (form.get("user") ?? "").trim();
        if (!user) {
          response.writeHead(400, { "Content-Type": "text/plain; charset=utf-8" });
          response.end("a name is needed to sign in\n");
          return;
        }
        const token = randomBytes(16).toString("hex");
        sessions.set(token, user);
        response.writeHead(303, {
          Location: safeReturn(form.get("rd")),
          "Set-Cookie": `${COOKIE}=${token}; Path=/; HttpOnly; SameSite=Lax`,
          "Cache-Control": "no-store",
        });
        response.end();
        return;
      }

      const user = sessions.get(sessionOf(request));
      if (user === undefined) {
        refused.push(url.pathname);
        const wantsPage = request.method === "GET" && (request.headers.accept ?? "").includes("text/html");
        if (wantsPage || expired) {
          response.writeHead(302, { Location: `${LOGIN_PATH}?rd=${encodeURIComponent(request.url)}`, "Cache-Control": "no-store" });
          response.end();
        } else {
          response.writeHead(401, { "Content-Type": "text/plain; charset=utf-8", "Cache-Control": "no-store" });
          response.end("not signed in\n");
        }
        request.resume();
        return;
      }

      const headers = { ...request.headers, "remote-user": user };
      const cookies = (request.headers.cookie ?? "")
        .split(";")
        .map((part) => part.trim())
        .filter((part) => part && !part.startsWith(`${COOKIE}=`));
      if (cookies.length > 0) headers.cookie = cookies.join("; ");
      else delete headers.cookie;
      const upstream = http.request(
        { host: "127.0.0.1", port: upstreamPort, method: request.method, path: request.url, headers },
        (answer) => {
          response.writeHead(answer.statusCode ?? 502, answer.headers);
          answer.pipe(response);
        },
      );
      const passing = { response, upstream };
      open.add(passing);
      response.once("close", () => open.delete(passing));
      upstream.on("error", (error) => {
        if (!response.headersSent) response.writeHead(502, { "Content-Type": "text/plain; charset=utf-8" });
        response.end(`chorus-server did not answer: ${error.message}\n`);
      });
      request.pipe(upstream);
    } catch (error) {
      if (!response.headersSent) response.writeHead(500, { "Content-Type": "text/plain; charset=utf-8" });
      response.end(`${error.message}\n`);
    }
  });

  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  return {
    origin: `http://127.0.0.1:${server.address().port}`,
    users: () => [...sessions.values()],
    refused: () => [...refused],
    expire() {
      sessions.clear();
      expired = true;
      for (const { response, upstream } of [...open]) {
        // The response ends as a whole one; the server's side is dropped.
        response.end();
        upstream.destroy();
      }
    },
    close: () =>
      new Promise((resolve) => {
        server.closeAllConnections();
        server.close(() => resolve());
      }),
  };
}
