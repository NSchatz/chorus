# 0207: the app records the measurement sweep as uncompressed samples through an AudioWorklet in a 48 kHz context, not with MediaRecorder; it asks the browser's processing off and shows what was granted, converts to the route's WAV in the page, and the screen measures with the room's correction off and puts it back

- Status: accepted, 2026-10-05
- Decided by: the task for the scope (a room-correction flow in the app: guidance, capture with
  the browser's processing asked off and what it granted shown, the sweep played, the recording
  uploaded and fitted on the server, the proposed filters shown and applied, the correction
  switched and undone; "the recording must be uncompressed PCM"). How the samples are taken, at
  which rate, how the file is made, how long the page records, what the screen does about a
  correction that is on, and where the seam for a test is are this record's: each is one module
  or one constant, cheap to reverse.
- Recorded by: the owner's agent harness, in the pull request that adds this record.
- Implemented in: `web/src/capture.js`, `web/src/capture-worklet.js`,
  `web/src/room-correction.js`, `web/src/api.js` (`roomFit`, `roomEqCommand`,
  `roomEqEnabledCommand`, `roomEqUndoCommand`, `measureSweepCommand`), `web/src/state.js`
  (`correctionOf`, `measurementOf`, the store's `roomFit`), `web/src/chorus-app.js`,
  `web/src/room-card.js`, `web/build.mjs`; `web/test/room-correction.test.js`,
  `web/live/room-correction.live.js`; `docs/room-correction.md`, `docs/app.md`.

## Context

The server's half of room correction exists: `measure_sweep` plays the sweep in one room
(`docs/decisions/0195-the-measurement-sweep-plays-on-a-stream-of-its-own.md`), and
`POST /api/room-fit` takes one recording, a 16-bit mono WAV at 48 kHz of at most 2 MiB, fits it
and stores nothing (`docs/decisions/0200-a-recording-is-fitted-and-not-kept.md`). What was
missing is the recording itself: K87's "browser mic access; defeat OS mic processing where
possible", from a page served under a Content-Security-Policy of `script-src 'self'` with no
inline script (`crates/server/src/control.rs`).

The fitter deconvolves the recording with the sweep that was played. A recording that went
through a lossy codec is not the microphone's signal any more, and nothing in the fitter or its
fixtures says what a codec does to a fit, so the samples have to reach the server as the
microphone gave them.

## What was read

In this repository, on 2026-10-05. In full: `docs/room-correction.md`, `web/src/sound.js`,
`web/src/theater.js`, `web/src/api.js`, `web/src/chorus-app.js`, `web/build.mjs`,
`web/test/fake-server.js`, `web/test/sound.test.js` (its first 140 lines), `web/live/sound.live.js`,
`web/live/house.js` (its header, `startHouse`, `until`), `tools/conventions/check-adrs.sh` (the
rule). In part: `docs/control-plane.md` ("Room correction: the `measure_sweep` command", "Room
correction: a recording, its fit and the undo"), `web/src/state.js` (`readRoom`, `createStore`),
`web/src/routes.js` (its header), `web/live/endpoint.js` (its header, `offerLineIn`),
`crates/server/src/control.rs` (`CONTENT_SECURITY_POLICY` and its comment),
`crates/server/tests/measure_sweep.rs` (its header and helpers),
`crates/server/tests/room_fit.rs` (its test names), `docs/app.md` ("The screens", "Phone
check"), `docs/proposals/P5-app-stack.md` (lines 20 to 21 and its microphone lines).

Outside it, each read 2026-10-05 (specifications, vendor documentation, a bug tracker's pages
and MDN's compatibility data; no source file of any browser was opened):

- W3C, Media Capture and Streams, https://www.w3.org/TR/mediacapture-streams/: the
  constrainable properties ("false means that no echo cancellation will take place"; noise
  suppression and automatic gain control each with "There are cases where it is not needed and
  it is desirable to turn it off so that the audio is not altered"; `channelCount`;
  `sampleRate`), `getSettings()` ("returns the current settings of all the constrainable
  properties of the object ... a setting is a target value that complies with constraints, and
  therefore may differ from measured performance at times"), `[SecureContext]` on
  `navigator.mediaDevices` and on `MediaDevices`, and the rejections `NotAllowedError` and
  `NotFoundError`.
- W3C, MediaStream Recording, https://www.w3.org/TR/mediastream-recording/: "If the container
  and codecs to use for the recording have not yet been fully specified, the User Agent
  specifies them".
- WebKit, "MediaRecorder API", https://webkit.org/blog/11353/mediarecorder-api/: "Safari
  currently supports the MP4 file format with H.264 as video codec and AAC as audio codec."
- W3C, Web Audio API, https://www.w3.org/TR/webaudio/: "inputs[n][m] is a Float32Array of audio
  samples for the mth channel of the nth input"; "The AudioContext's render quantum size is the
  default value of 128 frames"; "If contextOptions.sampleRate is specified, set the sampleRate
  of context to this value"; "Implementations MUST support sample rates between 3000 Hz and
  768000 Hz, inclusive"; and of ScriptProcessorNode, "This node type is deprecated, to be
  replaced by the AudioWorkletNode".
- MDN, AudioWorklet, https://developer.mozilla.org/en-US/docs/Web/API/AudioWorklet: "This
  feature is available only in secure contexts (HTTPS), in some or all supporting browsers."
- MDN browser-compat-data, `api/AudioWorklet.json`, `api/AudioContext.json` and
  `api/MediaStreamTrack.json` under
  https://raw.githubusercontent.com/mdn/browser-compat-data/main/: AudioWorklet from Chrome 66,
  Firefox 76 and Safari 14.1, with Chrome Android and Safari on iOS mirroring their desktops;
  the `sampleRate` option of the AudioContext constructor from Chrome 74, Firefox 61 and Safari
  14.1; the per-constraint rows `docs/room-correction.md` ("Per-phone limits") quotes.
- J. O. Smith, "Digital Audio Resampling Home Page",
  https://ccrma.stanford.edu/~jos/resample/Theory_Ideal_Bandlimited_Interpolation.html (the
  ideal interpolator is a sum of sincs; going down in rate "the lowpass cutoff must be placed
  below half the new lower sampling rate") and
  https://ccrma.stanford.edu/~jos/resample/Implementation.html (the finite filter is "designed
  by the window method based on a Kaiser window").
- The sources of "Per-phone limits" in `docs/room-correction.md`, listed there with what each
  says.

## Decision

1. **The samples are taken by an AudioWorklet** (`web/src/capture-worklet.js`): a processor on
   the browser's audio thread that copies its input's first channel, 32-bit floats, into
   batches of 4096 frames and posts each to the page. Nothing is encoded. The page joins the
   batches when the recording ends.
2. **The worklet is a file of its own in the build** (`dist/assets/capture-worklet-<hash>.js`,
   built first; the app's bundle is built knowing its name). The browser loads a worklet by its
   address, and the page's policy allows a script from this origin only, so a module made at
   run time from a string is not an option and is not wanted.
3. **The audio context is asked for at 48 kHz**, the one rate the route takes, so the browser
   resamples the microphone where its own rate differs. A browser that will not make such a
   context, or will not join the microphone to it, gets a context at its own rate, and the page
   resamples afterwards with a Kaiser-windowed sinc (`resample` in `web/src/capture.js`).
4. **The first channel only.** A phone that hands over two channels has two microphones, and
   their sum has a comb the room does not.
5. **The browser is asked for `echoCancellation`, `noiseSuppression` and `autoGainControl`
   false and `channelCount` 1**, as plain values (which a browser may not honour, and which
   cannot make the request fail), and the track's `getSettings()` is shown: each of the three as
   "off, as asked", "on ... the browser kept it on" (flagged, with a sentence that the fit may
   be wrong) or "not reported by this browser". The measurement goes on either way: whether to
   trust it is the person's, told what the browser said.
6. **The file is made in the page** (`wavOf`): the 44-byte header and `round(x * 32768)` held
   to the 16-bit range, so a sample at or beyond full scale is full scale and the fitter's
   `clipped` sees it. No dither is added: the fitter's fixtures have none.
7. **The recording runs from before `measure_sweep` is sent until one second after the server
   says the sweep's program has ended** (`AFTER_MS`), and is given up when the server has said
   nothing of it by the program's length and five seconds more (`GRACE_MS`). The server's word
   is its state's `measurement`, on the event stream the app already reads.
8. **The screen measures with the room's correction off and puts it back.** The route refuses a
   recording of a room whose correction is on. A room that has one switched on has it switched
   off before the sweep and on again when the server has answered the recording, whatever the
   answer, and when the screen is left meanwhile. The apply is then `room_eq` with the filters
   over a correction that is on, so the undo step holds the earlier filters switched on, as
   they were.
9. **Nothing is applied by a measurement.** The proposed filters are shown with the fitter's
   two figures; "Apply" sends them; a fit with no filters offers no apply.
10. **The seam for a test is the session** `openMicrophone` resolves to
    (`{ settings, kept, sampleRate, start, stop, close }`): the screen opens one through its
    `capture` property, and a test gives it a function that resolves to a session of its own. A
    session from a source that is not the microphone may name the sweep its recording holds
    (`sweep: { sweepMs, fadeInMs }`), which is sent as the route's query; the microphone's
    never does, and the route then takes the sweep `measure_sweep` plays.
11. **The recording is held in memory from the sweep to the upload and nowhere else.** The page
    writes it to no storage, sends it to this server's route only, and keeps no sample once the
    route has answered; the microphone's track is stopped when the recording is taken, when a
    step fails and when the screen is left.

## Considered and not chosen

- **MediaRecorder.** The specification leaves the container and codec to the browser, Safari
  records AAC in MP4, and no source read says any phone's browser records PCM with it. A
  compressed recording would also need a decoder on the server, which the route does not have.
- **ScriptProcessorNode.** It gives the same floats with no file of its own, but the
  specification calls it deprecated, and it runs on the page's main thread, where a busy page
  drops audio: a gap in a sweep is a wrong measurement, not a slow one.
- **The microphone's own rate, always resampled in the page.** One code path, but it sets
  aside the resampler every browser already has for a context at a named rate. The page's
  resampler is the fallback.
- **Refusing to measure when the browser kept processing on.** MDN's data says Safari supports
  neither the `noiseSuppression` nor the `autoGainControl` constraint, so every iPhone would be
  refused on what its browser reports and not on what it did (WebKit's bug 179411 says
  `echoCancellation` false disables its gain control too). Shown and flagged, not refused.
- **Uploading 32-bit floats.** The route takes 16-bit PCM, and changing the route is out of
  this task's scope.
- **Leaving the correction for the person to switch off.** The route's `correction_on` would
  then be the first thing most second measurements meet.

## ASSUMED values

- `AFTER_MS` 1000 and `GRACE_MS` 5000 (`web/src/room-correction.js`): how long past the
  server's word the page records, and how long it waits for that word. The room plays the
  sweep's last frame later than the server sends it, by a latency nothing here knows; one
  second is the tail the server itself leaves. No timing is claimed.
- `BATCH` 4096 frames (`web/src/capture-worklet.js`): about a twelfth of a second at 48 kHz.
- `ZEROS` 16 and `BETA` 8.6, the resampler's window (`web/src/capture.js`): with them a tone
  from 100 Hz to 10 kHz comes through within a thousandth of full scale in the unit test.
  Nothing is claimed for a real recording, and the fit of a resampled recording has not been
  compared with the fit of the same recording at 48 kHz.
- That the worklet's node has to be connected to the context's output to be rendered in every
  browser. It writes nothing there.

## Not claimed

No real phone, browser or microphone has recorded anything with this. The unit tests run over
a scripted browser; the live test runs the real server, its real sweep and its real fitter with
the fitter's own synthetic recordings in the microphone's place. What a real phone grants, at
what rate it records, whether the worklet runs in an installed app on iOS, and how a real
recording fits are the first measurement to report (`docs/room-correction.md`, "Per-phone
limits").

## Follow-ups

- The owner's phone check for this screen (`docs/app.md`, "Phone check": the microphone step).
- A level control on the screen (`measure_sweep`'s `volume`), so a `too_quiet` or `clipped`
  measurement is repeated without leaving for the room's card.
- Repeat sweeps and their average, seats, per-model calibration, detrending and clock skew:
  `docs/room-correction.md`, "Follow-ups for the measurement".
