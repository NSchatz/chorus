// The doctoring, for the states a real server cannot be made to produce.
//
// Several of the claims are about what the page does when the state it receives
// is not one chorus-server will ever send, or when the feed underneath it dies:
// a volume that is not a number, an endpoint list that is absent, an event
// stream that is severed and then restored, a state request that never answers,
// and a server that has STOPPED ANSWERING under a connection that is still
// established. There is no flag that makes a correct server emit any of those.
//
// So the page is put in front of a server that will. This one PROXIES the page,
// both its stylesheets, its script and its document straight through from the real
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

const PROXIED = new Set([
  "/",
  "/index.html",
  "/tokens.css",
  "/chorus.css",
  "/chorus.js",
]);

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

/// `paused` is the state a SIGSTOPped server is in, and the one an EventSource
/// cannot see: nothing is severed, every established connection stays up, and
/// the server answers nothing at all. It is reached with `/fixture/pause` and
/// left with `/fixture/resume`.
let scenario = {
  stateStatus: 200,
  stateDelayMs: 0,
  stateBody: null,
  state: twoZones(),
  eventsMode: "open",
  paused: false,
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

  // A paused server accepts the connection and answers nothing, on any route
  // the page under test uses. Nothing is destroyed: an EventSource already
  // established stays OPEN and a fetch just never comes back, which is what a
  // stopped process looks like from a browser.
  if (scenario.paused && (path === "/api/state" || path === "/api/events")) {
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

  // The demonstration for the paused-feed claim: a page that decides freshness
  // the way the control page did BEFORE this work - from `EventSource.onerror`
  // and a watchdog on `readyState`, and from nothing else. Against a server
  // that has stopped answering under a connection that is still established,
  // both of those go on saying everything is fine, and the same measuring
  // function the real claim is graded with reports it still reading as live.
  // It is served from here rather than written into the spec file because an
  // EventSource has to be same-origin with something that answers /api/events.
  if (path === "/demo/stale-blind") {
    response.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
    response.end(
      '<!doctype html><html lang="en"><head><meta charset="utf-8">' +
        "<title>a page that watches only its connection</title></head><body>" +
        "<p data-connection>Connecting</p>" +
        '<p>Figures <span data-freshness="kitchen">live</span></p>' +
        '<p>Figures <span data-freshness="footer">live</span></p>' +
        '<p data-volume="kitchen">38%</p>' +
        "<script>\n" +
        "var connection = document.querySelector('[data-connection]');\n" +
        "var marks = document.querySelectorAll('[data-freshness]');\n" +
        "var everOpened = false;\n" +
        "function paint(fresh) {\n" +
        "  connection.textContent = fresh ? 'Live' : 'Connection lost';\n" +
        "  for (var i = 0; i < marks.length; i += 1) {\n" +
        "    marks[i].textContent = fresh ? 'live' : 'last known';\n" +
        "  }\n" +
        "}\n" +
        "var events = new EventSource('/api/events');\n" +
        "events.onopen = function () { everOpened = true; paint(true); };\n" +
        "events.onmessage = function () { everOpened = true; paint(true); };\n" +
        "events.onerror = function () { if (everOpened) { paint(false); } };\n" +
        "window.setInterval(function () {\n" +
        "  if (events.readyState === 1) { everOpened = true; paint(true); return; }\n" +
        "  if (everOpened) { paint(false); }\n" +
        "}, 1000);\n" +
        "</script></body></html>\n"
    );
    return;
  }

  // The demonstration for the bounded-loading claim: a page that bootstraps its
  // state the way the control page did BEFORE this work - one fetch("/api/state")
  // with no deadline on it, with everything else the page does started from
  // that fetch's .finally(). Against a server that accepts the connection and
  // answers nothing the promise never settles, so the loading notice is the
  // whole page for as long as the tab is open, and the same
  // `reads.statesShowing` the real claim is graded with reports it still
  // loading. It is served from here rather than written into the spec file
  // because the request has to be same-origin with something that can be paused.
  if (path === "/demo/loading-blind") {
    response.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
    response.end(
      '<!doctype html><html lang="en"><head><meta charset="utf-8">' +
        "<title>a page whose first request has no deadline</title></head><body>" +
        "<p data-connection>Connecting</p>" +
        '<main data-zones><section data-loading data-region="loading">' +
        "<h2>Loading</h2><p>Reading this server's zones.</p>" +
        "</section></main>" +
        "<script>\n" +
        "var zones = document.querySelector('[data-zones]');\n" +
        "var connection = document.querySelector('[data-connection]');\n" +
        "function show(html) { zones.innerHTML = html; }\n" +
        "function fail() {\n" +
        "  show('<section data-error><h2>State could not be read</h2>" +
        "<p>Check chorus-server is running, then reload.</p></section>');\n" +
        "  connection.textContent = 'Connection lost';\n" +
        "}\n" +
        "function draw(state) {\n" +
        "  var html = '';\n" +
        "  for (var i = 0; i < state.zones.length; i += 1) {\n" +
        "    html += '<section data-zone=\"' + state.zones[i].id + '\"><h2>' +\n" +
        "      state.zones[i].name + '</h2></section>';\n" +
        "  }\n" +
        "  show(html);\n" +
        "  connection.textContent = 'Live';\n" +
        "}\n" +
        "fetch('/api/state')\n" +
        "  .then(function (r) {\n" +
        "    if (!r.ok) { throw new Error('the server answered ' + r.status); }\n" +
        "    return r.json();\n" +
        "  })\n" +
        "  .then(draw)\n" +
        "  .catch(fail);\n" +
        "</script></body></html>\n"
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
      paused: false,
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

  if (path === "/fixture/pause" && request.method === "POST") {
    scenario.paused = true;
    json(response, 200, { paused: true, streamsStillOpen: streams.size });
    return;
  }

  if (path === "/fixture/resume" && request.method === "POST") {
    scenario.paused = false;
    // Whatever is still subscribed hears the current state, so a page that
    // never lost its stream learns the server is back without reconnecting.
    push();
    json(response, 200, { paused: false, streamsStillOpen: streams.size });
    return;
  }

  // How many event streams this fixture is still holding open. It is what tells
  // a PAUSED feed apart from a severed one from the outside: a severed feed has
  // none, and a paused one has the connection it always had.
  if (path === "/fixture/streams") {
    json(response, 200, { open: streams.size });
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
