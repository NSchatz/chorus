// The state layer: the server's state message, held once for the whole app.
//
// The model is docs/control-page.md's: the server owns the state and the app
// keeps no copy of its own to disagree with it. The store holds the last state
// message the server sent and nothing derived from what the app did: a
// command changes what is shown only when the state that resulted comes back.
// There is no optimistic value.
//
// It starts with a snapshot (GET api/state), so there is something to show
// where the event stream cannot be opened, and then follows the event stream
// (GET api/events), whose every message is a complete state: each one
// replaces the last.

import { artworkUrl } from "./api.js";

// Where a group's artwork is when nobody says otherwise: the server's root as
// the page sees it (the app is served under /app/).
const artworkOfThePage = (group, art) => artworkUrl("../", group, art);

const textOrNull = (value) => (typeof value === "string" && value ? value : null);
const PLAY_STATES = ["playing", "paused", "buffering"];

// What every formed group plays, by group id: { source, nowPlaying }.
//   source      the group's `source` as the catalog spells it, or null
//   nowPlaying  the group's now-playing record, or null when it has none:
//               { title, artist, album, state, via, artwork }, each a string
//               or null where the server does not know it. `state` is
//               "playing", "paused" or "buffering". `artwork` is where the
//               page loads the cover from, this server's own artwork route,
//               or null when the record names no artwork.
export function playingOf(state, artwork = artworkOfThePage) {
  const formed = state && Array.isArray(state.groups) ? state.groups : [];
  const playing = new Map();
  for (const group of formed) {
    if (!group || typeof group !== "object" || typeof group.id !== "string" || !group.id) continue;
    const record = group.now_playing && typeof group.now_playing === "object" ? group.now_playing : null;
    const art = record ? textOrNull(record.art_url) : null;
    playing.set(group.id, {
      source: textOrNull(group.source),
      nowPlaying: record && {
        title: textOrNull(record.title),
        artist: textOrNull(record.artist),
        album: textOrNull(record.album),
        state: PLAY_STATES.includes(record.state) ? record.state : null,
        via: textOrNull(record.via),
        artwork: art ? artwork(group.id, art) : null,
      },
    });
  }
  return playing;
}

const NOT_PLAYING = { source: null, nowPlaying: null };

// The inputs the server offers now (the state's `inputs`, the line-ins as
// `<endpoint>/<input>`), in its order and no other: { id, source, label }.
// `source` is what a `take` names to play it; `label` is the name a person
// gave the input (`input_labels`), or its id.
export function inputsOf(state) {
  const offered = state && Array.isArray(state.inputs) ? state.inputs : [];
  const labels = new Map(
    (state && Array.isArray(state.input_labels) ? state.input_labels : [])
      .filter((label) => label && typeof label.input === "string" && typeof label.name === "string" && label.name)
      .map((label) => [label.input, label.name]),
  );
  return offered
    .filter((id) => typeof id === "string" && id)
    .map((id) => ({ id, source: `line-in:${id}`, label: labels.get(id) ?? id }));
}

// A room's sound as the screens read it: { bass, treble, loudness, night,
// speech }. `bass` and `treble` are whole dB and the other three booleans, as
// the server holds them; a member the state does not carry, or carries as
// something else, is null, and a screen says so rather than show a default.
export function soundOf(zone) {
  const sound = zone && zone.sound && typeof zone.sound === "object" ? zone.sound : {};
  const tone = (value) => (Number.isInteger(value) ? value : null);
  const flag = (value) => (typeof value === "boolean" ? value : null);
  return {
    bass: tone(sound.bass),
    treble: tone(sound.treble),
    loudness: flag(sound.loudness),
    night: flag(sound.night),
    speech: flag(sound.speech),
  };
}

// A room's volume limits and quiet hours as the screens read them:
// { limit, effectiveLimit, quietEnabled, windows }.
//   limit           the room's own maximum, in thousandths
//   effectiveLimit  the maximum in force now, as the server computed it (the
//                   lower of `limit` and the cap of a window that is active
//                   while quiet hours are switched on). The app never works
//                   one out.
//   quietEnabled    whether an active window caps the room
//   windows         the room's quiet-hours windows, in the server's order:
//                   { days, start, end, limit, active }, the days it starts
//                   on, "HH:MM" twice, its cap in thousandths, and whether
//                   the server's clock is inside it now (which it says
//                   whether or not quiet hours are switched on)
// A member the state does not carry, or carries as something else, is null
// (`windows` is then empty), and a screen says so rather than show a default.
export function limitsOf(zone) {
  const held = zone && typeof zone === "object" ? zone : {};
  const time = (value) => (typeof value === "string" && /^\d\d:\d\d$/.test(value) ? value : null);
  return {
    limit: thousandths(held.limit),
    effectiveLimit: thousandths(held.effective_limit),
    quietEnabled: typeof held.quiet_enabled === "boolean" ? held.quiet_enabled : null,
    windows: (Array.isArray(held.quiet) ? held.quiet : [])
      .filter((window) => window && typeof window === "object")
      .map((window) => ({
        days: (Array.isArray(window.days) ? window.days : []).filter((day) => typeof day === "string"),
        start: time(window.start),
        end: time(window.end),
        limit: thousandths(window.limit),
        active: window.active === true,
      })),
  };
}

// The house's autoplay rules (the state's `autoplay`), by input, in the
// server's order: { input, target, enabled, stopOnStandby, lowLatency }. The
// two TV fields are `true` unless the rule says `false`, which is how the
// catalog writes them.
export function autoplayOf(state) {
  const rules = state && Array.isArray(state.autoplay) ? state.autoplay : [];
  return rules
    .filter((rule) => rule && typeof rule.input === "string" && rule.input && typeof rule.target === "string")
    .map((rule) => ({
      input: rule.input,
      target: rule.target,
      enabled: rule.enabled === true,
      stopOnStandby: rule.stop_on_standby !== false,
      lowLatency: rule.low_latency !== false,
    }));
}

// The house's alarms (the state's `alarms`), in the server's order: { id,
// target, time, days, source, volume, rampS, durationMin, enabled, ringing }.
// `volume` is in thousandths; `days` empty is an alarm that rings once.
export function alarmsOf(state) {
  const alarms = state && Array.isArray(state.alarms) ? state.alarms : [];
  const count = (value) => (Number.isInteger(value) && value >= 0 ? value : 0);
  return alarms
    .filter((alarm) => alarm && typeof alarm.alarm === "string" && alarm.alarm && typeof alarm.target === "string")
    .map((alarm) => ({
      id: alarm.alarm,
      target: alarm.target,
      time: typeof alarm.time === "string" ? alarm.time : "",
      days: (Array.isArray(alarm.days) ? alarm.days : []).filter((day) => typeof day === "string"),
      source: typeof alarm.source === "string" ? alarm.source : "",
      volume: thousandths(alarm.volume) ?? 0,
      rampS: count(alarm.ramp_s),
      durationMin: count(alarm.duration_min),
      enabled: alarm.enabled === true,
      ringing: alarm.ringing === true,
    }));
}

// The sleep timers asked for (the state's `sleep`), in the server's order:
// { target, minutes, remainingS }. `minutes` is what was asked for;
// `remainingS` is the whole seconds left as the server last counted them, or
// null on a server that does not say (ADR 0194).
export function sleepOf(state) {
  const timers = state && Array.isArray(state.sleep) ? state.sleep : [];
  return timers
    .filter((timer) => timer && typeof timer.target === "string" && timer.target)
    .map((timer) => ({
      target: timer.target,
      minutes: Number.isInteger(timer.minutes) ? timer.minutes : null,
      remainingS: Number.isInteger(timer.remaining_s) && timer.remaining_s >= 0 ? timer.remaining_s : null,
    }));
}

// The stored sources (the state's `stored_sources`), in the server's order:
// { id, kind, value, name }, `kind` "url" or "spotify". An alarm names one as
// `stored:<id>`.
export function storedSourcesOf(state) {
  const stored = state && Array.isArray(state.stored_sources) ? state.stored_sources : [];
  return stored
    .filter((source) => source && typeof source.id === "string" && source.id && typeof source.kind === "string")
    .map((source) => ({
      id: source.id,
      kind: source.kind,
      value: typeof source.value === "string" ? source.value : "",
      name: typeof source.name === "string" && source.name ? source.name : source.id,
    }));
}

// The built-in chimes (the state's `chimes`, ADR 0194), in the server's
// order, or null on a server that does not say which it has. The app has no
// list of its own.
export function chimesOf(state) {
  if (!state || !Array.isArray(state.chimes)) return null;
  return state.chimes.filter((name) => typeof name === "string" && name);
}

// The Spotify receivers (the state's `soloist`, written only by a server
// started with receivers): the targets whose receiver is running, as the
// server spells them (`room:<id>`, `group:<id>`), or null on a server that
// runs none.
export function receiversOf(state) {
  const soloist = state && state.soloist && typeof state.soloist === "object" ? state.soloist : null;
  if (!soloist) return null;
  return (Array.isArray(soloist.receivers) ? soloist.receivers : [])
    .filter((receiver) => receiver && receiver.state === "running" && typeof receiver.target === "string" && receiver.target)
    .map((receiver) => receiver.target);
}

// The adopted speakers (the state's `speakers`, written only when there is
// one), in the server's order: { id, name, named, room, present, software,
// link, key, roles }.
//   name      the name a person gave it, or the one the server made for it
//   named     whether a person has named it
//   room      the room it is assigned to, or null
//   present   whether a session of it is up now
//   software  what its latest `hello` said it runs, or null before one
//   link      what it reported its link as ("wired", "wireless", "unknown")
//   key       the fingerprint of the key it is pinned to, or null
//   roles     its latest `hello`'s roles, by name
// `isNew` is a speaker nobody has dealt with yet: adopted, not named and in
// no room.
export function speakersOf(state) {
  const speakers = state && Array.isArray(state.speakers) ? state.speakers : [];
  return speakers
    .filter((speaker) => speaker && typeof speaker === "object" && typeof speaker.id === "string" && speaker.id)
    .map((speaker) => {
      const named = speaker.named === true;
      const room = textOrNull(speaker.room);
      return {
        id: speaker.id,
        name: textOrNull(speaker.name) ?? speaker.id,
        named,
        room,
        isNew: !named && room === null,
        present: speaker.present === true,
        software: textOrNull(speaker.software),
        link: textOrNull(speaker.link) ?? "unknown",
        key: textOrNull(speaker.key),
        roles: (Array.isArray(speaker.roles) ? speaker.roles : []).filter((role) => typeof role === "string" && role),
      };
    });
}

// The handshakes refused for a changed key (the state's `key_changes`, the
// latest per id, written only when there is one), in the server's order:
// { id, pinned, offered }, the fingerprint of the key the id is pinned to,
// which did not move, and of the key that was offered and refused.
export function keyChangesOf(state) {
  const changes = state && Array.isArray(state.key_changes) ? state.key_changes : [];
  return changes
    .filter((change) => change && typeof change === "object" && typeof change.id === "string" && change.id)
    .map((change) => ({ id: change.id, pinned: textOrNull(change.pinned), offered: textOrNull(change.offered) }));
}

// The room with this id among the rooms the store holds, or null.
export function roomOf(rooms, id) {
  return (Array.isArray(rooms) ? rooms : []).find((room) => room.id === id) ?? null;
}

// What the screens read of one room. A member this cannot read is null, and
// a screen says so in words rather than showing a made-up value.
function readRoom(zone, speakerNames, playing) {
  if (!zone || typeof zone !== "object" || typeof zone.id !== "string" || !zone.id) return null;
  const bond = Array.isArray(zone.bond) ? zone.bond : [];
  const group = typeof zone.group === "string" && zone.group ? zone.group : zone.id;
  return {
    id: zone.id,
    name: typeof zone.name === "string" && zone.name ? zone.name : zone.id,
    // Thousandths of full scale, 0 to 1000: the catalog's own step.
    volume: thousandths(zone.volume),
    muted: typeof zone.muted === "boolean" ? zone.muted : null,
    // Tone, loudness, night mode and speech enhancement (soundOf).
    sound: soundOf(zone),
    // Its volume limit and its quiet hours (limitsOf).
    limits: limitsOf(zone),
    // The id of the group the room plays in: its own id when it is alone.
    group,
    // What the room plays while it is alone in the group named for it (a
    // room in a saved or live group is shown that on the group's card):
    // `source` and `nowPlaying` as playingOf gives them, else null.
    ...((group === zone.id && playing.get(group)) || NOT_PLAYING),
    // The bonded set, in the server's order: each member's endpoint, the
    // name a person gave that speaker (or the endpoint's id) and its role.
    bond: bond
      .filter((member) => member && typeof member.endpoint === "string" && typeof member.role === "string")
      .map((member) => ({
        endpoint: member.endpoint,
        name: speakerNames.get(member.endpoint) ?? member.endpoint,
        role: member.role,
      })),
  };
}

// The state message's rooms, in the server's order.
export function roomsOf(state, artwork) {
  const zones = state && Array.isArray(state.zones) ? state.zones : [];
  const speakers = state && Array.isArray(state.speakers) ? state.speakers : [];
  const speakerNames = new Map(
    speakers
      .filter((speaker) => speaker && typeof speaker.id === "string" && typeof speaker.name === "string" && speaker.name)
      .map((speaker) => [speaker.id, speaker.name]),
  );
  const playing = playingOf(state, artwork);
  return zones.map((zone) => readRoom(zone, speakerNames, playing)).filter(Boolean);
}

// A volume of the state message in thousandths, or null where it is not one.
function thousandths(volume) {
  return typeof volume === "number" && volume >= 0 && volume <= 1 ? Math.round(volume * 1000) : null;
}

// The groups a person sees (docs/control-plane.md, "The state a server
// holds"): every saved group, active or not (K59), in the server's order,
// then every live group, in the server's order. A room alone in the group
// named for it is not one of them: it is a room.
//
// Each is { id, name, kind, active, defined, rooms, volume, source, nowPlaying }:
//   kind     "saved" or "live"
//   active   a saved group's `active` as the server says it; null for a live one
//   defined  a saved group's rooms as defined, [{ id, name }]; null for a live one
//   rooms    the rooms playing in it now, [{ id, name }]; empty when not formed
//   volume   the group volume as the server computed it (K77), in
//            thousandths; null when the group is not formed. The app never
//            works one out.
//   source, nowPlaying
//            what the group plays, as playingOf gives them; null when the
//            group is not formed.
// A live group has no name on the server: it is called by its rooms.
export function groupsOf(state, artwork) {
  const names = new Map(roomsOf(state, artwork).map((room) => [room.id, room.name]));
  const playing = playingOf(state, artwork);
  const member = (id) => ({ id, name: names.get(id) ?? id });
  const list = (value) => (Array.isArray(value) ? value : []);
  const members = (zones) => list(zones).filter((id) => typeof id === "string" && id).map(member);
  const hasId = (entry) => entry && typeof entry === "object" && typeof entry.id === "string" && entry.id;
  const formed = list(state?.groups).filter(hasId);
  const saved = list(state?.saved_groups).filter(hasId);
  const savedIds = new Set(saved.map((definition) => definition.id));
  return [
    ...saved.map((definition) => {
      const now = formed.find((group) => group.id === definition.id);
      return {
        id: definition.id,
        name: typeof definition.name === "string" && definition.name ? definition.name : definition.id,
        kind: "saved",
        active: definition.active === true,
        defined: members(definition.zones),
        rooms: now ? members(now.zones) : [],
        volume: now ? thousandths(now.volume) : null,
        ...((now && playing.get(definition.id)) || NOT_PLAYING),
      };
    }),
    ...formed
      .filter((group) => group.kind === "live" && !savedIds.has(group.id))
      .map((group) => {
        const rooms = members(group.zones);
        return {
          id: group.id,
          name: rooms.map((room) => room.name).join(" + ") || group.id,
          kind: "live",
          active: null,
          defined: null,
          rooms,
          volume: thousandths(group.volume),
          ...(playing.get(group.id) ?? NOT_PLAYING),
        };
      }),
  ];
}

const isState = (message) => Boolean(message) && typeof message === "object" && Array.isArray(message.zones);

// The store over a client of api.js. `subscribe(listener)` calls the listener
// now and at every change with { state, rooms, groups, inputs, status }; `status` is
// "connecting" until the server has been heard from, "live" while the event
// stream delivers, "signed-out" when the login in front of the server answered
// in its place, and "lost" otherwise; in the last two what is shown is last
// known.
export function createStore(client) {
  const listeners = new Set();
  let state = null;
  let rooms = [];
  let groups = [];
  let inputs = [];
  let status = "connecting";
  // Whether the event stream has delivered: from then on it is the one
  // source, and a snapshot that answers late is the older of the two.
  let streamed = false;
  let close = null;

  const view = () => ({ state, rooms, groups, inputs, status });
  const tell = () => {
    const now = view();
    for (const listener of [...listeners]) listener(now);
  };
  const hold = (message) => {
    state = message;
    rooms = roomsOf(message, client.artwork);
    groups = groupsOf(message, client.artwork);
    inputs = inputsOf(message);
  };

  function start() {
    if (close) return;
    close = client.events({
      onState(message) {
        if (!isState(message)) return;
        streamed = true;
        hold(message);
        tell();
      },
      onStatus(next) {
        if (status === next) return;
        status = next;
        tell();
      },
    });
    client.state().then(
      (message) => {
        if (streamed || !isState(message)) return;
        hold(message);
        tell();
      },
      () => {
        // No snapshot: the event stream, which retries, is the way in.
      },
    );
  }

  function stop() {
    close?.();
    close = null;
  }

  // Send one control message. The answer to an accepted command is the state
  // that resulted, the bytes every subscriber is sent, and is held when it is
  // newer than what the stream has delivered; a refusal changes nothing here
  // and is returned for the screen to show.
  async function command(body) {
    const result = await client.command(body);
    // A command that met the login says so at once; the event stream, still
    // open from before the login lapsed, may not have noticed yet.
    if (result.signedOut && status !== "signed-out") {
      status = "signed-out";
      tell();
    }
    if (result.ok && isState(result.state) && (!state || result.state.serial > state.serial)) {
      hold(result.state);
      tell();
    }
    return result;
  }

  function subscribe(listener) {
    listeners.add(listener);
    listener(view());
    return () => listeners.delete(listener);
  }

  return { start, stop, command, subscribe, view };
}
