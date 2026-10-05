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
  const volume = zone.volume;
  const bond = Array.isArray(zone.bond) ? zone.bond : [];
  return {
    id: zone.id,
    name: typeof zone.name === "string" && zone.name ? zone.name : zone.id,
    // Thousandths of full scale, 0 to 1000: the catalog's own step.
    volume: typeof volume === "number" && volume >= 0 && volume <= 1 ? Math.round(volume * 1000) : null,
    muted: typeof zone.muted === "boolean" ? zone.muted : null,
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

const isState = (message) => Boolean(message) && typeof message === "object" && Array.isArray(message.zones);

// The store over a client of api.js. `subscribe(listener)` calls the listener
// now and at every change with { state, rooms, status }; `status` is
// "connecting" until the server has been heard from, "live" while the event
// stream delivers, and "lost" otherwise, when what is shown is last known.
export function createStore(client) {
  const listeners = new Set();
  let state = null;
  let rooms = [];
  let status = "connecting";
  // Whether the event stream has delivered: from then on it is the one
  // source, and a snapshot that answers late is the older of the two.
  let streamed = false;
  let close = null;

  const view = () => ({ state, rooms, status });
  const tell = () => {
    const now = view();
    for (const listener of [...listeners]) listener(now);
  };
  const hold = (message) => {
    state = message;
    rooms = roomsOf(message);
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
