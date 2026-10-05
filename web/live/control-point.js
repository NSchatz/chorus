// What the live test of what is playing (`playing.live.js`) needs besides the
// server: somebody else's media and its cover, and a UPnP AV control point to
// tell a room's renderer to play them. That is the one way anything outside
// the server gives a group a now-playing record with artwork (docs/upnp.md;
// docs/control-plane.md: the record is set by the server's runtime, by no
// command).
//
// Everything is on loopback, which is why the test starts its server with
// `--media-allow-loopback`.

import assert from "node:assert/strict";
import { createSocket } from "node:dgram";
import { createServer } from "node:http";

// A 1 by 1 PNG: the server passes an image on by its first bytes, and a
// browser would draw this one.
const COVER = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==",
  "base64",
);

// Two minutes of 48 kHz 16-bit stereo silence as a WAV file: longer than the
// test runs, so the track never ends under it (a track that ends takes its
// now-playing record with it).
const RATE = 48_000;
const FRAME_BYTES = 4;
const DATA_BYTES = RATE * FRAME_BYTES * 120;
function wavHeader() {
  const header = Buffer.alloc(44);
  header.write("RIFF", 0, "ascii");
  header.writeUInt32LE(36 + DATA_BYTES, 4);
  header.write("WAVEfmt ", 8, "ascii");
  header.writeUInt32LE(16, 16);
  header.writeUInt16LE(1, 20); // PCM
  header.writeUInt16LE(2, 22); // channels
  header.writeUInt32LE(RATE, 24);
  header.writeUInt32LE(RATE * FRAME_BYTES, 28);
  header.writeUInt16LE(FRAME_BYTES, 32);
  header.writeUInt16LE(16, 34);
  header.write("data", 36, "ascii");
  header.writeUInt32LE(DATA_BYTES, 40);
  return header;
}

// The bytes [from, to] of the track, without ever holding the whole of it.
function trackBytes(from, to) {
  const body = Buffer.alloc(to - from + 1);
  if (from < 44) wavHeader().copy(body, 0, from, Math.min(44, to + 1));
  return body;
}

// Start the origin: `track` (the WAV) and `cover` (the PNG) are its two
// addresses, and `asked` the paths it has been asked for, in order.
export async function startOrigin() {
  const asked = [];
  const total = 44 + DATA_BYTES;
  const server = createServer((request, response) => {
    asked.push(request.url);
    if (request.url === "/cover.png") {
      response.writeHead(200, { "Content-Type": "image/png", "Content-Length": COVER.length });
      response.end(request.method === "HEAD" ? undefined : COVER);
      return;
    }
    if (request.url !== "/track.wav") {
      response.writeHead(404).end();
      return;
    }
    const range = /^bytes=(\d+)-(\d*)$/.exec(request.headers.range ?? "");
    const from = range ? Number(range[1]) : 0;
    const to = range && range[2] ? Math.min(Number(range[2]), total - 1) : total - 1;
    if (from > to) {
      response.writeHead(416, { "Content-Range": `bytes */${total}` }).end();
      return;
    }
    response.writeHead(range ? 206 : 200, {
      "Content-Type": "audio/wav",
      "Accept-Ranges": "bytes",
      "Content-Length": to - from + 1,
      ...(range ? { "Content-Range": `bytes ${from}-${to}/${total}` } : {}),
    });
    response.end(request.method === "HEAD" ? undefined : trackBytes(from, to));
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const root = `http://127.0.0.1:${server.address().port}`;
  return {
    track: `${root}/track.wav`,
    cover: `${root}/cover.png`,
    asked,
    async stop() {
      server.closeAllConnections();
      await new Promise((resolve) => server.close(resolve));
    },
  };
}

// A UDP socket on loopback: where the server is told to send its discovery
// notifications (`--upnp-ssdp-group`), and what the search below is sent from.
export async function openDiscovery() {
  const socket = createSocket("udp4");
  await new Promise((resolve) => socket.bind(0, "127.0.0.1", resolve));
  return {
    port: socket.address().port,
    socket,
    close: () => new Promise((resolve) => socket.close(resolve)),
  };
}

const escapeXml = (text) =>
  String(text).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

// Find the renderer of the room called `room`: search the server's discovery
// port for media renderers and read each one's description until one has
// that friendly name. Resolves to its root, `http://<host>:<port>/upnp/<uuid>`.
async function rendererOf(discovery, ssdpPort, room) {
  const locations = new Set();
  const onMessage = (message) => {
    const found = /^LOCATION:\s*(\S+)/im.exec(message.toString("latin1"));
    if (found) locations.add(found[1]);
  };
  discovery.socket.on("message", onMessage);
  const search = Buffer.from(
    'M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: "ssdp:discover"\r\nMX: 1\r\n' +
      "ST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n",
    "latin1",
  );
  const deadline = Date.now() + 10_000;
  const read = new Set();
  try {
    for (;;) {
      discovery.socket.send(search, ssdpPort, "127.0.0.1");
      await new Promise((resolve) => setTimeout(resolve, 50));
      for (const location of locations) {
        if (read.has(location)) continue;
        read.add(location);
        const description = await (await fetch(location)).text();
        if (description.includes(`<friendlyName>${escapeXml(room)}</friendlyName>`)) {
          return location.replace(/\/desc\.xml$/, "");
        }
      }
      assert.ok(Date.now() < deadline, `no renderer called '${room}' answered a search within 10 s (${[...read]})`);
    }
  } finally {
    discovery.socket.off("message", onMessage);
  }
}

// One AVTransport action, as a control point sends it.
async function act(renderer, action, members) {
  const service = "urn:schemas-upnp-org:service:AVTransport:1";
  const body =
    '<?xml version="1.0"?>\n' +
    '<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" ' +
    's:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/"><s:Body>' +
    `<u:${action} xmlns:u="${service}">${members}</u:${action}></s:Body></s:Envelope>`;
  const response = await fetch(`${renderer}/avt/control`, {
    method: "POST",
    headers: { "Content-Type": 'text/xml; charset="utf-8"', SOAPACTION: `"${service}#${action}"` },
    body,
  });
  const text = await response.text();
  assert.equal(response.status, 200, `the renderer took ${action}: ${text}`);
}

// Tell the renderer of `room` to play `track`, described by `title`, `artist`,
// `album` and `cover` (its artwork's address), as a control point does: set
// the transport's URI with its DIDL-Lite description, then play.
export async function playOnRenderer(discovery, ssdpPort, room, { track, title, artist, album, cover }) {
  const renderer = await rendererOf(discovery, ssdpPort, room);
  const didl =
    '<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" ' +
    'xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/">' +
    `<item id="1" parentID="0" restricted="1"><dc:title>${escapeXml(title)}</dc:title>` +
    `<upnp:artist>${escapeXml(artist)}</upnp:artist><upnp:album>${escapeXml(album)}</upnp:album>` +
    `<upnp:albumArtURI>${escapeXml(cover)}</upnp:albumArtURI>` +
    "<upnp:class>object.item.audioItem.musicTrack</upnp:class>" +
    `<res protocolInfo="http-get:*:audio/wav:*" duration="0:02:00">${escapeXml(track)}</res></item></DIDL-Lite>`;
  await act(
    renderer,
    "SetAVTransportURI",
    `<InstanceID>0</InstanceID><CurrentURI>${escapeXml(track)}</CurrentURI>` +
      `<CurrentURIMetaData>${escapeXml(didl)}</CurrentURIMetaData>`,
  );
  await act(renderer, "Play", "<InstanceID>0</InstanceID><Speed>1</Speed>");
}
