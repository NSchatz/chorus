# The app

The chorus app is the installable web app a household uses on a phone, a desktop and a wall
tablet: rooms, groups, volume, inputs and what is playing (part 1), and each room's sound,
limits, theater settings and room correction, the house's autoplay rules, its alarms, stored
sources and sleep timers, and its speakers with their setup and their firmware (part 2, built
2026-10-05). This page says what was built and how to check it. Its source is `web/`
(`web/README.md` is the map of the files and the rules a change there follows); the decisions
behind it are records 0181 to 0191, 0197 and 0207 in `docs/decisions/`, named where each applies
below. The stack is the owner's choice of proposal P5, Option C
(`docs/proposals/P5-app-stack.md`, K43): Lit 3 elements bundled by esbuild, tested with node's own
runner over happy-dom, with one browser test.

Nothing on this page is a measurement, and nothing here has been seen on a phone: what an agent
can show is a headless Chromium on a development host. What only the owner's phones can show is
the "Phone check" section at the end, and what only the owner's phones, a real speaker and the
owner's bench can show of part 2 is "Phone check, part 2" after it. Both are the owner's (K4), and
each has an item in the owner's queue. "Part 2 as built, and what was never run on real hardware"
is the list of what stays unproven until then.

## Where it is served

`chorus-server` serves the app itself, under `/app/` on its control listener, from files compiled
into the binary (`docs/decisions/0182-the-app-is-served-under-app.md`; the routes and headers are
in `docs/control-plane.md`, "The app under `/app/`").

- `GET /app/` and `GET /app/index.html` are the document; `GET /app/<path>` is the file of
  `web/dist` at that path; `GET /app` answers `308` to `/app/`; anything else under `/app/` is a
  `404`.
- `/app/` is permanent. An installed app keeps its start address and its scope, so the path does
  not move, also when the app later replaces the control page at `/` (`docs/control-page.md`).
- `web/dist` is the build's output, committed. `crates/server/build.rs` compiles it in with the
  standard library alone, so the server, the image and CI's Rust build need no node.
- The document, `sw.js` and the manifest are served `no-cache`; a file named by the hash of its
  content (`assets/<name>-<hash>.<ext>`) is served `immutable` for a year. Every file has a
  strong `ETag` and is answered `304` when the browser already holds it.
- Every response carries the server's one Content-Security-Policy, the control page's with
  `manifest-src 'self'` added: no inline script, no inline style, images from this origin only.

The server has no authentication and the app adds none (K40). A household reaches it through its
reverse proxy: HTTPS, local network only, behind the household login. HTTPS is not optional for
the app: a service worker, an install and the screen wake lock exist only in a secure context, so
the app opened on the server's plain HTTP port from another machine works as a page and installs
nothing.

## The screens

One document and one element (`chorus-app`). Its home is two regions, the groups and the rooms:
the navigation ("Groups", "Rooms") brings a region to the top and puts the focus on it. Every
further screen has an address of its own (below).

| Region | What it shows | What it does |
|---|---|---|
| Groups | every saved group, then every live group, each with its rooms, what it plays and what is playing | group volume (the server scales every room and keeps their ratio, K77); "Remove" a room; "Group these rooms" for a saved group that is not formed; choose an input |
| Rooms | every room with its bonded set, its volume and its mute; a room alone also says what it plays and what is playing | volume, mute; move the room by dragging its handle onto a room or a group, or with the "Plays with" list on its card; choose an input; open the room's sound screen ("Sound"), its limits screen ("Limits"), its room correction ("Correction") and, for a room with a TV input or a theater set, its theater screen ("Theater"). Under the rooms, "Autoplay" opens the house's autoplay rules and "Alarms" its alarms, stored sources and sleep timers |

**Further screens and their addresses** (`docs/decisions/0197-further-screens-have-an-address-in-the-fragment.md`).
A screen beyond the home has an address in the fragment, `#/` and a path, and is painted alone
in the main region, across both columns on a desktop, under a "Back" link:

| Address | Screen |
|---|---|
| `/app/` or `/app/#/` | the home: the groups and the rooms |
| `/app/#/rooms/<room id>/sound` | the sound of that room |
| `/app/#/rooms/<room id>/limits` | the volume limit and the quiet hours of that room |
| `/app/#/rooms/<room id>/theater` | the theater settings of that room: A/V trim, TV autoplay, TV upmix, bass management |
| `/app/#/rooms/<room id>/correction` | the room correction of that room: measure it with this device's microphone, apply, switch, undo |
| `/app/#/autoplay` | the house's autoplay rules, one for each input |
| `/app/#/alarms` | the house's alarms, its stored sources and its sleep timers |
| `/app/#/speakers` | the adopted speakers: names, rooms, presence, and any refused changed key |
| `/app/#/speakers/setup` | the walk-through for a compact Wi-Fi speaker |

- Opening a screen from the app (the "Sound", "Limits" or "Correction" link on a room's card, "Autoplay",
  "Alarms" or "Speakers" under the rooms, "Set up a Wi-Fi speaker" on the speakers screen) is a new entry of the
  browser's history, so the browser's back button, a phone's back gesture and the forward
  button work as on any site, and an address can be bookmarked, reloaded or sent to another
  person of the household.
- "Back" on the screen is that same one step back. Where the screen was the address the app was
  opened at, there is nothing of the app to step back to, and "Back" puts the home in its place.
- The navigation's "Groups" and "Rooms" leave a further screen for the region they name.
- An address that names no screen is the home.
- The fragment is never sent to the server, so the login, the service worker and the kiosk's
  `?kiosk` switch are untouched by it: `/app/?kiosk#/rooms/living/sound` is a kiosk on that
  screen.
- A later screen registers itself with `registerScreen` in `web/src/routes.js` (an id, a path
  such as `rooms/:room/limits` or `alarms`, a title and what it renders) and is linked with
  `<a href=${addressOf(id, params)} data-route>`; nothing else of the shell changes.

**A room's sound** (`web/src/sound.js`; the catalog's `sound` command, `docs/control-plane.md`,
"Per-room sound"):

| Control | What it is | What it sends |
|---|---|---|
| Bass, Treble | a slider each, whole dB from -10 to 10, with the value beside it ("+3 dB") | `sound` with `bass` or `treble` alone, when the slider is let go |
| Loudness, Night mode, Speech enhancement | a button each, pressed when on, with "On" or "Off" beside it | `sound` with `loudness`, `night` or `speech` alone: the opposite of what the server holds |

Each control sends one command that carries its one field, so a setting another client has just
changed is never written back from a page that had not yet heard. Nothing changes on the screen
until the server's answer says so; a refused command is shown as "Refused (<field>): <the
server's words>"; a setting the state does not carry is "Unavailable". A change made anywhere
else appears on the screen with no reload, except under a slider a person has hold of. Bass
management and the TV upmix are on the room's theater screen (below); a room's limits have a
screen of their own.

**A room's volume limit and quiet hours** (`web/src/limits.js`; the catalog's `limit`,
`quiet_hours` and `quiet_hours_enabled`, `docs/control-plane.md`, "The commands catalog version
2 adds", and `docs/decisions/0150-quiet-hours-switched-off-and-on-per-room.md`):

| Control | What it is | What it sends |
|---|---|---|
| Volume limit | a slider, 0 to 100%, with the value beside it | `limit`, when the slider is let go |
| Limit in force now, Volume now | two figures, the server's `effective_limit` and the room's volume as the server holds it under that limit | nothing: the app never works a limit out |
| Quiet hours | a button, pressed when on, with "On" or "Off" beside it | `quiet_hours_enabled`: the opposite of what the server holds. Off, the windows are kept and cap nothing |
| Window 1 to 8 | each window's seven days (a button a day, pressed on the days it starts on), "From" and "Until" (a time each), its limit (a slider) and "Remove"; beside its number, "Active now", "Not active now" or, when quiet hours are off, "Inside it now, and quiet hours are off", from the state's `active` | `quiet_hours` with every window of the room and that one change made, since the command replaces the whole list |
| Add a window | a draft: its days, "From", "Until", its limit and "Add window". It starts as every day, 22:00 to 07:00, at 25% | nothing until "Add window", which sends `quiet_hours` with the room's windows and the draft after them. At 8 windows (the catalog's most) the draft gives way to the words that say so |

A window whose end is not after its start runs past midnight and belongs to the days it starts
on, as the catalog says. Which window is active is the server's answer on its own civil clock,
never the phone's. A change made to a window before the server has answered the last one (three
days tapped one after another) is made to the list the last one asked for, so it does not undo
it; nothing of that is shown before the server says it. What the server refuses (a window that
starts and ends at the same minute, a window with no day) is shown as "Refused (windows): <the
server's words>", and the control goes back to the server's value. A volume asked for above the
limit in force is clamped by the server, and the room's card and this screen show the clamp.

**Autoplay** (`web/src/autoplay.js`; the catalog's `autoplay` command and the state's
`autoplay`, `docs/control-plane.md`): one entry for each input the state names, the ones
offered now in the server's order, then any that has a rule and is not offered now (its speaker
is not connected; "Not offered now"). An input is called by the name a person gave it, with its
id under it.

| Control | What it is | What it sends |
|---|---|---|
| Autoplay | a button, pressed when the rule is on, with "On" or "Off" beside it. An input with no rule has no target to play in, so the button waits: "Choose where it plays, then switch it on" | `autoplay` for that input with its target as the server holds it and `enabled` the opposite |
| Plays in | a list of the rooms and the saved groups, on the rule's target ("Nowhere yet" for an input with no rule; a target the server no longer has is still named) | `autoplay` for that input with the chosen target and `enabled` as the server holds it; for an input with no rule that makes the rule, switched off |

Each change is one `autoplay` command for its input, and neither control changes what the other
shows. A rule's TV options (`stop_on_standby`, `low_latency`) are not on this screen but on
the theater screen of the TV input's room (below); a rule that has one switched off keeps it
through a change made here. A refusal is shown on the input
it was for.

**A room's theater settings** (`web/src/theater.js`; the catalog's `av_trim`, `autoplay`,
`sound` (`tv_upmix`) and `bass_management`, `docs/control-plane.md`, "The TV path" and "Per-room
sound"). The screen is offered where it applies, as a "Theater" link on the room's card, and a
room it does not apply to has no link:

- a room with a **TV input**: an input the state says is a TV's (`input_kinds`, kind `optical`
  or `hdmi_arc`) that an endpoint of the room offers, or that plays in the room by its autoplay
  rule. Only an input offered now has a kind, so a room whose hub is not connected, and that has
  no theater set, has no link until it is;
- a room whose **bonded set is more than a pair**: it has a centre, a sub or surrounds.

Opened by its address for any other room, it says the room has no TV input and no theater set,
and has no control.

| Part | Control | What it is | What it sends |
|---|---|---|---|
| A/V trim | A/V trim | a slider, whole milliseconds over the catalog's range, -100 to 200, with the value beside it ("+40 ms"). Later delays the room's TV sound, earlier brings it forward | `av_trim`, when the slider is let go |
| | 1 ms earlier, 1 ms later | a button each, for the step a slider over three hundred values is too coarse for; disabled at the end of the range it would step past | `av_trim` with the server's value one lower or one higher |
| TV autoplay | Autoplay, Plays in | the autoplay screen's own controls (above), for each TV input of the room and for no other input, with what the input is ("Optical", "HDMI ARC"). Here an input with no rule does not wait for a target: the switch makes the rule, playing in this room | `autoplay` for the TV input: its whole rule with the one change |
| | Stop on standby, Low latency | a button each, pressed when on, with "On" or "Off" beside it. Both are on unless the rule says otherwise, as the catalog writes them. On an input with no rule, one of them makes the rule for this room, switched off | `autoplay` for the TV input: its target and its switch as the server holds them, and both options with the one changed (the catalog writes an option only when it is off) |
| TV upmix | Off, Ambient | a button each, the server's one pressed: what the surround speakers of a theater set play from a TV in stereo. A set with no surround speakers is told so, and can still set it | `sound` with `tv_upmix` alone |
| Bass management | Crossover | a slider, whole Hz from 40 to 200 ("80 Hz") | `bass_management` with `crossover_hz` alone |
| | Sub level | a slider, -12 to 6 dB in half-dB steps, with the server's value beside it to two places ("-3.50 dB") | `bass_management` with `sub_level_db` alone, written with two places |
| | Sub polarity | "Normal" and "Inverted", the server's one pressed | `bass_management` with `sub_polarity` alone |

Bass management is the sub's. A room whose set has no sub (the server's own `active`, which is
"the set has an `LFE` member") shows no control for it and says so: "This room's set has no sub".
As on the sound screen, each control sends one command for its one change, nothing changes on
the screen until the server's answer says so, a refused command is shown as "Refused (<field>):
<the server's words>" (a refused `autoplay` on its input), a setting the state does not carry is
"Unavailable", and a change made anywhere else appears with no reload, except under a slider a
person has hold of. The screen shows the numbers a person sets and the server holds. It says
nothing about how well picture and sound line up: that is set by eye and ear, and nothing here
measures it.

**A room's correction** (`web/src/room-correction.js`, `web/src/capture.js`; the catalog's
`measure_sweep`, `room_eq` and `room_eq_undo` and the route `POST /api/room-fit`,
`docs/control-plane.md`, "Room correction: the `measure_sweep` command" and "Room correction: a
recording, its fit and the undo"; `docs/room-correction.md`, "The measurement in the app" and
"Per-phone limits"; `docs/decisions/0207-the-room-is-recorded-with-an-audio-worklet.md`). Every
room's card has a "Correction" link. The screen measures the room with the microphone of the
device it is open on, shows what the server's fitter proposes, and applies it only when told to.
The measurement is a walk, each step a press:

| Step | What the screen shows | What happens |
|---|---|---|
| 1. Guidance | where to hold the phone (the listening place, ear height, microphone uncovered, held still), a quiet room, what will play (half a second of silence, one rising tone of 5 seconds from the lowest bass to the highest treble, a second of silence, in this room only, at this room's volume), that the recording is kept nowhere, and that a phone's microphone is not calibrated | nothing is asked of the browser or the server yet |
| 2. "Use the microphone" | "What the browser granted": echo cancellation, noise suppression and automatic gain control, each "off, as asked", "on ...: asked off, and the browser kept it on" (flagged, with "a fit of this recording may be wrong") or "not reported by this browser"; the channels; the microphone's sample rate; the rate recorded at | the browser is asked for the microphone with the three kinds of processing off and one channel; the browser's own permission prompt appears |
| 3. "Play the sweep and record" | "The sweep is playing and the microphone is recording. Hold still.", then "The recording is with the server, being fitted. The microphone is off." | a correction that is on is switched off (`room_eq`, `enabled` false); `measure_sweep`; the recording ends one second after the server says the sweep finished and is sent to `POST /api/room-fit`; the correction is switched back on if it was |
| 4. "Apply" or "Discard" | "Proposed filters", each as "45 Hz, -9.32 dB, Q 6.409", and the fitter's own two figures (the deviation before, and predicted after); "Nothing has changed in the room yet." | "Apply" sends `room_eq` with the filters, switched on; "Discard" sends nothing. A fit with no filters says there is nothing to correct and offers no apply |

Every way a step can fail ends in words and a "Measure again" button, never in a screen that
waits:

- **No microphone.** Opened over plain HTTP: the page is not a secure context, the browser
  gives it no microphone, and the screen says to open the app at its https address. A denied
  permission, no microphone on the device, a microphone another app holds, and a browser that
  cannot record uncompressed audio each have their own sentence.
- **No sweep.** The server's refusal of `measure_sweep` is shown in its words (the room is
  muted, has no speaker, an alarm rings, another room is being measured). A sweep the server
  calls off, or whose end it does not report within the sweep's own length and five seconds,
  ends the measurement with nothing uploaded.
- **A recording the fitter refuses** is shown under the fitter's name and in its words, which
  carry the numbers, with what to do about that one: `too_short`, keep the screen open and the
  phone awake until the sweep has finished; `clipped`, turn the room down or hold the phone
  further away; `too_quiet`, turn the room up, move closer, uncover the microphone; `too_noisy`,
  pause music elsewhere, quiet the room, or turn the room up. Nothing is applied and there is
  no apply to press.

Below the walk is the room's correction as the server holds it: its filters, a "Correction"
switch (`room_eq` with `enabled` alone; pressed when on, disabled for a room with no filters)
and "Undo" (`room_eq_undo`: the correction as it was before the last apply; disabled when the
state says there is nothing to undo). As on every screen, what is shown is the server's state,
a refused command is shown in the server's words, and a change made anywhere else appears with
no reload.

The recording is in the page's memory from the sweep to the upload. It is sent to this
server's one route and nowhere else, written to no storage, and not kept once the route has
answered; the service worker neither answers nor keeps anything of the API
(`web/test/worker.test.js`). The microphone is stopped when the recording is taken, when a step
fails, on "Stop", and when the screen is left; a room whose correction was switched off for a
sweep gets it back on when the screen is left meanwhile.

**The correction screen has never measured a room.** `web/test/room-correction.test.js` is the
screen over a scripted browser (a faked `navigator.mediaDevices`, audio context and worklet) and
a scripted server; `web/live/room-correction.live.js` is the screen against a real
`chorus-server`, its sweep and its fitter, with the fitter's synthetic recordings
(`fixtures/roomfit/`) in the microphone's place. No phone, no microphone and no room is in
either, the screen says nothing about how well a room is corrected, and what each phone's
browser does with the three constraints is in `docs/room-correction.md`, "Per-phone limits",
as read and not as seen.

**Alarms, stored sources and sleep timers** (`web/src/alarms.js`; the catalog's `alarm_set`,
`alarm_delete`, `alarm_stop`, `sleep`, `source_store` and `source_forget`,
`docs/control-plane.md`; the four alarm sources, `docs/inputs.md` and
`docs/decisions/0129-stored-alarm-sources-line-in-sharing-and-streamer-inputs.md`, K80). One
screen in three parts.

| Part | What it shows | What it does |
|---|---|---|
| Alarms | every alarm of the state: its name (the catalog's id), its time and its days ("once" for an alarm with no day), what it plays and where, its volume, its rise and how long it plays; "Ringing now" while the state says `ringing` | "Alarm", a button pressed when the alarm is on: `alarm_set` with the alarm as the server holds it and `enabled` the opposite. "Stop", shown only while it rings: `alarm_stop`. "Edit": copies the alarm into the form below and sends nothing. "Delete": `alarm_delete` |
| Set an alarm | a draft: its name, where it rings (a room or a saved group), its time, a button for each day (none pressed is once, at the next such time), what it plays, its volume (a slider), "Rises over, seconds" (0 to 600), "Plays for, minutes" (0 to 720; 0 plays until it is stopped) and "Switched on". It starts as 07:00 on Monday to Friday, at 30%, rising over 30 s, for 60 min, in the first room, playing the first source offered | nothing until "Save alarm", which sends one `alarm_set` with every field. The catalog's command creates or replaces a whole alarm: saving under the name of an alarm that exists replaces it (the form says so), and under another name makes a second one |
| Stored sources | every stored source of the state: its name, its kind ("Stream URL" or "Spotify URI"), its id and its value | "Forget": `source_forget`. The server refuses while an alarm plays the source, and its words, which name the alarm, are shown on that source |
| Store a source | a draft: the kind, an id, a name and the address (an `http://` or `https://` stream) or the Spotify URI (`spotify:playlist:<id>`, or an album, a track or an episode) | nothing until "Store source", which sends `source_store`. A value the server refuses is shown as "Refused (value): <the server's words>", which name what it takes (a scheme that is not `http://` or `https://` is refused there, not when the alarm rings), and what was written stays in the form to be put right |
| Sleep timers | every timer of the state, by the name of its room or its group, with the time left ("17 min 22 s left") | "Cancel": `sleep` with 0 minutes |
| Start a sleep timer | a room or a group formed now, and the minutes (0 to 720) | "Start": `sleep`. 0 minutes cancels the timer the target has |

**What an alarm plays** is one of four kinds, and the picker lists what the state names of each,
under a heading for the kind. The app has no list of its own: the chime names are the state's
`chimes` (`docs/chimes.md`), read from the server's own build.

| Kind | What the picker offers | The source it sends | What the server needs to play it |
|---|---|---|---|
| A chime | every name in the state's `chimes` | `chime:<name>` | nothing more: every server that runs the schedule has the chimes |
| A line-in | every input offered now (the state's `inputs`), by the name a person gave it | `line-in:<endpoint>/<input>` | the input offered when the alarm rings: its speaker connected |
| A stored stream URL | every stored source of kind `url`, by its name | `stored:<id>` | **a network media player**: the server started with `--players` (one more than the casts it expects at that hour), and a URL its fetch policy allows (`docs/streams.md`) |
| A stored Spotify URI | every stored source of kind `spotify`, by its name | `stored:<id>` | **a Spotify receiver for the alarm's room or saved group, and the alarm switch**: the server started with `--soloist-receivers` and with `--soloist-alarms`, which is off by default and the owner's to turn (`docs/inputs.md`, "The Spotify playlist source"; `docs/soloist.md`), the receiver running, and a person of the household having chosen that room's device in the Spotify app once |

An alarm is never refused for a source the server cannot play that morning: it is set, and it
rings the `bell` chime instead, with the reason in the server's log (`docs/inputs.md`). So the
screen says what the state lets it know beforehand, on the alarm and under the picker, each
ending "rings the bell chime instead":

| The screen says | When the state says |
|---|---|
| "This server does not say which chimes it has" | it has no `chimes` (a server that runs no schedule); the picker then offers no chime |
| "This server has no chime ..." | the alarm names a chime that is not in `chimes` |
| "The input ... is not offered now: its speaker is not connected" | the alarm's line-in is not in `inputs` |
| "This server has no stored source ..." | the alarm names a stored source that is not in `stored_sources` |
| "This server runs no Spotify receiver" | it has no `soloist` member, which a server started without `--soloist-receivers` does not write |
| "No Spotify receiver is running for ..." | `soloist.receivers` has no receiver in the state `running` whose target is the alarm's room or saved group |

Two things the server needs are **not in the state**, so the screen cannot say them and offers
the source without a warning: whether the server has a player (`--players`) for a stream URL,
and whether Spotify alarms are switched on (`--soloist-alarms`). On a server without them such
an alarm is set, shows no warning, and rings the bell (`no-players`, `soloist-off` in the log).

A sleep timer's time left is the server's count (`remaining_s`, ADR 0194). The server sends a
state for the countdown alone only when the whole minutes change, so between two states the
screen counts the seconds down itself, on the browser's monotonic clock, and takes every state's
value as the truth; the entry leaves the screen when it leaves the state, not when the screen's
own count reaches zero. On a server that does not count (no `remaining_s`) it shows the minutes
asked for. There is no snooze: the catalog has no command for one.

**Speakers** (`web/src/speakers.js`; the catalog's `speaker_name`, `speaker_room` and
`speaker_forget` and the state's `speakers` and `key_changes`, `docs/control-plane.md`,
"Speakers: adoption, names and rooms"; `docs/decisions/0106-speakers-adopted-named-and-assigned-rooms.md`).
Adoption is the server's and is automatic (trust on first use, R11): a speaker's first session
pins its key and lists it, unnamed and in no room. The app adopts nothing; this screen is where
that speaker surfaces. It lists every speaker of the state, in the server's order, each with its
name, its id, whether a session of it is up ("Connected" or "Not connected"), its link ("Wired",
"Wi-Fi", or "Not reported" for an endpoint with no control client), the software its latest
`hello` named and the fingerprint of its pinned key. A speaker with `named: false` and
`room: null` is marked "New: adopted, not named and in no room yet", and stops being new when
it has either.

| Control | What it is | What it sends |
|---|---|---|
| Name, "Save name" | a text field holding the server's name, and a button (Enter in the field does the same). The field is the one thing on the screen that holds what a person types, until the server has it | `speaker_name` with the name as typed, less the spaces around it. An empty name, and the name a person already gave, send nothing; the name the server made for an unnamed speaker can be saved as it stands |
| Room | a list of "No room" and the rooms, on the speaker's assigned room (a room the server no longer has is still named) | `speaker_room` with the room, or with `"room":null` for "No room" |
| Forget | a button that asks first ("Yes, forget it", "Keep it"), saying what goes: the name, the room and the pinned key | `speaker_forget`, on the second press only |

Each sends one command, in the bytes of its vector in `fixtures/control/v2`, and the screen
changes when the state that resulted comes back. A refusal is shown on the speaker it was for,
in the server's words: moving or forgetting a speaker that plays in a room's bonded set says
"speaker '<id>' plays in room '<room>''s bonded set; unbond room '<room>' first".

**A changed key is shown as the refusal it is.** For each entry of `key_changes` the screen
says, above the speakers, that a session under that id offered a key other than the one the id
is pinned to, that the server refused it and that the pin did not move, with both fingerprints
("Pinned key", "Offered key, refused"). The entry has no control: nothing in the app accepts the
offered key, and the catalog has no command that would. What it says instead is the owner's one
way past, which is the server's: forgetting the speaker (the "Forget" button on its own row,
which asks first), after which the next session under that id is adopted as a new speaker. An
entry for an id that is not among the listed speakers says that nothing on the screen can forget
it.

**Firmware: "update available" and the explicit install** (`web/src/speakers.js`; the catalog's
`firmware_install`, `firmware_cancel` and `firmware_rescan` and the state's `firmware`,
`docs/control-plane.md`, "Firmware: staged images and explicit installs";
`docs/firmware-updates.md`; `docs/decisions/0110-explicit-firmware-installs.md`). A speaker that
reported what it runs has a "Firmware" part on its row: the version, the board profile and the
slot, and one line for its `firmware.state`, in words, with the server's `reason` after it
("Reason: not_confirmed.") whenever it is not `none`:

| `firmware.state` | What the row says |
|---|---|
| `idle` | "No install is in progress." |
| `requested` | "Install requested: the server is offering image <name> (version <v>) to the speaker." |
| `receiving` | "Receiving image <name> (version <v>): <received> of <size> bytes.", with a progress bar |
| `verified` | "Written and checked: image <name> (version <v>). The speaker restarts into it." |
| `pending_verify` | "On trial: the speaker runs version <v> and has not confirmed it yet." |
| `confirmed` | "Installed: image <name> (version <v>) confirmed itself, and the speaker runs version <v>." |
| `rolled_back` | "Rolled back: version <tried> did not confirm, and the speaker runs version <v> again. Nothing retries it." |
| `refused` | "Refused by the speaker: image <name> (version <v>) was not installed." (the reason is the speaker's: `too_large`, `wrong_board`, `bad_digest`, ...) |
| `interrupted` | "Interrupted: the install of image <name> (version <v>) did not finish and is not resumed. Install again to start over." (`session_ended`, `not_resumed`) |
| `cancelled` | "Cancelled: the install of image <name> (version <v>) was abandoned." |

A state a later server adds is said in the server's own word. A speaker with no `firmware` (it
takes no updates) has no such part.

"Update available" is the server's `update_available` and nothing the app works out: it is
shown for a speaker whose flag is true and for no other, with one line for each staged image
the flag is about (verified, of the speaker's board, of another version than it runs), saying
that image's `version` and `name` from `firmware.images`. Below the speakers, "Firmware images"
lists every staged image with its version, its board and the server's verdict ("Verified", or
"Refused: <reason>. It is never offered to a speaker.").

| Control | What it is | What it sends |
|---|---|---|
| Install | a button beside one image on one speaker's row. It asks first, naming the image, its version, the speaker and its id, and saying what the speaker runs now ("Yes, install it", "Not now"). Disabled while the speaker is not connected; absent while an install is in progress | `firmware_install` with that `speaker` and that `image`, on "Yes, install it" only |
| Cancel install | a button on the row while the state is `requested` or `receiving` | `firmware_cancel` with the speaker |
| Rescan | a button under the staged images | `firmware_rescan` |

**Nothing installs without that explicit action (K93, R11).** Opening the screen, a new state,
an update appearing, a rescan, a reload and the question itself send no `firmware_install`; the
unit test holds the scripted server's command log empty until "Install" has been pressed and
confirmed, and then to exactly one command. The app never sends the catalog's `"all": true` or
`"force": true`, has no automatic install of any kind, and does not upload an image: images are
staged as files in the server's firmware directory (`docs/firmware-updates.md`). A question
about an update that has meanwhile gone from the state is withdrawn.

**The app's install and the owner's bench variable.** The app sends the same command a `curl`
would, and the server decides. An install to a speaker that is not on the server's own host
writes to a real device, which is the owner's action: unless the server's environment holds
`CHORUS_OWNER_AT_BENCH` set to `1`, the server refuses it as `owner-not-at-bench` and sends the
speaker nothing. The app has no way to set, read or get round that variable, and does not hide
"Install" for it either (the state does not say whether it is set): the press is refused and
the row says so in the server's words, "Refused: owner-not-at-bench: ...". Every other refusal
is shown the same way, by the name the server's detail starts with (`unknown-image`,
`image-not-verified`, `speaker-absent`, `not-updatable`, `busy`, `wrong-board`,
`already-running`; `nothing-to-cancel` for a cancel; `no-firmware-dir` for a rescan). With the
variable set, pressing and confirming is still what starts an install: the variable allows one,
it never starts one.

A server started without `--firmware-dir` writes no `firmware` in its state. The screen then
shows what each speaker runs and no update control at all: no "Update available", no "Install",
no "Cancel install", no list of images and no "Rescan".

**Nothing here ran on a board.** `web/test/firmware.test.js` is the screen over a scripted
server. `web/live/firmware.live.js` is the screen against a real `chorus-server` with a staged
image, and its speaker is a scripted session on loopback (`web/live/endpoint.js`) that keeps
the image in memory and checks its digest: no flash, no bootloader, no radio and no device. A
real install and a real rollback on hardware are the owner's bench session
(`docs/firmware-updates.md`).

**The walk-through for a compact Wi-Fi speaker** (`web/src/speaker-setup.js`;
`docs/decisions/0103-wifi-provisioning-over-softap.md`, decisions 2 to 4, which names this
screen as where it lives). It is guidance and a watch on the state, and nothing else: the
server has no route for provisioning, and the app cannot drive the speaker's join page (a
secure page cannot portably fetch a plain-HTTP device, which is why the speaker serves the page). Its four
steps are the decision's:

1. Switch the speaker on. With no network it raises its own access point, `chorus-setup-<6
   characters>`; its 12-character setup secret is the access point's password, and the speaker
   prints it and the address of its join page on its serial console.
2. Join that access point from the phone's Wi-Fi settings, with the setup secret.
3. Open the address the speaker printed in the phone's browser: the speaker's own page ("chorus
   speaker setup", `GET /`), a form with two fields. The house network's name and passphrase are
   typed there and posted to the speaker (`POST /join`); a failed join leaves the access point
   up and the page says why (`auth-error`, `network-not-found`).
4. Come back to the house's network.

**The app never asks for the network's passphrase and never holds it**: the walk-through has no
field of any kind and sends no request. What it keeps is the ids of the speakers that were
adopted when it began, and it completes by itself, with nothing pressed, when the state lists a
speaker that is not one of them: adoption on the audio network is the success signal (K92). It
then names the speaker and links to the speakers screen, where the speaker is new.

The phone leaves the house's network to reach the speaker, so the app loses the server while
the walk-through is open. That is the store's ordinary "lost" status (or "signed-out", when the
login lapsed meanwhile): the steps stay, the screen says the server cannot be reached and that
this is expected, and the event stream's own retry brings the state back with no reload. A
phone may also discard the page while its owner is in the Wi-Fi settings, so the ids it began
with are kept in the tab's session storage and a page loaded again goes on from them; leaving
the screen ends the walk-through and removes them. Session storage holds those ids and nothing
else.

**The walk-through has never run against a real speaker.** What is tested is the app's half:
the steps' words against decision 0103, and completion on an arrival in the state, which
`web/live/speakers.live.js` produces with a scripted endpoint session adopted by a real
`chorus-server`. No access point, join page or radio has been part of any test, and the
firmware's own binding has not run on hardware either (decision 0103, "NOT host-tested"). The
first run on a speaker is the owner's bench step, `docs/bench-packet.md` S9. Two things in the
steps are therefore taken from the records and not seen: that a phone's browser reaches the
page at the printed address, and that the phone returns to the house's network on its own. The
setup secret and the page's address are on the speaker's serial console only (decision 0103,
follow-ups), so today the walk-through needs someone who can read that console.

- **The server owns the state** (`docs/decisions/0185-the-apps-state-layer-and-its-live-test.md`).
  The app reads `GET /api/state`, follows `GET /api/events` and sends `POST /api/command`. An
  element shows what the last state message says and keeps no value of its own: a control changes
  on the page when the state that resulted comes back, and a refused command shows the server's
  words. The one thing an update leaves alone is a control a person has hold of.
- **Groups** (`docs/decisions/0187-groups-in-the-app.md`). A move is a room and a destination and
  becomes one command. The drag is written on Pointer Events, not HTML drag and drop, so that it
  starts from a finger: press the handle on a room's card ("Move <room>"), drag it onto another
  room or a group, let go. A release over nothing, or Escape, changes nothing. The same move
  without a drag is the "Plays with" list on the card, a native list a keyboard and a screen
  reader have for nothing.
- **Inputs and now playing** (`docs/decisions/0189-inputs-and-now-playing-in-the-app.md`). One
  button for each input the server offers. What is playing is the title, the artist, the album,
  the artwork and whether it is playing, paused or buffering. The app controls rooms, groups,
  inputs and sound, not content (K64): there is no library and no queue.
- **Artwork** (`docs/decisions/0184-artwork-is-proxied-not-the-policy-widened.md`). The image is
  fetched by the server and served on its own `GET /api/artwork?group=<id>`, so the policy's
  `img-src 'self'` stands. A record with no artwork, or one whose image does not load, shows a
  placeholder.

### Part 2 as built, and what was never run on real hardware

Part 2 (built 2026-10-05) is every further screen in the table of addresses above: a room's
sound, its volume limit and quiet hours, its theater settings (A/V trim, TV autoplay and its
options, the TV upmix, bass management) and its correction with the microphone of the device it
is open on; the house's autoplay rules; its alarms with all four K80 sources, its stored sources
and its sleep timers; and its speakers (new, name, room, forget, a refused changed key), the
firmware on each with "update available" and the explicit install, and the walk-through for a
compact Wi-Fi speaker. No screen of part 2 is left to build. There is no snooze, no upload of a
firmware image and no way to accept a changed key, because the catalog has no command for any of
them.

How each is held: by its unit test over a scripted server (`web/test/`), by its live test
against a real `chorus-server` in node (`web/live/`), and, since a browser paints it nowhere
else, by the second test of the one browser smoke test ("Running the tests", below).

**What was never run on real hardware.** All of the above ran on a development host, against a
server on loopback, with scripted sessions in the speakers' place. None of the following has
happened, and each is a step of "Phone check, part 2":

| Never run | What stood in for it | Where it is settled |
|---|---|---|
| Any part-2 screen on a phone, under a finger | happy-dom in node, and a headless Chromium at a phone's width for the home alone | part 2, step 1 |
| A room measured with a real microphone in a real room | the fitter's synthetic recordings (`fixtures/roomfit/`) through the screen's capture seam, and in the browser Chromium's fake audio device, which is a beep and not a room | part 2, steps 2 to 4 |
| What a phone's browser does with the three microphone constraints, and whether it keeps recording to the end of the sweep | `docs/room-correction.md`, "Per-phone limits", as read and not as seen | part 2, step 2 |
| The correction heard: whether a room sounds better with it on | nothing: no test listens, and the fitter's "predicted after" is a prediction from one recording | part 2, step 4 |
| The Wi-Fi walk-through against a real speaker: its access point, its join page, the phone leaving and rejoining the house's network | a scripted endpoint session adopted by a real server (`web/live/speakers.live.js`); the firmware's own binding has not run on hardware either (decision 0103) | part 2, step 5, with `docs/bench-packet.md` S9 |
| An install pressed in the app that writes a real speaker's flash, its trial boot, its confirmation and a rollback | a scripted session on loopback that keeps the image in memory (`web/live/firmware.live.js`); the server refuses any other speaker as `owner-not-at-bench` | part 2, step 6, with `docs/bench-packet.md` S10 |
| A changed key from a real speaker (a board reflashed with its key erased) | the scripted server of `web/test/speakers.test.js` | part 2, step 6, when a board's flash is erased at the bench |
| An alarm ringing in a room at its hour from each of the four sources, and a sleep timer ending the music | a server whose schedule runs ten times faster, with a scripted line-in, a stream on loopback and a fake Spotify receiver (`web/live/alarms.live.js`) | part 2, step 7 |
| The A/V trim against a real TV's picture, TV autoplay from a real optical or ARC input, the upmix on real surrounds, bass management on a real sub | a scripted endpoint offering an optical input; the room of the live test has no sub | part 2, step 8, with `docs/bench-packet.md` S8 |
| A quiet-hours window taking effect at its hour on the household's server | a server whose civil clock is held (`web/live/limits.live.js`) | part 2, step 7 |

## Installing, and the service worker's cache rules

The app installs from the browser (`docs/decisions/0190-the-app-installs-behind-the-login.md`).
Its manifest (`manifest.webmanifest`: name "chorus", `start_url`, `scope` and `id` of `/app/`,
`standalone`, icons of 192 and 512 pixels and a 180 pixel icon for a phone's home screen) is
linked with `crossorigin="use-credentials"`, because behind a login a manifest fetched without
the session is refused and an app with no manifest does not install.

The service worker is written by hand: `web/src/worker.js` is its rules as pure functions,
`web/src/sw.js` hands the browser's events to them, and the build bundles both into `dist/sw.js`.
It exists so that the app opens when the server cannot be reached, and it must never serve the
login page as the app. Its rules:

| Request | What the worker does |
|---|---|
| anything under the server's `/api/` (the state, a command, the artwork, every event stream) | nothing: the request is the browser's own, exactly as with no worker; it is never fetched by the worker and never written to a cache |
| another origin, the server's other pages, any request that is not a `GET` | nothing, the same |
| a navigation to the app | the network first; the document it answers with is kept as the shell; the shell is the answer when the network gives none, or gives a `5xx` (a proxy answering for a server that is down); a redirect to the login is handed to the browser, which follows it |
| a file named by its hash (`assets/<name>-<hash>.<ext>`) | the cache first, the network when it is not there yet |
| any other file of the app | the network first, the cache when the network fails |

One function, `mayStore`, decides what is written to a cache, and every write goes through it:
only a `GET` of a file under `/app/` that is not the API, answered with status `200`, of type
`basic` (this origin's), not redirected. So a redirect that was not followed, the page a followed
redirect ended on, a `401`, a `404`, a gateway's `502` and another origin's response are never
kept, whichever path they arrive on.

- At install the worker fetches every file of the build (the shell) with `redirect: "manual"` and
  keeps them all or none: a worker installed while the login has lapsed is not installed, and the
  browser tries again at the next load.
- The cache is `chorus-app-<version>`, the version a digest of the build's files; activating
  deletes the other `chorus-app-` caches and nothing else.
- The worker does not skip waiting and claims no open page. A new build reaches an open page at
  its next load, and its worker takes over when the old one's pages are closed.
- Offline, or with the server down, a person gets the shell, which says the server cannot be
  reached and follows it when it is back. Nothing can be controlled offline.

## The signed-out state

A login lapses, and the proxy then answers any request with a redirect to its login page or with
a refusal. Every request of `web/src/api.js` is made with `redirect: "manual"`, so the page never
reads a login page as the server's answer. An `opaqueredirect` or a `401` is read as signed out:
`chorus-server` answers neither on any route, so both can only come from in front of it.

- The page says "Signed out." with a link, "Sign in", to its own address. Following it is a
  navigation: the login shows its page and returns to the app.
- The rooms last known stay on the page, marked as last known. Nothing can be changed: a command
  is answered by the login and never reaches the server, and the page says so at once.
- The event stream keeps trying, so a person who signs in in another tab is live again with no
  reload.

Not known from here: whether the real login answers an API call with a redirect or a `401` (both
are read as signed out), and whether its redirect completes inside an installed app on a phone.
The second is step 5 of the phone check.

## The three layouts, and kiosk mode

`docs/decisions/0191-phone-and-desktop-layouts-and-the-kiosk.md`. There is no television layout
(K86).

| Layout | When | What is painted |
|---|---|---|
| phone | the viewport is narrower than 48em (768 CSS pixels at the default text size) | one column, the groups and then the rooms; the navigation in a bar fixed to the bottom edge |
| desktop | the viewport is 48em wide or wider | two columns, the groups beside the rooms; the navigation in the header |
| kiosk | the app was opened with `?kiosk` | either of the two by its width, with no wordmark (and, wide, no header), every control at least 64 by 64 CSS pixels, larger text, and the screen kept on |

The one breakpoint is `DESKTOP_MIN_EM` in `web/src/layout.js` and is written nowhere else:
`chorus-app` reflects the layout as an attribute and the styles select on it, so no element holds
a media query. A touch target is at least 44 by 44 CSS pixels outside the kiosk.

A further screen (a room's sound, its limits, its theater settings, its correction, the autoplay rules, the alarms, the speakers and the walk-through) is the same in all three: one column as wide as the page, with
its "Back" link inside the screen's own region, because a wide kiosk paints no header. On a
phone the navigation bar stays at the bottom edge and leads back to the groups or the rooms.

**Entering kiosk mode.** Kiosk mode is for a tablet on a wall, and its switch is the address:

1. Open `/app/?kiosk` once in the tablet's browser. That browser is a kiosk from then on: the
   choice is kept in `localStorage`, so it survives a reload and the installed app's start
   address, which carries no query.
2. Install the app from that browser, or set the tablet's browser up as a kiosk. The browser's
   own chrome is not the page's to remove; the manifest's `standalone` is what removes it.
3. To leave, open `/app/?kiosk=0`.

A kiosk asks for a screen wake lock and asks again each time the page becomes visible, because a
browser takes the lock back when the page is hidden. Without a secure context, or when the browser
refuses, the kiosk works the same and the screen sleeps as the tablet is set.

## Running the tests

From the repository root, with the pinned node and pnpm of `mise.toml` (`mise install`). CI runs
all three in `make gate`; here each is a narrow run of its own.

| Command | What it is | It ends with |
|---|---|---|
| `make web-test` | the unit tests: `node --test` over happy-dom, no browser, no server. The elements by their labels, the store, the worker's rules against a scripted network and cache, the signed-out state, the layouts and the kiosk's switch, the navigation over happy-dom's own address and history, and the sound, limits, autoplay, alarms, theater, room-correction, speakers, firmware and walk-through screens over a scripted server | node's own summary, `fail 0` |
| `CHORUS_SERVER_BIN=<a built chorus-server> make web-live` | the live tests (`web/live/*.live.js`): the same elements and store in node, no browser, against a real `chorus-server`. Each screen's controls are driven by their labels and what the page shows is compared with the server's `/api/state`, in both directions. `alarms.live.js` also needs the two test programs built beside the server (below) | `web-live: PASS` |
| `CHORUS_SERVER_BIN=<a built chorus-server> make web-smoke` | the one browser test (`web/smoke/app.spec.js`, `docs/decisions/0183-the-one-browser-smoke-test.md`): Playwright's headless Chromium loads `/app/` from a real `chorus-server` through a fake login, and walks the further screens | `web-smoke: PASS` |

- `web-live` and `web-smoke` without `CHORUS_SERVER_BIN` end `web-live: SKIPPED` and
  `web-smoke: SKIPPED`; under `CI=true` and in the gate a skip is a failure.
- `web/live/alarms.live.js` runs the server with its civil clock started three schedule minutes
  before 07:00 and its schedule ten times faster (`--civil-time-from`,
  `--schedule-time-scale`), sets an alarm of each of the four kinds through the screen, and
  holds each to ringing as its own kind in `/api/state` and on the screen (the room plays
  `chime:<name>`, `line-in:<input>`, `player:p0` and `soloist:r0`, and the log has no
  `fallback=chime`), then stops each with the screen's "Stop"; a sleep timer set through the
  screen then ends by itself and leaves the state and the screen. What each kind needs it
  starts itself (`web/live/house.js`, `web/live/endpoint.js`): an endpoint session offering a
  line-in, a stream on loopback (`--players 1 --media-allow-loopback`), and the fake Soloist
  under the real receiver supervisor (`--soloist-receivers 1 --soloist-alarms`). The last two
  are the test programs `server-test-soloistd` and `server-test-fake-soloist`, which it looks
  for in `examples/` beside `CHORUS_SERVER_BIN`: `cargo build -p chorus-server --examples`
  builds them, and so does the gate's build of every target. No real Soloist, Spotify account
  or network is used.
- `web/live/theater.live.js` has a scripted endpoint session offer an optical input (a TV's,
  by the kind the real server then says) and puts that endpoint in a room. The "Theater" link
  appears on that room's card and on no other; the A/V trim (both ends of its range), the TV
  input's autoplay rule with both options off and on again, and the TV upmix are set through the
  screen and read back equal from `/api/state`. The room has no sub, so the real server's
  `active` is `false` and the screen has no bass management control; the bass controls' commands
  are held by the unit test (`web/test/theater.test.js`).
- `web/live/speakers.live.js` opens the speakers screen and, from it, the walk-through, then has
  a scripted endpoint session (`web/live/endpoint.js`) connect under an id the server has never
  seen. The open walk-through completes on that arrival; the speaker is then new on the speakers
  screen, is named, put in a room, taken out and forgotten through the screen, and each step is
  read back from `/api/state`. The server is started with an identity directory, where its pins
  are kept. No speaker, access point or join page is part of it.
- `web/live/firmware.live.js` stages an image with `chorus-server stage-firmware`, starts the
  server with `--firmware-dir` and has a scripted endpoint session that takes updates report an
  older version on loopback. The screen shows the update; across repeated reads of
  `/api/state`, a rescan from the screen and a declined question the state stays `idle`, the
  server logs no offer and the endpoint is sent none; "Install", confirmed, makes the state
  leave `idle`. The install is then cancelled from the screen (`cancelled`), started again,
  received whole by the endpoint (`verified`, byte for byte the staged file) and confirmed by
  its next session. The endpoint is a script with no flash: the real update unit against the
  real server is `crates/server/tests/firmware_install.rs`, whose C endpoint the gate builds
  after this step. Loopback only, where the server's guard does not apply; it never sets or
  reads `CHORUS_OWNER_AT_BENCH` (the flash guard lets no JavaScript file name it).
- `make web-smoke-install` downloads, once, the Chromium build the pinned `@playwright/test`
  names; a run never downloads it. On a host with no root Chromium's libraries and fonts come
  from two prefixes, as `web/README.md` writes out.
- `make web-build` rebuilds `web/dist` after a change to `web/src`; the output is committed with
  the change, and the gate rebuilds it and fails on a difference.

What the smoke test holds, since it is the only test in which a browser paints the app: the
page renders a room's name as the server has it and reports no Content-Security-Policy
violation; the manifest loads through the login; the service worker becomes active and controls
the page; after the page has read the state, followed an event and sent a command, the caches
hold the build's files and nothing under `/api/`; with the fake login expired the page says
"Signed out", its link leads to the login page and no cache holds that page; the phone layout
at 390 pixels, the desktop layout at 1280, the breakpoint (767 is one column, 768 is two) and
kiosk mode. A later change that needs a browser adds to that one file.

Its second test is part 2 in a browser, on a server with a scripted session
(`web/live/endpoint.js`) that is a speaker of one room and offers a TV's optical input:

- every further screen is opened by the app's own links (the room's card for its sound, limits,
  theater and correction; "Autoplay", "Alarms" and "Speakers" under the rooms; the walk-through
  from the speakers screen), has the address of the table above, and "Back" returns;
- the "Theater" link is on that room's card only once the TV input is there, with no reload;
- on the sound screen "Loudness" is pressed and "Bass" is set to +3 dB, and each is read back
  from the server's own `/api/state` and from the screen;
- on the correction screen "Use the microphone" asks the browser's real `getUserMedia` for one
  channel with echo cancellation, noise suppression and automatic gain control off, and Chromium
  (started with `--use-fake-device-for-media-stream --use-fake-ui-for-media-stream`) grants its
  fake audio device: "What the browser granted" shows the three "off, as asked" and the track's
  own channels and sample rate (on 2026-10-05 two channels at 44100 Hz, though one was asked
  for), recorded at 48000 Hz. "Play the sweep and record" then has the real server play its
  sweep to the scripted speaker while the fake device is recorded, and the recording is sent to
  `POST /api/room-fit`. The fake device makes a beep at full scale, not a sweep in a room, so
  the fitter's answer is whatever it makes of that (on 2026-10-05 the refusal `clipped`, a
  `422`); the test holds that the answer is the server's, that nothing was applied and that the
  microphone's track ended;
- after all of it no answer to any request under `/api/` (the state, the events, the commands,
  the upload) came from the service worker, and the caches hold the build's files and nothing
  else.

That test shows the screens open and work in one browser engine on a development host. It is
not a phone, the fake device is not a microphone, and nothing in it is evidence about a room.

What no test here holds is everything below.

## Phone check

The owner's, on the owner's own phones: no agent has a phone, the household's login or the
household's network. Its item is in the owner's queue (issues in the owner's agent harness;
`goals needs add`, `/goals:needs`), and this section is what it carries. It settles what
`docs/proposals/P5-app-stack.md` ("Open inputs") still marks `ASSUMED` about Safari and iOS, and
it is the first time the app is in a hand.

**Before it starts.** All three are the owner's own steps and none is done by this repository:

- A `chorus-server` whose build serves `/app/` is deployed. `GET /app/` on it answers `200`; a
  `404` means the deployed image is older than the app.
- The household's reverse proxy serves it over HTTPS behind the household login, with a
  certificate the phones trust.
- At least two rooms exist, and something with artwork can be played in one of them (a control
  point casting a track with a cover is enough).

**Which phones.** Every phone the household will use, and at least one iPhone in Safari, because
the open inputs are Safari's. Run steps 1 to 8 on each phone, in order. For each phone write down
its model, its OS version and its browser with its version.

1. **Open it.** On the house Wi-Fi, open `https://<the address chorus is served on>/app/` in the
   phone's browser and sign in at the household login when it asks.
   Report: whether the browser showed any certificate warning; whether the login returned to
   the app; whether the rooms appear, and whether the page looks styled (cards, a bar with
   "Groups" and "Rooms" along the bottom edge) or is bare unstyled text. Bare text on Safari
   means element styles are refused under the policy there.
2. **Install it from the browser.** Safari: the share button, then "Add to Home Screen". Chrome
   on Android: the menu, then "Install app" or "Add to Home screen".
   Report: whether the browser offered it; the name and the icon the home screen shows (it
   should be "chorus", a dot and two rings on a dark square).
3. **Launch it from the home screen.** Tap the icon.
   Report: whether it opens with no browser address bar; whether it asks to sign in again and,
   if so, whether signing in comes back to the app inside the installed window or lands in the
   browser instead; whether the rooms are live (change a volume from another device and watch it
   move with no reload).
4. **Relaunch it.** Close the app from the app switcher, then tap the icon again. Do it once
   more with the phone in airplane mode, then turn airplane mode off.
   Report: online, whether it opens straight to the rooms with no sign-in; in airplane mode,
   whether the app's own page opens, saying the server cannot be reached or the connection is
   lost (and not a browser error page); whether it goes live again by itself after airplane mode is turned off.
5. **Signed-out recovery.** End the session while the installed app is open: sign out at the
   household login (in the browser on Android, which shares its session with the installed app;
   on an iPhone the installed app has a session of its own, so end that session at the login's
   own session list, or leave the app until the login lapses). Bring the installed app to the
   front and move a volume slider.
   Report: whether the page says "Signed out." with a "Sign in" link, and how long that took to
   appear; whether the rooms stay on the page; whether tapping "Sign in" shows the login, and
   after signing in returns to the app, in the installed window, live again; anything else it
   did instead (a blank page, the login page shown inside the app's frame for good, a browser
   tab opening).
6. **Artwork.** Play something with a cover in one room.
   Report: whether the cover appears on that room's card beside the title and the artist, and
   whether it changes when the track changes; if a placeholder shows instead, the title that
   was playing.
7. **Drag to group on touch.** Press the handle on one room's card, drag it onto another room's
   card and let go. Then drag the room out again, or use "Remove" on the group's card.
   Report: whether the drag starts from a finger or the page scrolls instead; whether the room
   under the finger is marked during the drag; whether the group appears under "Groups" after
   the release, and dissolves again; whether the group's volume slider moves both rooms. If the
   drag does not work, whether the "Plays with" list on the card does the same move.
8. **Anything else the hand notices.** Controls too small to hit, text cut off, the bottom bar
   under the phone's own home indicator, a slider that fights the finger.

Optional, when a tablet is on hand: open `/app/?kiosk`, install it, and report whether the
controls are larger, whether the screen stays on for longer than the tablet's own sleep time, and
whether the kiosk is still a kiosk after the installed app is closed and opened again.

**What to report.** Per phone: the model, OS and browser line, then for each of steps 1 to 8
"as written" or what happened instead, in a sentence. A screenshot helps for a layout fault; no
address, account name or household name belongs in what is pasted back.

**How it ends.** The owner tells any session of the owner's agent harness what happened, in the
owner's own words; that session records it and closes the item. What the answer changes:

- The `ASSUMED` lines of `docs/proposals/P5-app-stack.md` ("Open inputs") for Safari are
  replaced by what the phones did: element styles under the policy (step 1), the login's
  redirect inside an installed app (steps 3 and 5), the drag on touch (step 7), and HTTPS with
  a certificate the phones trust (step 1).
- A step that did not go as written becomes a task of its own, with the phone's line and the
  report as its evidence.
- Not part of steps 1 to 8: the microphone, and everything else of part 2. That is the check
  below, with an item of its own in the owner's queue.

## Phone check, part 2

The owner's as well, and for the same reason: the screens of part 2 have never been in a hand,
the correction screen has never heard a room, the walk-through has never met a speaker, and no
install pressed in the app has written a board. Its item is in the owner's queue (an issue in
the owner's agent harness; `goals needs add`, `/goals:needs`), and this section is what it
carries. It can be done in three sittings, since they need different things: steps 1 to 4 and 7
need the phones and a room that plays; step 5 needs a speaker built for Wi-Fi; step 6 needs the
bench. A sitting that cannot happen yet (no Wi-Fi speaker is planned, 2026-10-04) is reported as
"not run", which is an answer.

**Before it starts.** The owner's own steps, none done by this repository:

- "Phone check" above has been run, or is run first: part 2 assumes the app opens, installs and
  stays signed in on each phone.
- The deployed `chorus-server` is a build that has part 2: a room's card shows "Sound",
  "Limits" and "Correction", and "Autoplay", "Alarms" and "Speakers" are under the rooms. A card
  with none of them means the deployed image is older.
- For steps 2 to 4, a room with a speaker that plays, at its usual volume, and a quiet house.
- For step 5, a compact Wi-Fi speaker flashed and at its first boot, with its serial console
  readable (`docs/bench-packet.md`, S9, through S9.1).
- For step 6, the owner at the bench with a board that plays (`docs/bench-packet.md`, S10), a
  newer image staged in the server's firmware directory (`docs/firmware-updates.md`, "Staging
  an image"), and, only for the last part of the step, the server run with
  `CHORUS_OWNER_AT_BENCH=1` in its environment for that session (`docs/firmware-updates.md`,
  "The bench variable"). Setting it is the owner's act and nothing in this repository does it.

**Which phones.** The phones of "Phone check", at least one iPhone in Safari and one Android
phone in Chrome where the household has both, because `docs/room-correction.md`, "Per-phone
limits", expects them to differ. Steps 2 to 4 are run on each phone, once in the browser and once
in the installed app. Write down the model, OS and browser line as before.

1. **Open every screen.** From a room's card open "Sound", "Limits" and "Correction" in turn;
   from under the rooms open "Autoplay", "Alarms" and "Speakers". On the sound screen move
   "Bass" and press "Loudness"; then use the phone's own back gesture.
   Should show: each screen alone under a "Back" link, in one column as wide as the phone, the
   bar with "Groups" and "Rooms" still at the bottom edge; the value beside "Bass" follows the
   slider ("+3 dB") and "Loudness" says "On"; the same values on a second device with no reload;
   the back gesture returns to the rooms.
   Report: any screen that is bare text, scrolls sideways or has a control too small to hit;
   whether the back gesture went to the rooms or left the app.
2. **The microphone.** Open a room's "Correction" and read the five lines of guidance. Press
   "Use the microphone".
   Should show: the browser's own permission prompt, once; then "What the browser granted" with
   six lines (echo cancellation, noise suppression, automatic gain control, channels, the
   microphone's sample rate, recorded at) and "Play the sweep and record".
   Report: the prompt's words and whether it came again on a later visit; every one of the six
   lines exactly as shown ("off, as asked", "on ...: asked off, and the browser kept it on", or
   "not reported by this browser"), and whether the flagged warning about a fit that may be
   wrong appeared. In the installed app, whether the prompt appeared at all. If instead of the
   prompt there is a sentence (no HTTPS, not allowed, no microphone, in use), which one.
3. **The sweep.** Stand where the room is listened to, phone at ear height, microphone
   uncovered. Press "Play the sweep and record" and hold still.
   Should show: "The sweep is playing and the microphone is recording. Hold still." while one
   rising tone of 5 seconds plays in that room only; then "The recording is with the server,
   being fitted. The microphone is off."; then, within a few seconds, either "Proposed filters"
   (lines such as "45 Hz, -9.32 dB, Q 6.409", the two figures, "Nothing has changed in the room
   yet.", "Apply" and "Discard") or "The server refused the recording:" with a name
   (`too_short`, `clipped`, `too_quiet`, `too_noisy`) and what to do about it.
   Report: whether the tone played in that room and in no other; whether the phone's screen
   stayed on and the recording ran to the end (a `too_short` refusal says it did not); the
   proposed filters and both figures, or the refusal's name and its words; whether the phone's
   own microphone indicator went off afterwards. Do it twice from the same place and report
   both answers: two recordings of one seat that propose different filters is a finding.
4. **Apply, listen, switch, undo.** Press "Apply". Play music the household knows in that room.
   Press "Correction" to switch it off and on while it plays, then "Undo".
   Should show: "Applied. Undo puts back what the room had."; the filters under "This room's
   correction" with the switch "On"; the switch changing what is heard; after
   "Undo", "Put back." and the room as it was, with "Nothing to undo".
   Report: whether the room sounds better, worse or no different with the correction on, in the
   owner's own words; whether switching it clicked, dropped out or changed the volume. This is
   the only evidence there will be of whether the correction corrects: nothing here measures it.
5. **The Wi-Fi walk-through, on a real speaker.** On the phone open "Speakers", then "Set up a
   Wi-Fi speaker", and do what its four steps say with the speaker of S9.1: join
   `chorus-setup-<6 characters>` with the setup secret from the console, open the address the
   console printed, type the house network's name and passphrase into the speaker's page, come
   back to the house's network. This is `docs/bench-packet.md` S9.2 done from the app's screen,
   with a `chorus-server` running on the audio network so that the speaker is adopted.
   Should show: while the phone is on the speaker's access point, the walk-through still on the
   screen, saying the server cannot be reached and that this is expected; the speaker's own
   page, "chorus speaker setup", with two fields; back on the house's network, with nothing
   pressed, the walk-through completing by itself and naming the new speaker, with a link to
   the speakers screen, where the speaker is marked "New".
   Report: whether the phone's browser reached the page at the printed address; whether the
   phone came back to the house's network by itself or had to be moved; whether the app's page
   was still there or had been discarded and loaded again, and whether it completed either way;
   how long from "Join" to the completion. The app never asks for the passphrase: report it if
   anything of the app did. No network name, passphrase or setup secret belongs in the report.
6. **Install from the app, at the bench.** With the board of S10 adopted and connected and a
   newer image staged, open "Speakers" on the phone. First with the server run as usual (no
   bench variable): press "Install" beside the image and "Yes, install it". Then, the owner at
   the bench, with the server restarted with `CHORUS_OWNER_AT_BENCH=1` for this session: press
   "Install" and "Yes, install it" again, and watch the board's console.
   Should show: "Update available" on that speaker's row with the image's version and name, and
   nothing installing while the screen is merely open; the question naming the image, its
   version, the speaker and what it runs now; the first press refused on the row in the
   server's words, "Refused: owner-not-at-bench: ...", and the board sent nothing; the second
   moving through "Install requested", "Receiving ..." with its progress bar, "Written and
   checked", "On trial" and "Installed: ... confirmed itself", the row then showing the new
   version and no "Update available".
   Report: each line the row showed, in order, against the console's; whether "Cancel install"
   pressed during a second install stopped it and the board kept running what it ran; and, when
   S10 installs its deliberately bad image, whether the row ends at "Rolled back" with the
   version that runs again. If a board's flash is erased at the bench, its next session offers a new key
   under the old id: report whether the speakers screen then shows "Refused: ... offered a
   changed key" with both fingerprints and no button that accepts it. Afterwards the bench
   variable is taken out of the server's environment again.
7. **An alarm, a sleep timer and quiet hours.** On "Alarms" set an alarm a few minutes ahead in
   one room with a chime; when it has rung and been stopped, one with each other source the
   household uses (a line-in, a stored stream URL, a stored Spotify URI). Start a sleep timer
   of one minute in a room that plays. On that room's "Limits" add a quiet-hours window that
   covers the present time at 25%.
   Should show: the alarm listed with its time, days and source; at its minute the room rising
   to the alarm's volume, "Ringing now" and a "Stop" button, and silence after "Stop"; the
   sleep timer counting down in seconds and the room going quiet when it ends, the entry then
   gone; the window marked "Active now", "Limit in force now" at 25% and the room's volume held
   under it.
   Report: for each source whether it rang as itself or as the bell chime (a stream URL needs
   a server started with `--players`, a Spotify URI needs `--soloist-receivers` and
   `--soloist-alarms`, and the screen cannot warn of either); how many seconds late the alarm
   was by the phone's clock; whether the quiet hours ended by themselves at the window's end.
8. **Theater, where there is a TV.** Only for a room with a hub on a TV's optical or ARC output
   or with a centre, a sub or surrounds (`docs/bench-packet.md`, S8): its card has a "Theater"
   link. Open it, play the TV, move "A/V trim" while watching speech, and try each control that
   is there.
   Should show: the TV input by its kind ("Optical" or "HDMI ARC"); the trim's value following
   the slider and the two 1 ms buttons; bass management controls only when the room's set has a
   sub, and "This room's set has no sub" otherwise.
   Report: the trim at which picture and speech line up by eye, and whether the range reached
   it; whether TV autoplay started the TV's sound in the room when the TV was switched on and
   stopped on standby; anything a control did that its words did not say.

**What to report.** Per phone: the model, OS and browser line, then for each step run "as
written" or what happened instead, in a sentence; for a step not run, "not run" and why.
Console lines for steps 5 and 6 are the ones `docs/bench-packet.md` asks for and no others. No
address, account, network name, passphrase or household name belongs in what is pasted back.

**How it ends.** As "Phone check" ends: the owner tells any session of the owner's agent
harness what happened, and that session records it and closes the item. What the answer
changes:

- `docs/room-correction.md`, "Per-phone limits": each **ASSUMED** and **not known** line about
  the three constraints, the sample rate and the screen staying awake is replaced by what each
  phone showed in steps 2 and 3, and "No real phone has been measured with this" by the phones
  that were.
- This page: "The correction screen has never measured a room", "The walk-through has never run
  against a real speaker", "Nothing here ran on a board" and the table of what was never run
  on real hardware each lose the lines the report settles, with the date.
- `docs/bench-packet.md`: S9's and S10's own Needs items are answered by their sessions as that
  packet says; steps 5 and 6 add only what the app's screen showed beside them.
- A step that did not go as written becomes a task of its own, with the phone's line and the
  report as its evidence. A correction that sounds worse is such a task and not a reason to
  remove the switch: "Undo" and the switch are the owner's way back meanwhile.
