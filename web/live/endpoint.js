// A scripted endpoint for the live tests (`live/*.live.js`): it opens a real
// audio session with chorus-server and offers one line-in, so the server's
// state lists an input no flag could put there. Node's standard library only.
//
// What it speaks is docs/protocol.md: "Frame", "The session: encryption with
// adoption" (Noise_XX_25519_ChaChaPoly_SHA256, the endpoint initiating, then
// `secure_record`s), "Hello and capabilities", and "Source" (`source_offer`,
// `source_control`). It is held to the committed vectors of
// fixtures/protocol/v2 (handshake_init, handshake_finish, secure_record,
// source_offer). The Rust tests' own scripted endpoint is
// crates/server/tests/common/line_in.rs.
//
// It declares the player and source roles and plays nothing: what the server
// streams to it is read and dropped. A `source_control` start is not answered
// (no `stream_format`, no audio), which the protocol allows ("A `start` it
// cannot honour ... the input stays offered"): the group that took the input
// hears silence and the input stays offered. Nothing in the protocol asks a
// session for a keepalive, and the server asks no `time_sync` of it.

import {
  createCipheriv,
  createDecipheriv,
  createHash,
  createHmac,
  createPrivateKey,
  createPublicKey,
  diffieHellman,
  randomBytes,
} from "node:crypto";
import { once } from "node:events";
import { connect } from "node:net";

// Message types (docs/protocol.md, "Message catalog").
const HELLO = 0x10;
const CAPABILITIES = 0x11;
const HANDSHAKE_INIT = 0x20;
const HANDSHAKE_RESPONSE = 0x21;
const HANDSHAKE_FINISH = 0x22;
const SESSION_REFUSED = 0x23;
const SECURE_RECORD = 0x24;
const SOURCE_OFFER = 0x36;

const ROLE_PLAYER = 1 << 0;
const ROLE_SOURCE = 1 << 4;
const KIND_LINE_IN = 1;
const TAG = 16;

// The Noise protocol name is exactly 32 bytes, so it is the first hash as it
// stands. The prologue is `handshake_init`'s magic, version 2 and suite 1.
const NOISE = Buffer.from("Noise_XX_25519_ChaChaPoly_SHA256");
const PROLOGUE = Buffer.from("43485253000201", "hex");
// The DER that wraps a raw X25519 key (RFC 8410): node takes no raw keys.
const PKCS8 = Buffer.from("302e020100300506032b656e04220420", "hex");
const SPKI = Buffer.from("302a300506032b656e032100", "hex");

const NOTHING = Buffer.alloc(0);
const sha256 = (...parts) => parts.reduce((h, p) => h.update(p), createHash("sha256")).digest();
const hmac = (key, ...parts) => parts.reduce((h, p) => h.update(p), createHmac("sha256", key)).digest();

// Noise section 4.3: HKDF with two outputs.
function hkdf2(chainingKey, material) {
  const temp = hmac(chainingKey, material);
  const first = hmac(temp, Buffer.from([1]));
  return [first, hmac(temp, first, Buffer.from([2]))];
}

function keypair(secret = randomBytes(32)) {
  const key = createPrivateKey({ key: Buffer.concat([PKCS8, secret]), format: "der", type: "pkcs8" });
  return { key, public: createPublicKey(key).export({ format: "der", type: "spki" }).subarray(SPKI.length) };
}

function dh(pair, remote) {
  const publicKey = createPublicKey({ key: Buffer.concat([SPKI, remote]), format: "der", type: "spki" });
  return diffieHellman({ privateKey: pair.key, publicKey });
}

// Noise section 5.1: a key and a counter. The 96-bit nonce is 32 zero bits,
// then the counter little-endian. Without a key the text passes unchanged.
class Cipher {
  #key;
  #counter = 0n;

  constructor(key) {
    this.#key = key;
  }

  #nonce() {
    const nonce = Buffer.alloc(12);
    nonce.writeBigUInt64LE(this.#counter++, 4);
    return nonce;
  }

  seal(ad, plain) {
    if (!this.#key) return plain;
    const cipher = createCipheriv("chacha20-poly1305", this.#key, this.#nonce(), { authTagLength: TAG });
    cipher.setAAD(ad, { plaintextLength: plain.length });
    return Buffer.concat([cipher.update(plain), cipher.final(), cipher.getAuthTag()]);
  }

  open(ad, sealed) {
    if (!this.#key) return sealed;
    if (sealed.length < TAG) throw new Error("a sealed text shorter than its tag");
    const cipher = createDecipheriv("chacha20-poly1305", this.#key, this.#nonce(), { authTagLength: TAG });
    cipher.setAAD(ad, { plaintextLength: sealed.length - TAG });
    cipher.setAuthTag(sealed.subarray(sealed.length - TAG));
    return Buffer.concat([cipher.update(sealed.subarray(0, sealed.length - TAG)), cipher.final()]);
  }
}

// The initiator of an XX handshake (Noise sections 5.2 and 7.5):
//   -> e    <- e, ee, s, es    -> s, se
// `first()` is message 1, `finish(message2, payload)` reads message 2 and
// gives message 3, the server's payload and the two transport ciphers.
function initiator(statik, ephemeral) {
  let hash = sha256(NOISE, PROLOGUE);
  let chainingKey = NOISE;
  let cipher = new Cipher(null);
  const mixHash = (data) => {
    hash = sha256(hash, data);
  };
  const mixKey = (material) => {
    let key;
    [chainingKey, key] = hkdf2(chainingKey, material);
    cipher = new Cipher(key);
  };
  const encrypt = (plain) => {
    const sealed = cipher.seal(hash, plain);
    mixHash(sealed);
    return sealed;
  };
  const decrypt = (sealed) => {
    const plain = cipher.open(hash, sealed);
    mixHash(sealed);
    return plain;
  };

  return {
    first() {
      mixHash(ephemeral.public);
      return Buffer.concat([ephemeral.public, encrypt(NOTHING)]);
    },
    finish(message2, payload) {
      if (message2.length < 32 + 48 + TAG) throw new Error(`a handshake_response of ${message2.length} bytes`);
      const theirEphemeral = message2.subarray(0, 32);
      mixHash(theirEphemeral);
      mixKey(dh(ephemeral, theirEphemeral));
      const theirStatic = decrypt(message2.subarray(32, 80));
      mixKey(dh(ephemeral, theirStatic));
      const theirPayload = decrypt(message2.subarray(80));
      const sealedStatic = encrypt(statik.public);
      mixKey(dh(statik, theirEphemeral));
      const message3 = Buffer.concat([sealedStatic, encrypt(payload)]);
      const [send, receive] = hkdf2(chainingKey, NOTHING).map((key) => new Cipher(key));
      return { message3, theirPayload, send, receive };
    },
  };
}

// --- frames and the messages this endpoint sends ("Frame", "Hello and
// capabilities", "0x36 source offer") ---------------------------------------

function frame(type, payload) {
  const header = Buffer.alloc(3);
  header[0] = type;
  header.writeUInt16BE(payload.length, 1);
  return Buffer.concat([header, payload]);
}

function shortText(text) {
  const bytes = Buffer.from(text, "utf8");
  if (bytes.length > 255) throw new Error(`"${text}" is longer than a short text's 255 bytes`);
  return Buffer.concat([Buffer.from([bytes.length]), bytes]);
}

function hello(roles, name, software) {
  const head = Buffer.alloc(4);
  head.writeUInt16BE(2, 0);
  head.writeUInt16BE(roles, 2);
  return frame(HELLO, Buffer.concat([head, shortText(name), shortText(software)]));
}

function capabilities({ codecs, sampleFormats, maxChannels, ratesHz, bufferMs, latencyNs, leds, bands }) {
  const payload = Buffer.alloc(4 + 4 * ratesHz.length + 9);
  payload.set([codecs, sampleFormats, maxChannels, ratesHz.length]);
  let at = 4;
  for (const rate of ratesHz) at = payload.writeUInt32BE(rate, at);
  at = payload.writeUInt16BE(bufferMs, at);
  at = payload.writeUInt32BE(latencyNs, at);
  at = payload.writeUInt16BE(leds, at);
  payload[at] = bands;
  return frame(CAPABILITIES, payload);
}

function sourceOffer(sourceId, kind, signal, name) {
  return frame(SOURCE_OFFER, Buffer.concat([Buffer.from([sourceId, kind, signal ? 1 : 0]), shortText(name)]));
}

// What this endpoint says it can play: PCM in every sample format, up to 8
// channels at the usual rates, so the server's stream, whatever it is
// configured as, negotiates. It plays none of it.
const CAN_PLAY = {
  codecs: 0b001,
  sampleFormats: 0b111,
  maxChannels: 8,
  ratesHz: [44_100, 48_000, 88_200, 96_000, 176_400, 192_000],
  bufferMs: 500,
  latencyNs: 0,
  leds: 0,
  bands: 0,
};

// The frames of one buffer, each { type, payload }. A frame cut short is an
// error: a record carries whole frames.
function framesOf(bytes) {
  const found = [];
  for (let at = 0; at < bytes.length; ) {
    if (bytes.length - at < 3) throw new Error("a record ended inside a frame header");
    const end = at + 3 + bytes.readUInt16BE(at + 1);
    if (end > bytes.length) throw new Error("a record ended inside a frame");
    found.push({ type: bytes[at], payload: bytes.subarray(at + 3, end) });
    at = end;
  }
  return found;
}

// `session_refused`: a reason byte and a long text, a sentence for a person.
function refusal(payload) {
  const detail = payload.length >= 3 ? payload.subarray(3, 3 + payload.readUInt16BE(1)).toString("utf8") : "";
  return new Error(`the server refused the session (reason ${payload[0]}): ${detail}`);
}

// The socket's frames, one at a time: `next()` resolves to { type, header,
// payload } and rejects once the connection is over.
function frameReader(socket) {
  let pending = NOTHING;
  let over = null;
  let wake = () => {};
  socket.on("data", (chunk) => {
    pending = pending.length ? Buffer.concat([pending, chunk]) : chunk;
    wake();
  });
  socket.on("error", (error) => {
    over ??= error;
    wake();
  });
  socket.on("close", () => {
    over ??= new Error("the server closed the session");
    wake();
  });
  return async function next() {
    for (;;) {
      if (pending.length >= 3) {
        const end = 3 + pending.readUInt16BE(1);
        if (pending.length >= end) {
          const found = { type: pending[0], header: pending.subarray(0, 3), payload: pending.subarray(3, end) };
          pending = pending.subarray(end);
          return found;
        }
      }
      if (over) throw over;
      await new Promise((resolve) => {
        wake = resolve;
      });
    }
  };
}

// Connect to the server's audio port as endpoint `endpoint` (a fresh key
// every time, so give each server an id once: a second session under the same
// id is a changed key, refused), open the session, and offer one line-in with
// a signal present. `name` is the offer's name: the server lists the input as
// `<endpoint>/<name>` when the name is lower-case letters, digits and `-`,
// else as `<endpoint>/line-1`, which is what the empty default gives.
//
// Resolves, once the offer has been sent, to { stop }. Until `stop()` the
// session stays up and the input stays offered. `stop()` closes the socket
// and resolves when it is closed, which is the input going.
export async function offerLineIn({ host, port, endpoint, name = "" }) {
  const socket = connect({ host, port });
  socket.setNoDelay(true);
  const next = frameReader(socket);
  const limit = setTimeout(() => socket.destroy(new Error("no session with the server within 10 s")), 10_000);

  let send;
  let receive;
  // One record: whole frames sealed under the record frame's own header.
  const record = (...frames) => {
    const plain = Buffer.concat(frames);
    const header = frame(SECURE_RECORD, NOTHING);
    header.writeUInt16BE(plain.length + TAG, 1);
    socket.write(Buffer.concat([header, send.seal(header, plain)]));
  };
  // The frames of the next record. After the handshake anything that is not
  // a record ends the session, and so does a record that does not open.
  const received = async () => {
    const got = await next();
    if (got.type !== SECURE_RECORD) throw new Error(`a frame of type ${got.type} where a record belongs`);
    const frames = framesOf(receive.open(got.header, got.payload));
    const refused = frames.find((f) => f.type === SESSION_REFUSED);
    if (refused) throw refusal(refused.payload);
    return frames;
  };

  try {
    await once(socket, "connect");
    const handshake = initiator(keypair(), keypair());
    socket.write(frame(HANDSHAKE_INIT, Buffer.concat([PROLOGUE, handshake.first()])));
    const answer = await next();
    if (answer.type === SESSION_REFUSED) throw refusal(answer.payload);
    if (answer.type !== HANDSHAKE_RESPONSE) throw new Error(`a frame of type ${answer.type} for handshake_response`);
    const finished = handshake.finish(answer.payload, shortText(endpoint));
    ({ send, receive } = finished);
    socket.write(frame(HANDSHAKE_FINISH, finished.message3));

    // `hello` and `capabilities` first, in one record; the offer once the
    // server has said its own `hello`, which is the session being up.
    record(hello(ROLE_PLAYER | ROLE_SOURCE, "", "chorus-web-live-endpoint"), capabilities(CAN_PLAY));
    while (!(await received()).some((f) => f.type === HELLO));
    record(sourceOffer(1, KIND_LINE_IN, true, name));
  } catch (error) {
    socket.destroy();
    throw new Error(`endpoint ${endpoint} could not offer its line-in to ${host}:${port}: ${error.message}`, {
      cause: error,
    });
  } finally {
    clearTimeout(limit);
  }

  // The session, kept: every record is opened (the counter has to follow)
  // and dropped, a `source_control` among them. It ends with the socket.
  (async () => {
    for (;;) await received();
  })().catch(() => socket.destroy());

  return {
    async stop() {
      if (socket.closed) return;
      const closed = once(socket, "close");
      socket.destroy();
      await closed;
    },
  };
}
