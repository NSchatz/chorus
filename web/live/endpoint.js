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
//
// `updatableSpeaker` is the same session for a speaker that takes firmware
// updates ("Firmware update": the `ota` feature, `firmware_offer`,
// `firmware_chunk`, `firmware_status`). It is a script, not a board and not
// the C endpoint: it keeps the image it is sent in memory, checks its
// SHA-256 and says `verified`, and has no flash, no slots and no bootloader.
// A "restart" is its socket closing, and the next boot is a new session under
// the same key that says what the test tells it to. The real update unit
// against the real server is crates/server/tests/firmware_install.rs.

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
const FIRMWARE_OFFER = 0x18;
const FIRMWARE_CHUNK = 0x19;
const FIRMWARE_STATUS = 0x1a;

const ROLE_PLAYER = 1 << 0;
const ROLE_SOURCE = 1 << 4;
// The kinds of source input ("0x36 source offer"). The last two are a TV's.
const KINDS = { line_in: 1, optical: 2, hdmi_arc: 3 };
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

// `features` is the trailing byte, written only when a bit is set (bit 1 is
// `ota`): without it the payload is what it was before the field existed.
export function capabilities({ codecs, sampleFormats, maxChannels, ratesHz, bufferMs, latencyNs, leds, bands, features = 0 }) {
  const payload = Buffer.alloc(4 + 4 * ratesHz.length + 9 + (features ? 1 : 0));
  payload.set([codecs, sampleFormats, maxChannels, ratesHz.length]);
  let at = 4;
  for (const rate of ratesHz) at = payload.writeUInt32BE(rate, at);
  at = payload.writeUInt16BE(bufferMs, at);
  at = payload.writeUInt32BE(latencyNs, at);
  at = payload.writeUInt16BE(leds, at);
  payload[at] = bands;
  if (features) payload[at + 1] = features;
  return frame(CAPABILITIES, payload);
}

const FEATURE_OTA = 1 << 1;
const FIRMWARE_STATES = ["idle", "receiving", "verified", "pending_verify", "confirmed", "rolled_back", "refused"];
const FIRMWARE_REASONS = ["none", "too_large", "bad_digest", "write_failed", "busy", "wrong_board", "not_confirmed", "bad_offset", "medium_refused"]; // prettier-ignore

// "0x1A firmware status": what runs (`version`, `board`, `slot`) and how the
// transfer it names stands.
export function firmwareStatus({ transfer = 0, state = "idle", reason = "none", received = 0, version, board, slot = 0, imageVersion = "" }) {
  const head = Buffer.alloc(10);
  head.writeUInt32BE(transfer, 0);
  head[4] = FIRMWARE_STATES.indexOf(state);
  head[5] = FIRMWARE_REASONS.indexOf(reason);
  head.writeUInt32BE(received, 6);
  return frame(
    FIRMWARE_STATUS,
    Buffer.concat([head, shortText(version), shortText(board), Buffer.from([slot]), shortText(imageVersion)]),
  );
} // prettier-ignore

// "0x18 firmware offer", read: transfer 0 is the cancel.
function readOffer(payload) {
  const versionEnd = 43 + payload[42];
  return {
    transfer: payload.readUInt32BE(0),
    size: payload.readUInt32BE(4),
    sha256: Buffer.from(payload.subarray(8, 40)),
    chunkBytes: payload.readUInt16BE(40),
    version: payload.subarray(43, versionEnd).toString("utf8"),
    board: payload.subarray(versionEnd + 1, versionEnd + 1 + payload[versionEnd]).toString("utf8"),
  };
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

// Connect to the server's audio port as endpoint `endpoint` and open the
// session: the handshake, then `opening` (the frames of the first record:
// `hello` and `capabilities`), then `after` (a record of its own for what
// the session says right after them, if anything), then wait for the
// server's own `hello`, which is the session being up. `secret` is the
// endpoint's static key (32 bytes); without one the key is fresh, so give
// each server an id once: a second session under the same id with another
// key is a changed key, refused.
//
// Resolves to { socket, record, received }: `record(...frames)` seals whole
// frames into one record, and `received()` resolves to the frames of the next
// record the server sent (every record has to be opened: the counter
// follows).
async function openSession({ host, port, endpoint, secret, opening, after = [], what }) {
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

  // Frames the server sent in the record that carried its `hello`, or before.
  const early = [];
  try {
    await once(socket, "connect");
    const handshake = initiator(keypair(secret), keypair());
    socket.write(frame(HANDSHAKE_INIT, Buffer.concat([PROLOGUE, handshake.first()])));
    const answer = await next();
    if (answer.type === SESSION_REFUSED) throw refusal(answer.payload);
    if (answer.type !== HANDSHAKE_RESPONSE) throw new Error(`a frame of type ${answer.type} for handshake_response`);
    const finished = handshake.finish(answer.payload, shortText(endpoint));
    ({ send, receive } = finished);
    socket.write(frame(HANDSHAKE_FINISH, finished.message3));

    record(...opening);
    if (after.length > 0) record(...after);
    for (;;) {
      const frames = await received();
      early.push(...frames);
      if (frames.some((f) => f.type === HELLO)) break;
    }
  } catch (error) {
    socket.destroy();
    throw new Error(`endpoint ${endpoint} could not ${what} ${host}:${port}: ${error.message}`, { cause: error });
  } finally {
    clearTimeout(limit);
  }
  return { socket, record, received, early };
}

const stopOf = (socket) =>
  async function stop() {
    if (socket.closed) return;
    const closed = once(socket, "close");
    socket.destroy();
    await closed;
  };

// Open a session as endpoint `endpoint` and offer one line-in with a signal
// present. `name` is the offer's name: the server lists the input as
// `<endpoint>/<name>` when the name is lower-case letters, digits and `-`,
// else as `<endpoint>/line-1`, which is what the empty default gives. `kind`
// is what the input is (`line_in`, or a TV's `optical` or `hdmi_arc`), which
// the server's state says back as the input's kind.
//
// Resolves, once the offer has been sent, to { stop }. Until `stop()` the
// session stays up and the input stays offered. `stop()` closes the socket
// and resolves when it is closed, which is the input going.
export async function offerLineIn({ host, port, endpoint, name = "", kind = "line_in" }) {
  if (!(kind in KINDS)) throw new Error(`"${kind}" is not a kind of source input`);
  // `hello` and `capabilities` first, in one record; the offer once the
  // server has said its own `hello`.
  const { socket, record, received } = await openSession({
    host,
    port,
    endpoint,
    opening: [hello(ROLE_PLAYER | ROLE_SOURCE, "", "chorus-web-live-endpoint"), capabilities(CAN_PLAY)],
    what: "offer its line-in to",
  });
  record(sourceOffer(1, KINDS[kind], true, name));

  // The session, kept: every record is opened (the counter has to follow)
  // and dropped, a `source_control` among them. It ends with the socket.
  (async () => {
    for (;;) await received();
  })().catch(() => socket.destroy());

  return { stop: stopOf(socket) };
}

// Open a session as a speaker that takes firmware updates: `capabilities`
// with the `ota` feature, then one `firmware_status` saying what it runs
// (`version`, `board`, `slot`), as every such session opens. `secret` is its
// static key: the same one on its next boot, or the server refuses it for a
// changed key.
//
// What it does with an offer is "0x18 firmware offer" and "0x19 firmware
// chunk" as far as a script with no flash can: it answers `receiving`, takes
// the chunks in order, acknowledges every 16, and when the whole image is
// there says `verified` if its SHA-256 is the offer's (`refused`,
// `bad_digest` if not) and then "restarts", which here is the socket closing.
// The cancel is answered `idle` and what it held is dropped.
//
// `holdAt` is a number of bytes at which it stops taking chunks (they wait,
// unread by the script, as on a speaker that is slow to write), so a test can
// look at a transfer that is under way; `release()` lets it go on.
//
// `trial` is the boot AFTER an install: the transfer id the image arrived
// under. The session then opens `pending_verify` and, once the server's
// `hello` has been read (the session reached its server, which is when an
// image confirms itself), says `confirmed`.
//
// Resolves to { stop, release, statuses, restarted }: `statuses()` is the
// states it has reported, in order, and `restarted` resolves, when a verified
// image made it restart, to { transfer, version, image } (the bytes it was
// sent).
export async function updatableSpeaker({ host, port, endpoint, secret, version, board, slot = 0, holdAt = null, trial = null }) {
  const runs = { version, board, slot };
  const statuses = [];
  const status = (fields) => {
    statuses.push(fields.state ?? "idle");
    return firmwareStatus({ ...runs, ...fields });
  };
  const { socket, record, received, early } = await openSession({
    host,
    port,
    endpoint,
    secret,
    opening: [
      hello(ROLE_PLAYER, "", "chorus-web-live-endpoint"),
      capabilities({ ...CAN_PLAY, features: FEATURE_OTA }),
    ],
    // The server reads `hello` and `capabilities` as the session's opening
    // and everything else after it, so the status is a record of its own.
    after: [status(trial === null ? {} : { transfer: trial, state: "pending_verify" })],
    what: "open an updatable speaker's session with",
  });
  if (trial !== null) record(status({ transfer: trial, state: "confirmed" }));

  // The transfer being received: the offer, the bytes so far, and the chunks
  // that wait while it is held.
  let transfer = null;
  let held = holdAt;
  const waiting = [];
  let restart;
  const restarted = new Promise((resolve) => {
    restart = resolve;
  });
  const about = () => ({ transfer: transfer.offer.transfer, imageVersion: transfer.offer.version, received: transfer.bytes });

  const takeChunk = (payload) => {
    if (!transfer || payload.readUInt32BE(0) !== transfer.offer.transfer) return;
    const offset = payload.readUInt32BE(4);
    const data = payload.subarray(8);
    // A duplicate is ignored; a chunk past a gap is answered with where to resume.
    if (offset < transfer.bytes) return;
    if (offset > transfer.bytes) {
      if (!transfer.gap) record(status({ ...about(), state: "receiving", reason: "bad_offset" }));
      transfer.gap = true;
      return;
    }
    transfer.gap = false;
    transfer.parts.push(Buffer.from(data));
    transfer.bytes += data.length;
    transfer.chunks += 1;
    if (transfer.bytes < transfer.offer.size) {
      if (transfer.chunks % 16 === 0) record(status({ ...about(), state: "receiving" }));
      return;
    }
    const image = Buffer.concat(transfer.parts);
    const good = image.length === transfer.offer.size && sha256(image).equals(transfer.offer.sha256);
    record(status({ ...about(), state: good ? "verified" : "refused", reason: good ? "none" : "bad_digest" }));
    const done = { transfer: transfer.offer.transfer, version: transfer.offer.version, image };
    transfer = null;
    if (!good) return;
    // Verified: it restarts into the image. Here that is the session ending,
    // once the status has left.
    socket.end(() => restart(done));
  };

  const take = (f) => {
    if (f.type === FIRMWARE_OFFER) {
      const offer = readOffer(f.payload);
      if (offer.transfer === 0) {
        // The cancel: give up what is held and say so.
        transfer = null;
        waiting.length = 0;
        record(status({}));
        return;
      }
      if (offer.board !== board) {
        record(status({ transfer: offer.transfer, state: "refused", reason: "wrong_board", imageVersion: offer.version }));
        return;
      }
      if (transfer && transfer.offer.transfer !== offer.transfer) {
        record(status({ transfer: offer.transfer, state: "refused", reason: "busy", imageVersion: offer.version }));
        return;
      }
      // A new transfer, or the same offer again (the resume point).
      transfer ??= { offer, parts: [], bytes: 0, chunks: 0, gap: false };
      record(status({ ...about(), state: "receiving" }));
      return;
    }
    if (f.type !== FIRMWARE_CHUNK) return;
    if (held !== null && transfer && transfer.bytes >= held) waiting.push(f.payload);
    else takeChunk(f.payload);
  };

  // The session, kept: every record is opened and everything but the
  // firmware messages dropped. It ends with the socket.
  (async () => {
    for (const f of early) take(f);
    for (;;) for (const f of await received()) take(f);
  })().catch(() => socket.destroy());

  return {
    stop: stopOf(socket),
    release() {
      held = null;
      for (const payload of waiting.splice(0)) takeChunk(payload);
    },
    statuses: () => [...statuses],
    restarted,
  };
}
