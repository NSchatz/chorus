// The microphone, for the room-correction measurement (room-correction.js;
// docs/room-correction.md, "The measurement in the app" and "Per-phone
// limits").
//
// `openMicrophone` asks the browser for the microphone with its processing
// asked off (W3C Media Capture and Streams: `echoCancellation`,
// `noiseSuppression` and `autoGainControl` false, `channelCount` 1), reads
// back what the browser actually granted (`getSettings()`), and records
// uncompressed samples through an AudioWorklet (capture-worklet.js). What it
// resolves to is a capture session:
//
//   { settings, kept, sampleRate, start(), stop(), close() }
//
//   settings    the track's `getSettings()`: what the browser says it gave
//   kept        the processing the browser did not switch off, by constraint
//               name: `keptOn(settings)`
//   sampleRate  the rate of the samples `stop()` gives, in Hz
//   start()     samples are kept from now
//   stop()      resolves to { samples, sampleRate }: every sample since
//               `start()`, mono, as 32-bit floats; then the session is closed
//   close()     the microphone track is stopped and the audio context closed.
//               It can be called at any time and more than once.
//
// That shape is the seam: the screen records through whatever `open` function
// it is given, and a test gives it one that plays back a file. A session from
// another source may name the sweep its recording holds, in what `stop()`
// resolves to: `sweep: { sweepMs, fadeInMs }`.
//
// A microphone that cannot be had ends in a `CaptureError`, whose `reason` is
// one of REASONS and whose message is words for a person. Nothing here waits
// without an end: every wait is the browser's own promise, and the one wait
// on the audio thread (the last samples, at `stop()`) has a deadline.
//
// The page talks to the microphone and to nothing else here: no sample is
// sent anywhere, written to any storage, or kept after `stop()` resolves.

/* global __CHORUS_CAPTURE_WORKLET__ */

/** What the browser is asked for: its three kinds of processing off, one channel. */
export const WANTED = Object.freeze({
  echoCancellation: false,
  noiseSuppression: false,
  autoGainControl: false,
  channelCount: 1,
});

/** The three constraints that are processing, in the order they are shown. */
export const PROCESSING = Object.freeze(["echoCancellation", "noiseSuppression", "autoGainControl"]);

/** The rate the server's fitter takes a recording at (docs/control-plane.md, `POST /api/room-fit`). */
export const UPLOAD_RATE_HZ = 48_000;

/** The name the worklet's processor registers under (capture-worklet.js). */
export const CAPTURE_PROCESSOR = "chorus-capture";

// Where the worklet's module is, from the page: the build fills in the name
// of the file it made (build.mjs). Unbuilt (the tests), it is the source's.
export const WORKLET_URL =
  typeof __CHORUS_CAPTURE_WORKLET__ === "string" ? __CHORUS_CAPTURE_WORKLET__ : "capture-worklet.js";

// How long `stop()` waits for the audio thread to hand over its last samples
// before it goes on with what it has.
export const FLUSH_MS = 1_000;

/** Why there is no recording, and what each is called. */
export const REASONS = Object.freeze({
  insecure:
    "This page was not opened over HTTPS, so the browser gives it no microphone. Open the app at its https address and try again.",
  unsupported: "This browser cannot record uncompressed audio in a page, so it cannot measure a room.",
  denied:
    "The microphone was not allowed. Allow the microphone for this site in the browser's settings for the page, then try again.",
  missing: "The browser found no microphone on this device.",
  busy: "The microphone could not be started. Another app may be using it: close it and try again.",
  failed: "The microphone could not be opened.",
});

export class CaptureError extends Error {
  constructor(reason, detail = "") {
    super(detail ? `${REASONS[reason]} (${detail})` : REASONS[reason]);
    this.name = "CaptureError";
    this.reason = reason;
  }
}

/**
 * The processing a browser kept on: the names among PROCESSING whose setting
 * is anything but `false` where the browser reports it. A setting the browser
 * does not report is in `unreported`, not here: nothing is known of it.
 */
export function keptOn(settings) {
  return PROCESSING.filter((name) => settings?.[name] !== undefined && settings[name] !== false);
}

/** The names among PROCESSING the browser's settings say nothing of. */
export function unreported(settings) {
  return PROCESSING.filter((name) => settings?.[name] === undefined);
}

// A getUserMedia rejection, by the name the specification gives it.
function refusalOf(error) {
  const name = error?.name ?? "";
  if (name === "NotAllowedError" || name === "SecurityError") return new CaptureError("denied");
  if (name === "NotFoundError" || name === "OverconstrainedError") return new CaptureError("missing");
  if (name === "NotReadableError" || name === "AbortError") return new CaptureError("busy");
  return new CaptureError("failed", name || String(error?.message ?? error ?? ""));
}

const stopTracks = (stream) => {
  for (const track of stream?.getTracks?.() ?? []) track.stop();
};

// The audio graph: the microphone's stream into the worklet, at `rate` where
// the browser will make a context at that rate, else at the browser's own.
async function graphOf(env, stream, rate) {
  const Context = env.AudioContext ?? env.webkitAudioContext;
  const context = rate ? new Context({ sampleRate: rate }) : new Context();
  try {
    await context.audioWorklet.addModule(WORKLET_URL);
    const source = context.createMediaStreamSource(stream);
    const node = new env.AudioWorkletNode(context, CAPTURE_PROCESSOR, { numberOfInputs: 1, numberOfOutputs: 1 });
    source.connect(node);
    // The node is connected on to the output so that every browser renders
    // it; the processor writes nothing there, so what it adds is silence.
    node.connect(context.destination);
    if (context.state === "suspended") await context.resume();
    return { context, source, node };
  } catch (error) {
    await context.close?.().catch?.(() => {});
    throw error;
  }
}

/**
 * Open the microphone. `env` is where the browser's objects are read from
 * (the page's globals), so a test can give others.
 */
export async function openMicrophone(env = globalThis) {
  // A page that is not a secure context has no `navigator.mediaDevices` at
  // all: say that, and not "unsupported".
  if (env.isSecureContext === false) throw new CaptureError("insecure");
  const devices = env.navigator?.mediaDevices;
  if (typeof devices?.getUserMedia !== "function") throw new CaptureError("unsupported");
  if (!(env.AudioContext ?? env.webkitAudioContext) || !env.AudioWorkletNode) throw new CaptureError("unsupported");

  let stream;
  try {
    stream = await devices.getUserMedia({ audio: { ...WANTED }, video: false });
  } catch (error) {
    throw refusalOf(error);
  }
  const track = stream.getAudioTracks?.()[0];
  if (!track) {
    stopTracks(stream);
    throw new CaptureError("missing");
  }
  const settings = { ...(track.getSettings?.() ?? {}) };

  let graph;
  try {
    // A context at the upload's rate has the browser do the resampling. A
    // browser that will not make one, or will not join a microphone at
    // another rate to it, gets a context at its own rate, and the page
    // resamples afterwards (toUploadRate).
    try {
      graph = await graphOf(env, stream, UPLOAD_RATE_HZ);
    } catch {
      graph = await graphOf(env, stream, 0);
    }
  } catch (error) {
    stopTracks(stream);
    throw new CaptureError("unsupported", String(error?.message ?? error ?? ""));
  }
  const { context, source, node } = graph;

  let recording = false;
  let closed = false;
  let chunks = [];
  let flushed = null;
  node.port.onmessage = (event) => {
    const data = event.data;
    if (data?.samples && recording) chunks.push(data.samples);
    if (data?.flushed) flushed?.();
  };

  function close() {
    if (closed) return;
    closed = true;
    recording = false;
    chunks = [];
    try {
      node.port.postMessage("stop");
      node.port.onmessage = null;
      source.disconnect();
      node.disconnect();
    } catch {
      // A graph that is already gone has nothing to disconnect.
    }
    stopTracks(stream);
    context.close?.().catch?.(() => {});
  }

  return {
    settings,
    kept: keptOn(settings),
    sampleRate: context.sampleRate,
    start() {
      if (closed) return;
      chunks = [];
      recording = true;
    },
    async stop() {
      if (closed) return { samples: new Float32Array(0), sampleRate: context.sampleRate };
      // The audio thread holds up to a batch: ask for it, and wait no longer
      // than FLUSH_MS for the answer.
      await new Promise((resolve) => {
        const timer = env.setTimeout(resolve, FLUSH_MS);
        flushed = () => {
          env.clearTimeout(timer);
          resolve();
        };
        node.port.postMessage("flush");
      });
      flushed = null;
      const samples = joined(chunks);
      close();
      return { samples, sampleRate: context.sampleRate };
    },
    close,
  };
}

function joined(chunks) {
  const samples = new Float32Array(chunks.reduce((sum, chunk) => sum + chunk.length, 0));
  let at = 0;
  for (const chunk of chunks) {
    samples.set(chunk, at);
    at += chunk.length;
  }
  return samples;
}

// The resampler, for a browser that recorded at another rate than the
// upload's: band-limited interpolation, a sinc windowed by a Kaiser window
// (J. O. Smith, "Digital Audio Resampling Home Page", read 2026-10-05:
// https://ccrma.stanford.edu/~jos/resample/Theory_Ideal_Bandlimited_Interpolation.html
// has the ideal interpolator, a sum of sincs, and for a lower new rate "the
// lowpass cutoff must be placed below half the new lower sampling rate";
// https://ccrma.stanford.edu/~jos/resample/Implementation.html designs the
// finite filter "by the window method based on a Kaiser window"). ZEROS zero crossings each side and BETA are ASSUMED: with them
// a tone in the fit band comes through within a thousandth of full scale
// (test/room-correction.test.js); nothing is claimed for a real recording.
const ZEROS = 16;
const BETA = 8.6;

// The zeroth-order modified Bessel function of the first kind, by its series.
function bessel0(x) {
  let sum = 1;
  let term = 1;
  for (let k = 1; k < 40; k += 1) {
    term *= (x / (2 * k)) ** 2;
    sum += term;
    if (term < sum * 1e-12) break;
  }
  return sum;
}

/** `samples` at `fromHz`, resampled to `toHz`: a Float32Array of round(length * toHz / fromHz) samples. */
export function resample(samples, fromHz, toHz) {
  if (fromHz === toHz) return samples;
  const step = fromHz / toHz;
  // The cut-off as a fraction of the input's Nyquist frequency.
  const cutoff = Math.min(1, toHz / fromHz);
  const half = ZEROS / cutoff;
  const scale = 1 / bessel0(BETA);
  const out = new Float32Array(Math.round((samples.length * toHz) / fromHz));
  for (let n = 0; n < out.length; n += 1) {
    const centre = n * step;
    const first = Math.max(0, Math.ceil(centre - half));
    const last = Math.min(samples.length - 1, Math.floor(centre + half));
    let sum = 0;
    let weight = 0;
    for (let k = first; k <= last; k += 1) {
      const d = centre - k;
      const x = Math.PI * cutoff * d;
      const sinc = x === 0 ? 1 : Math.sin(x) / x;
      const r = d / half;
      const w = sinc * bessel0(BETA * Math.sqrt(Math.max(0, 1 - r * r))) * scale;
      sum += samples[k] * w;
      weight += w;
    }
    // Divided by the weights' sum, a constant comes through as itself at
    // every phase and at the recording's two ends.
    out[n] = weight === 0 ? 0 : sum / weight;
  }
  return out;
}

/** A recording at the rate the server takes: itself at 48 kHz, else resampled to it. */
export function toUploadRate({ samples, sampleRate }) {
  return resample(samples, sampleRate, UPLOAD_RATE_HZ);
}

/**
 * Mono samples as the file the server's route takes (docs/control-plane.md,
 * `POST /api/room-fit`): a RIFF WAVE file, integer PCM, one channel, 16 bits,
 * 48 kHz, as a Uint8Array. A sample is `round(x * 32768)` held to the 16-bit
 * range, the scale the server plays the sweep at, so a sample at or beyond
 * full scale is written as full scale and the fitter's `clipped` sees it.
 */
export function wavOf(samples, rateHz = UPLOAD_RATE_HZ) {
  const bytes = new Uint8Array(44 + samples.length * 2);
  const view = new DataView(bytes.buffer);
  const tag = (at, text) => {
    for (let i = 0; i < 4; i += 1) bytes[at + i] = text.charCodeAt(i);
  };
  tag(0, "RIFF");
  view.setUint32(4, 36 + samples.length * 2, true);
  tag(8, "WAVE");
  tag(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true); // integer PCM
  view.setUint16(22, 1, true); // one channel
  view.setUint32(24, rateHz, true);
  view.setUint32(28, rateHz * 2, true); // bytes a second
  view.setUint16(32, 2, true); // bytes a frame
  view.setUint16(34, 16, true); // bits a sample
  tag(36, "data");
  view.setUint32(40, samples.length * 2, true);
  for (let i = 0; i < samples.length; i += 1) {
    const value = Math.round(samples[i] * 32768);
    view.setInt16(44 + i * 2, Number.isNaN(value) ? 0 : Math.min(32767, Math.max(-32768, value)), true);
  }
  return bytes;
}
