// The doctoring, for the states a real server cannot be made to produce.
//
// Four of the claims are about what the page does when the state it receives is
// not one chorus-server will ever send, or when the feed underneath it dies: a
// volume that is not a number, an endpoint list that is absent, an event stream
// that is severed and then restored, and a state request that never answers.
// There is no flag that makes a correct server emit any of those.
//
// So the page is put in front of a server that will. This one PROXIES the page,
// its stylesheet, its script and its document straight through from the real
// chorus-server, byte for byte and header for header, and answers only
// `/api/state` and `/api/events` itself. What is under test is therefore the
// shipped page, unmodified, reading a state that has been doctored on purpose.
// The doctoring is the fixture, and nothing about the page is stubbed.
//
//   CHORUS_UI_BASE=http://127.0.0.1:4020 CHORUS_FIXTURE_PORT=4099 node fixture-server.js

const http = require("http");
const { URL } = require("url");

const UPSTREAM = process.env.CHORUS_UI_BASE;
const PORT = Number(process.env.CHORUS_FIXTURE_PORT || 0);

if (!UPSTREAM) {
  process.stderr.write(
    "CHORUS_UI_BASE names the real chorus-server whose page this proxies\n"
  );
  process.exit(2);
}

const PROXIED = new Set(["/", "/index.html", "/chorus.css", "/chorus.js"]);

/// A state message with two zones, which is what most scenarios start from.
function twoZones() {
  return {
    v: 1,
    t: "state",
    serial: 4,
    zones: [
      {
        id: "kitchen",
        name: "The Kitchen",
        group: "downstairs",
        volume: 0.375,
        muted: false,
        endpoints: ["endpoint-a", "endpoint-b"],
        present: ["endpoint-a"],
        audio: "127.0.0.1:4011",
      },
      {
        id: "study",
        name: "The Study",
        group: "study",
        volume: 0.9,
        muted: true,
        endpoints: ["endpoint-c"],
        present: ["endpoint-c"],
        audio: "127.0.0.1:4012",
      },
    ],
  };
}

let scenario = {
  stateStatus: 200,
  stateDelayMs: 0,
  stateBody: null,
  state: twoZones(),
  eventsMode: "open",
};

const streams = new Set();

function currentBody() {
  if (scenario.stateBody !== null) {
    return scenario.stateBody;
  }
  return JSON.stringify(scenario.state);
}

function proxy(request, response) {
  const target = new URL(request.url, UPSTREAM);
  const upstream = http.request(
    {
      hostname: target.hostname,
      port: target.port,
      path: target.pathname + target.search,
      method: request.method,
      headers: { host: target.host },
    },
    (answer) => {
      // Verbatim: the status and every header the real server set, the
      // Content-Security-Policy included. What the browser loads here is what it
      // would load from the real server.
      response.writeHead(answer.statusCode, answer.headers);
      answer.pipe(response);
    }
  );
  upstream.on("error", (e) => {
    response.writeHead(502, { "Content-Type": "text/plain" });
    response.end(`the fixture could not reach ${UPSTREAM}: ${e.message}\n`);
  });
  request.pipe(upstream);
}

function json(response, status, value) {
  const body = JSON.stringify(value);
  response.writeHead(status, {
    "Content-Type": "application/json",
    "Content-Length": Buffer.byteLength(body),
    "Cache-Control": "no-store",
  });
  response.end(body);
}

function readBody(request) {
  return new Promise((resolve) => {
    let text = "";
    request.on("data", (chunk) => {
      text += chunk;
    });
    request.on("end", () => resolve(text));
  });
}

function push() {
  const line = `data: ${currentBody()}\n\n`;
  for (const stream of streams) {
    stream.write(line);
  }
}

function sever() {
  scenario.eventsMode = "severed";
  for (const stream of streams) {
    // A network error, not a status code. An EventSource whose connection is
    // refused with a status gives up for good; one whose socket dies keeps
    // retrying, which is what a server that has gone away looks like and what
    // the restore half of the claim needs.
    stream.socket.destroy();
  }
  streams.clear();
}

const server = http.createServer(async (request, response) => {
  const path = request.url.split("?")[0];

  if (PROXIED.has(path) || path.startsWith("/docs/")) {
    proxy(request, response);
    return;
  }

  if (path === "/api/state") {
    const answer = () => {
      if (scenario.stateStatus !== 200) {
        response.writeHead(scenario.stateStatus, {
          "Content-Type": "text/plain",
          "Cache-Control": "no-store",
        });
        response.end("the fixture is refusing the state on purpose\n");
        return;
      }
      const body = currentBody();
      response.writeHead(200, {
        "Content-Type": "application/json",
        "Content-Length": Buffer.byteLength(body),
        "Cache-Control": "no-store",
      });
      response.end(body);
    };
    if (scenario.stateDelayMs > 0) {
      setTimeout(answer, scenario.stateDelayMs);
    } else {
      answer();
    }
    return;
  }

  if (path === "/api/events") {
    if (scenario.eventsMode === "severed") {
      request.socket.destroy();
      return;
    }
    response.writeHead(200, {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-store",
      Connection: "close",
    });
    streams.add(response);
    response.on("close", () => streams.delete(response));
    if (scenario.eventsMode === "open") {
      response.write(`data: ${currentBody()}\n\n`);
    }
    return;
  }

  if (path === "/api/command") {
    json(response, 400, {
      v: 1,
      t: "error",
      field: "name",
      detail: "the fixture refuses every command; the real server is where a refusal is graded",
    });
    return;
  }

  // The two demonstrations for the policy claim. Both are pages this fixture
  // serves rather than the real server, because what they mutate IS the header.
  if (path === "/demo/demo.css") {
    response.writeHead(200, { "Content-Type": "text/css" });
    response.end("body { background: rgb(1, 2, 3); }\n");
    return;
  }
  if (path === "/demo/no-csp" || path === "/demo/blocked-style") {
    const headers = { "Content-Type": "text/html; charset=utf-8" };
    if (path === "/demo/blocked-style") {
      // A policy that silences the page it was sent with. This is the failure
      // the claim names by name: a policy is not a pass on its own.
      headers["Content-Security-Policy"] = "default-src 'none'";
    }
    response.writeHead(200, headers);
    response.end(
      '<!doctype html><html lang="en"><head><meta charset="utf-8">' +
        '<link rel="stylesheet" href="/demo/demo.css"></head>' +
        "<body><p>a demonstration page</p></body></html>\n"
    );
    return;
  }

  if (path === "/fixture/scenario" && request.method === "POST") {
    const wanted = JSON.parse((await readBody(request)) || "{}");
    scenario = {
      stateStatus: 200,
      stateDelayMs: 0,
      stateBody: null,
      state: twoZones(),
      eventsMode: "open",
      ...wanted,
    };
    for (const stream of streams) {
      stream.socket.destroy();
    }
    streams.clear();
    json(response, 200, { scenario: { ...scenario, state: "<set>" } });
    return;
  }

  if (path === "/fixture/sever" && request.method === "POST") {
    sever();
    json(response, 200, { severed: true });
    return;
  }

  if (path === "/fixture/restore" && request.method === "POST") {
    scenario.eventsMode = "open";
    json(response, 200, { severed: false });
    return;
  }

  if (path === "/fixture/push" && request.method === "POST") {
    const body = await readBody(request);
    if (body) {
      scenario.state = JSON.parse(body);
      scenario.stateBody = null;
    }
    push();
    json(response, 200, { pushed: streams.size });
    return;
  }

  response.writeHead(404, { "Content-Type": "text/plain" });
  response.end("no such route in the fixture\n");
});

server.listen(PORT, "127.0.0.1", () => {
  process.stdout.write(
    `fixture listening on=http://127.0.0.1:${server.address().port} upstream=${UPSTREAM}\n`
  );
});
