// The rooms the server has, read once from its state message when the page
// opens: the least the shell can show of a real server. The live state layer
// (the snapshot followed by the event stream, the room card) is a later
// change and replaces this module.

// The state message's rooms, in the server's order, as { id, name }.
export function roomsOf(state) {
  const zones = state && Array.isArray(state.zones) ? state.zones : [];
  return zones
    .filter((zone) => zone && typeof zone.id === "string")
    .map((zone) => ({ id: zone.id, name: typeof zone.name === "string" && zone.name ? zone.name : zone.id }));
}

// GET <base>api/state. `base` is the server's root as the page sees it: the
// app is served under /app/, so from the page that is "../". A server that
// cannot be reached or answers with something else gives no rooms.
export async function loadRooms(fetchState = globalThis.fetch, base = "../") {
  try {
    const response = await fetchState(`${base}api/state`, { headers: { Accept: "application/json" } });
    if (!response.ok) return [];
    return roomsOf(await response.json());
  } catch {
    return [];
  }
}
