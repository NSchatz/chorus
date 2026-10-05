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

// What the screens read of one room. A member this cannot read is null, and
// a screen says so in words rather than showing a made-up value.
function readRoom(zone, speakerNames) {
  if (!zone || typeof zone !== "object" || typeof zone.id !== "string" || !zone.id) return null;
  const bond = Array.isArray(zone.bond) ? zone.bond : [];
  return {
    id: zone.id,
    name: typeof zone.name === "string" && zone.name ? zone.name : zone.id,
    // Thousandths of full scale, 0 to 1000: the catalog's own step.
    volume: thousandths(zone.volume),
    muted: typeof zone.muted === "boolean" ? zone.muted : null,
    // The id of the group the room plays in: its own id when it is alone.
    group: typeof zone.group === "string" && zone.group ? zone.group : zone.id,
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
export function roomsOf(state) {
  const zones = state && Array.isArray(state.zones) ? state.zones : [];
  const speakers = state && Array.isArray(state.speakers) ? state.speakers : [];
  const speakerNames = new Map(
    speakers
      .filter((speaker) => speaker && typeof speaker.id === "string" && typeof speaker.name === "string" && speaker.name)
      .map((speaker) => [speaker.id, speaker.name]),
  );
  return zones.map((zone) => readRoom(zone, speakerNames)).filter(Boolean);
}

// A volume of the state message in thousandths, or null where it is not one.
const thousandths = (volume) =>
  typeof volume === "number" && volume >= 0 && volume <= 1 ? Math.round(volume * 1000) : null;

// The groups a person sees (docs/control-plane.md, "The state a server
// holds"): every saved group, active or not (K59), in the server's order,
// then every live group, in the server's order. A room alone in the group
// named for it is not one of them: it is a room.
//
// Each is { id, name, kind, active, defined, rooms, volume }:
//   kind     "saved" or "live"
//   active   a saved group's `active` as the server says it; null for a live one
//   defined  a saved group's rooms as defined, [{ id, name }]; null for a live one
//   rooms    the rooms playing in it now, [{ id, name }]; empty when not formed
//   volume   the group volume as the server computed it (K77), in
//            thousandths; null when the group is not formed. The app never
//            works one out.
// A live group has no name on the server: it is called by its rooms.
export function groupsOf(state) {
  const names = new Map(roomsOf(state).map((room) => [room.id, room.name]));
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
        };
      }),
  ];
}

const isState = (message) => Boolean(message) && typeof message === "object" && Array.isArray(message.zones);

// The store over a client of api.js. `subscribe(listener)` calls the listener
// now and at every change with { state, rooms, groups, status }; `status` is
// "connecting" until the server has been heard from, "live" while the event
// stream delivers, and "lost" otherwise, when what is shown is last known.
export function createStore(client) {
  const listeners = new Set();
  let state = null;
  let rooms = [];
  let groups = [];
  let status = "connecting";
  // Whether the event stream has delivered: from then on it is the one
  // source, and a snapshot that answers late is the older of the two.
  let streamed = false;
  let close = null;

  const view = () => ({ state, rooms, groups, status });
  const tell = () => {
    const now = view();
    for (const listener of [...listeners]) listener(now);
  };
  const hold = (message) => {
    state = message;
    rooms = roomsOf(message);
    groups = groupsOf(message);
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
