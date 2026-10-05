// The recorder that runs on the browser's audio thread: an
// AudioWorkletProcessor that hands the microphone's samples to the page as
// they are, 32-bit floats, with nothing encoded and nothing dropped
// (docs/decisions, "the recording is taken with an AudioWorklet").
//
// The build bundles this file on its own (build.mjs): an audio worklet is a
// module the browser loads by its address, from this origin, as the page's
// Content-Security-Policy asks of every script.
//
// It takes the first channel of its one input: a phone that hands over two
// channels has two microphones, and summing them would comb the response. The
// samples are gathered into batches of BATCH frames so that the page is sent
// a few messages a second and not one per render quantum (128 frames). A
// batch is a buffer of its own, transferred, so the audio thread copies once
// and allocates once per batch.
//
// Messages from the page: "flush" (send what is held, then answer
// `{ flushed: true }`) and "stop" (the processor ends).

/* global AudioWorkletProcessor, registerProcessor */

export const CAPTURE_PROCESSOR = "chorus-capture";
export const BATCH = 4096;

class ChorusCapture extends AudioWorkletProcessor {
  constructor() {
    super();
    this._held = new Float32Array(BATCH);
    this._count = 0;
    this._running = true;
    this.port.onmessage = (event) => {
      if (event.data === "flush") {
        this._send();
        this.port.postMessage({ flushed: true });
      } else if (event.data === "stop") {
        this._running = false;
      }
    };
  }

  _send() {
    if (this._count === 0) return;
    const samples = this._held.slice(0, this._count);
    this._count = 0;
    this.port.postMessage({ samples }, [samples.buffer]);
  }

  process(inputs) {
    const channel = inputs[0]?.[0];
    if (channel) {
      let at = 0;
      while (at < channel.length) {
        const take = Math.min(channel.length - at, BATCH - this._count);
        this._held.set(channel.subarray(at, at + take), this._count);
        this._count += take;
        at += take;
        if (this._count === BATCH) this._send();
      }
    }
    return this._running;
  }
}

registerProcessor(CAPTURE_PROCESSOR, ChorusCapture);
