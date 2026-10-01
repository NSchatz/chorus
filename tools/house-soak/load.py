#!/usr/bin/env python3
"""The house soak's control-plane side: the setup, a recorder of the state
stream, and the seeded command load (tools/house-soak-run.sh, ADR 0078).

Standard library only, so the harness needs nothing the gate does not already
have. Three modes, each a subcommand:

- `setup`: what the house is before any endpoint plays. The bonded rooms'
  endpoints are attached with `link: wired` (the Linux client attaches at
  catalog v1 and carries no link, and a v1 attach leaves a declared link as it
  is, `crates/control/src/zones.rs`), each bonded room is bonded FL/FR, and a
  bond in the wireless room is attempted and must be REFUSED naming the room
  (K91): the one refusal the setup expects.
- `subscribe`: holds one `GET /api/events` stream open for the whole run, as a
  phone's page would, and writes every state message it is sent with the
  CLOCK_MONOTONIC time it arrived. It is also a fanout subscriber the grade
  holds to zero drops.
- `run`: one command every `--interval-ms` for `--seconds`, drawn from the
  table below by a seeded generator (so a run is replayable from its seed and
  the server's answers), each built from the state the last answer carried, so
  most commands name a room or group that exists. Every answer is logged; a
  command the server does not know (`field` `t`, "is not a command") is counted
  as unknown and the load carries on, so the table can name commands a server
  build has not grown yet (the alarms and sleep runtime landed in parallel).
  Whenever an applied command changes what a room hears (its group, or its
  group's source) that is logged as a source switch, which is what the grade
  attributes an endpoint's underrun or hard resync to.

Every time here is CLOCK_MONOTONIC (`time.monotonic`); the wall clock is read
once, by the shell script, for the report's header.
"""

import argparse
import http.client
import json
import random
import socket
import sys
import time

# The table: each kind and its weight, the relative frequency it is drawn
# with. ASSUMED: volume paths dominate (what a household does most), the
# membership changes that move sessions between slots come next, and the
# configuration commands (limits, quiet hours, alarms, saved groups) are rarer.
TABLE = [
    ("volume", 8),
    ("volume_step", 8),
    ("group_volume", 5),
    ("group_volume_step", 5),
    ("mute", 4),
    ("join", 5),
    ("take", 4),
    ("ungroup", 4),
    ("group_save", 2),
    ("group_delete", 1),
    ("limit", 4),
    ("quiet_hours", 3),
    ("alarm_set", 2),
    ("alarm_stop", 1),
    ("alarm_delete", 1),
    ("sleep", 2),
]

def compact(value):
    """JSON with no optional whitespace, the way the catalog's vectors are written."""
    return json.dumps(value, separators=(",", ":"))


DAYS = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]


def post(control, body):
    """POST one command; return (http status, parsed answer or None)."""
    host, port = control.rsplit(":", 1)
    conn = http.client.HTTPConnection(host, int(port), timeout=10)
    try:
        conn.request(
            "POST",
            "/api/command",
            body=body.encode(),
            headers={"Content-Type": "application/json", "Host": "chorus"},
        )
        answer = conn.getresponse()
        raw = answer.read().decode("utf-8", "replace")
        try:
            parsed = json.loads(raw)
        except ValueError:
            parsed = None
        return answer.status, parsed
    finally:
        conn.close()


def get(control, path):
    host, port = control.rsplit(":", 1)
    conn = http.client.HTTPConnection(host, int(port), timeout=10)
    try:
        conn.request("GET", path, headers={"Host": "chorus"})
        answer = conn.getresponse()
        return answer.status, answer.read().decode("utf-8", "replace")
    finally:
        conn.close()


def setup(args):
    """Attach the bonded endpoints as wired, bond them, and probe K91."""
    bonded = [r for r in args.bonded.split(",") if r]
    out = open(args.out, "w", buffering=1)
    failures = 0

    def expect(body, want_status, want_field=None):
        nonlocal failures
        status, answer = post(args.control, body)
        field = (answer or {}).get("field")
        ok = status == want_status and (want_field is None or field == want_field)
        out.write(compact({"body": json.loads(body), "status": status, "field": field,
                              "detail": (answer or {}).get("detail"), "ok": ok}) + "\n")
        if not ok:
            failures += 1
            print(f"house-soak: setup command {body} answered {status} {field}", file=sys.stderr)

    for room in bonded:
        for side in ("l", "r"):
            expect(compact({"v": 2, "t": "attach", "zone": room,
                               "endpoint": f"{room}-{side}", "link": "wired"}), 200)
        expect(compact({"v": 2, "t": "bond", "zone": room, "members": [
            {"endpoint": f"{room}-l", "role": "FL"},
            {"endpoint": f"{room}-r", "role": "FR"}]}), 200)
    if args.wireless:
        w = args.wireless
        expect(compact({"v": 2, "t": "attach", "zone": w, "endpoint": w,
                           "link": "wireless"}), 200)
        # K91: a room declared wireless cannot hold a bond, refused naming the
        # room before any member is looked at (`Zones::bond`), so the second
        # member need not exist.
        expect(compact({"v": 2, "t": "bond", "zone": w, "members": [
            {"endpoint": w, "role": "FL"}, {"endpoint": f"{w}-probe", "role": "FR"}]}),
               400, "zone")
    out.close()
    return 1 if failures else 0


def subscribe(args):
    """Hold one event stream; write `<monotonic s> <state json>` per message."""
    host, port = args.control.rsplit(":", 1)
    s = socket.create_connection((host, int(port)))
    s.sendall(b"GET /api/events HTTP/1.1\r\nHost: chorus\r\nConnection: close\r\n\r\n")
    with open(args.out, "w", buffering=1) as out:
        held = b""
        while True:
            chunk = s.recv(65536)
            if not chunk:
                out.write(f"{time.monotonic():.6f} closed\n")
                return 0
            held += chunk
            while b"\n" in held:
                line, held = held.split(b"\n", 1)
                line = line.decode("utf-8", "replace").strip()
                if line.startswith("data: "):
                    out.write(f"{time.monotonic():.6f} {line[len('data: '):]}\n")


def hhmm(rng):
    return f"{rng.randrange(24):02d}:{rng.randrange(60):02d}"


def some_days(rng, allow_empty):
    days = [d for d in DAYS if rng.random() < 0.5]
    if not days and not allow_empty:
        days = [rng.choice(DAYS)]
    return days


class House:
    """What the last answer said: rooms, formed groups, saved groups, alarms."""

    def __init__(self, rooms):
        self.rooms = rooms
        self.groups = list(rooms)
        self.saved = []
        self.alarms = []
        self.hearing = {}

    def update(self, state):
        if not state or state.get("t") != "state":
            return
        self.groups = [g["id"] for g in state.get("groups", [])] or self.groups
        self.saved = [g["id"] for g in state.get("saved_groups", [])]
        self.alarms = [a["alarm"] for a in state.get("alarms", [])]

    @staticmethod
    def heard(state):
        """Each room's (group, that group's source): what its sessions hear."""
        sources = {g["id"]: g.get("source") for g in state.get("groups", [])}
        return {z["id"]: (z["group"], sources.get(z["group"])) for z in state.get("zones", [])}


def build(kind, rng, house, civil_day):
    """One command of `kind`, as JSON text, from the house as last seen."""
    room = rng.choice(house.rooms)
    group = rng.choice(house.groups)
    vol = f"{rng.randrange(1001) / 1000:.3f}"
    if kind == "volume":
        return f'{{"v":1,"t":"volume","zone":"{room}","volume":{vol}}}'
    if kind == "volume_step":
        return compact({"v": 2, "t": "volume_step", "zone": room,
                           "step": rng.randrange(-200, 201)})
    if kind == "group_volume":
        return f'{{"v":2,"t":"group_volume","group":"{group}","volume":{vol}}}'
    if kind == "group_volume_step":
        return compact({"v": 2, "t": "group_volume_step", "group": group,
                           "step": rng.randrange(-200, 201)})
    if kind == "mute":
        # Mostly unmuting, so the house is not left mostly silent.
        return compact({"v": 1, "t": "mute", "zone": room, "muted": rng.random() < 0.3})
    if kind == "join":
        target = rng.choice([r for r in house.rooms if r != room] + house.groups)
        return compact({"v": 2, "t": "join", "zone": room, "target": target})
    if kind == "take":
        command = {"v": 2, "t": "take", "target": rng.choice(house.rooms + house.saved)}
        r = rng.random()
        if r < 0.15:
            command["source"] = "none"
        elif r < 0.6:
            command["source"] = "stream"
        return compact(command)
    if kind == "ungroup":
        return compact({"v": 1, "t": "ungroup", "zone": room})
    if kind == "group_save":
        zones = rng.sample(house.rooms, rng.randrange(2, 5))
        n = rng.randrange(1, 4)
        return compact({"v": 2, "t": "group_save", "group": f"saved-{n}",
                           "name": f"Saved {n}", "zones": zones})
    if kind == "group_delete":
        return compact({"v": 2, "t": "group_delete",
                           "group": rng.choice(house.saved or ["saved-1"])})
    if kind == "limit":
        return f'{{"v":2,"t":"limit","zone":"{room}","limit":{rng.randrange(200, 1001) / 1000:.3f}}}'
    if kind == "quiet_hours":
        # Written by hand rather than by json.dumps, which would write a limit
        # of 0.25 where the catalog's vectors write three decimals.
        windows = []
        if rng.random() < 0.5:
            # A window that holds at the fixed civil time (`--civil-time`), so
            # an effective limit below the room's limit is exercised.
            windows.append(([civil_day], "00:00", "23:59", rng.randrange(100, 601)))
        for _ in range(rng.randrange(0, 2)):
            start, end = hhmm(rng), hhmm(rng)
            if start != end:
                windows.append((some_days(rng, False), start, end, rng.randrange(100, 601)))
        text = ",".join(
            '{"days":%s,"start":"%s","end":"%s","limit":%.3f}'
            % (compact(days), start, end, cap / 1000)
            for days, start, end, cap in windows)
        return '{"v":2,"t":"quiet_hours","zone":"%s","windows":[%s]}' % (room, text)
    if kind == "alarm_set":
        n = rng.randrange(1, 4)
        return (
            '{"v":2,"t":"alarm_set","alarm":"alarm-%d","target":"%s","time":"%s","days":%s,'
            '"source":"stream","volume":%s,"ramp_s":%d,"duration_min":%d,"enabled":%s}'
            % (n, rng.choice(house.rooms + house.saved), hhmm(rng),
               compact(some_days(rng, True)), vol, rng.randrange(0, 61),
               rng.randrange(1, 6), "true" if rng.random() < 0.8 else "false"))
    if kind == "alarm_stop":
        return compact({"v": 2, "t": "alarm_stop",
                           "alarm": rng.choice(house.alarms or ["alarm-1"])})
    if kind == "alarm_delete":
        return compact({"v": 2, "t": "alarm_delete",
                           "alarm": rng.choice(house.alarms or ["alarm-1"])})
    if kind == "sleep":
        return compact({"v": 2, "t": "sleep", "target": group,
                           "minutes": rng.randrange(0, 4)})
    raise ValueError(kind)


def run(args):
    rng = random.Random(args.seed)
    house = House(args.rooms.split(","))
    status, text = get(args.control, "/api/state")
    last = json.loads(text.split("\r\n\r\n", 1)[-1]) if status == 200 else {}
    house.update(last)
    hearing = House.heard(last) if last else {}
    kinds = [k for k, _ in TABLE]
    weights = [w for _, w in TABLE]
    log = open(args.out, "w", buffering=1)
    switches = open(args.switches, "w", buffering=1)
    start = time.monotonic()
    end = start + args.seconds
    i = 0
    while True:
        due = start + i * args.interval_ms / 1000.0
        if due >= end:
            break
        now = time.monotonic()
        if due > now:
            time.sleep(due - now)
        kind = rng.choices(kinds, weights)[0]
        body = build(kind, rng, house, args.civil_day)
        sent = time.monotonic()
        try:
            status, answer = post(args.control, body)
            error = None
        except OSError as e:
            status, answer, error = 0, None, str(e)
        answered = time.monotonic()
        field = detail = None
        verdict = "unanswered"
        if status == 200:
            verdict = "applied"
            house.update(answer)
            now_hearing = House.heard(answer)
            for room, heard in now_hearing.items():
                if hearing.get(room) != heard:
                    switches.write(compact({"mono": answered, "room": room,
                                               "from": hearing.get(room), "to": heard,
                                               "by": kind}) + "\n")
            hearing = now_hearing
        elif status in (400, 426) and answer:
            field, detail = answer.get("field"), answer.get("detail", "")
            unknown = field == "t" and "is not a command" in (detail or "")
            verdict = "unknown" if unknown else "refused"
        elif status:
            verdict = f"http-{status}"
        log.write(compact({"i": i, "sent": sent, "answered": answered, "kind": kind,
                              "status": status, "verdict": verdict, "field": field,
                              "detail": (detail or error or "")[:200],
                              "serial": (answer or {}).get("serial"), "body": body}) + "\n")
        i += 1
    # The window is the whole of `--seconds`, not up to the last command.
    now = time.monotonic()
    if now < end:
        time.sleep(end - now)
    log.write(compact({"done": True, "start": start, "end": time.monotonic(),
                          "commands": i}) + "\n")
    return 0


def main():
    p = argparse.ArgumentParser()
    sub = p.add_subparsers(dest="mode", required=True)
    s = sub.add_parser("setup")
    s.add_argument("--control", required=True)
    s.add_argument("--bonded", default="")
    s.add_argument("--wireless", default="")
    s.add_argument("--out", required=True)
    e = sub.add_parser("subscribe")
    e.add_argument("--control", required=True)
    e.add_argument("--out", required=True)
    r = sub.add_parser("run")
    r.add_argument("--control", required=True)
    r.add_argument("--rooms", required=True)
    r.add_argument("--seconds", type=float, required=True)
    r.add_argument("--interval-ms", type=int, required=True)
    r.add_argument("--seed", type=int, required=True)
    r.add_argument("--civil-day", required=True)
    r.add_argument("--out", required=True)
    r.add_argument("--switches", required=True)
    args = p.parse_args()
    return {"setup": setup, "subscribe": subscribe, "run": run}[args.mode](args)


if __name__ == "__main__":
    sys.exit(main())
