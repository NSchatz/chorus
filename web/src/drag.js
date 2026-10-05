// The drag gesture, on Pointer Events (proposal P5, "Drag-to-group on phones
// and tablets"): one code path for a mouse, a finger and a pen. HTML drag and
// drop is not used: it does not start from a touch on phones and tablets.
//
// A press on a room's handle (an element with `data-drag-room`) becomes a
// drag once the pointer has travelled DRAG_THRESHOLD; until then it is a
// press, and releasing it is the handle's ordinary click. The handle captures
// the pointer, so the moves and the release come to it wherever the pointer
// is, and what lies under the pointer is found with elementFromPoint, through
// shadow roots. A drop target is an element with `data-drop` ("room", "group"
// or "alone") and, for the first two, `data-drop-id`.
//
// The gesture decides nothing about groups: it reports the room and the
// destination (grouping.js's shape), and `onEnd` gets null for a drag that
// was cancelled (pointercancel, a lost capture, Escape) or released over no
// target, which issues no command.

// How far a pressed pointer travels, in CSS pixels, before it is a drag.
export const DRAG_THRESHOLD = 8;

// The innermost element at a point of the viewport, looking into shadow roots.
export function deepElementFromPoint(root, x, y) {
  let element = root.elementFromPoint?.(x, y) ?? null;
  while (element?.shadowRoot?.elementFromPoint) {
    const inner = element.shadowRoot.elementFromPoint(x, y);
    if (!inner || inner === element) break;
    element = inner;
  }
  return element;
}

// The drop target an element is in, as a destination, or null. The walk goes
// up through slots and out of shadow roots.
export function destinationOf(element) {
  for (let node = element; node; node = node.assignedSlot ?? node.parentNode ?? node.host) {
    const kind = node.dataset?.drop;
    if (kind === "alone") return { kind };
    if ((kind === "room" || kind === "group") && node.dataset.dropId) return { kind, id: node.dataset.dropId };
  }
  return null;
}

export const destinationAt = (root, x, y) => destinationOf(deepElementFromPoint(root, x, y));

// `onStart(room)` when a press becomes a drag, `onOver(destination)` at each
// move (null over no target) and `onEnd(room, destination)` once, when it is
// over. `begin(event)` is given every pointerdown of the app.
export function createDrag({ root = document, onStart = () => {}, onOver = () => {}, onEnd = () => {} } = {}) {
  let drag = null;

  const stop = () => {
    const { handle, pointerId } = drag;
    handle.removeEventListener("pointermove", move);
    handle.removeEventListener("pointerup", up);
    handle.removeEventListener("pointercancel", cancel);
    handle.removeEventListener("lostpointercapture", cancel);
    root.removeEventListener("keydown", key, true);
    try {
      handle.releasePointerCapture?.(pointerId);
    } catch {
      // The capture is already gone.
    }
    drag = null;
  };

  function move(event) {
    if (!drag || event.pointerId !== drag.pointerId) return;
    if (!drag.moving) {
      if (Math.hypot(event.clientX - drag.x, event.clientY - drag.y) < DRAG_THRESHOLD) return;
      drag.moving = true;
      onStart(drag.room);
    }
    event.preventDefault();
    onOver(destinationAt(root, event.clientX, event.clientY));
  }

  function up(event) {
    if (!drag || event.pointerId !== drag.pointerId) return;
    const { room, moving } = drag;
    stop();
    if (!moving) return;
    // The release of a drag is not a click on the handle.
    const swallow = (click) => {
      click.stopPropagation();
      click.preventDefault();
    };
    root.addEventListener("click", swallow, true);
    setTimeout(() => root.removeEventListener("click", swallow, true), 0);
    onEnd(room, destinationAt(root, event.clientX, event.clientY));
  }

  function cancel(event) {
    if (!drag || (event && event.pointerId !== undefined && event.pointerId !== drag.pointerId)) return;
    const { room, moving } = drag;
    stop();
    if (moving) onEnd(room, null);
  }

  function key(event) {
    if (event.key === "Escape") cancel();
  }

  function begin(event) {
    if (drag || event.isPrimary === false || event.button > 0) return;
    const handle = event.composedPath().find((node) => node.dataset?.dragRoom);
    if (!handle) return;
    drag = {
      handle,
      room: handle.dataset.dragRoom,
      pointerId: event.pointerId,
      x: event.clientX,
      y: event.clientY,
      moving: false,
    };
    try {
      handle.setPointerCapture?.(event.pointerId);
    } catch {
      // No such active pointer (a synthetic event): the listeners below are
      // on the handle either way.
    }
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
    handle.addEventListener("pointercancel", cancel);
    handle.addEventListener("lostpointercapture", cancel);
    root.addEventListener("keydown", key, true);
  }

  return { begin, cancel: () => cancel(), active: () => Boolean(drag?.moving) };
}
