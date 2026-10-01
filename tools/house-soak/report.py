#!/usr/bin/env python3
"""Grade a house soak's run directory and write its report (ADR 0078).

    python3 tools/house-soak/report.py --run-dir <dir> [--out docs/measurements/<name>.md]

Reads only the raw files tools/house-soak-run.sh left in the run directory, so
a report can be regenerated from a kept run. Prints one line per criterion and
exits 0 when every one passes, 1 otherwise. Standard library only.

HOST SOFTWARE SOAK ON ALSA NULL, NOT A HARDWARE MEASUREMENT AND NOT TIMING
EVIDENCE: no number here says when a sample would reach a DAC, and the report
says so in those words.

Every bound below that is not chorus's own documented shape is ASSUMED and the
report prints it as such:

- RSS_GROWTH_BOUND_KB: a process may grow at most 8 MiB over the window, from
  its first sample after the warm-up to its last. ASSUMED: about twice what a
  debug build's allocator settles into on this host in the short run; a leak
  of a buffer per command (one every 2 s) would pass it within minutes.
- RSS_WARMUP_S: the first 30 s of the window are the warm-up the growth is not
  measured over (queues fill, the allocator settles). ASSUMED.
- ATTRIBUTION_WINDOW_S: an endpoint's underrun or hard resync within 5 s of a
  logged source switch of its own room is attributed to that switch. ASSUMED:
  the client's delay-log timeline starts within a second of the harness's
  launch stamp, and a switch's effects are over within a few start fills.
"""

import argparse
import hashlib
import json
import os
import re
import sys

RSS_GROWTH_BOUND_KB = 8 * 1024
RSS_WARMUP_S = 30.0
ATTRIBUTION_WINDOW_S = 5.0
LIMIT_SLACK = 0.0005  # half the catalog's 0.001 step, for a three-decimal print


def kv_file(path):
    out = {}
    with open(path) as f:
        for line in f:
            line = line.rstrip("\n")
            if "=" in line:
                k, v = line.split("=", 1)
                out[k] = v
    return out


def fields(text):
    return dict(m.groups() for m in re.finditer(r"(\w+)=(\S+)", text))


def jsonl(path):
    if not os.path.exists(path):
        return []
    with open(path) as f:
        return [json.loads(line) for line in f if line.strip()]


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def population(path):
    """`<mono> <name> <pid> <threads> <rss_kb>` lines, by name."""
    out = {}
    with open(path) as f:
        for line in f:
            parts = line.split()
            if len(parts) == 5:
                out[parts[1]] = {"pid": parts[2], "threads": int(parts[3] or 0),
                                 "rss_kb": int(parts[4] or 0)}
    return out


class Grade:
    def __init__(self):
        self.rows = []

    def add(self, name, ok, result, bound):
        self.rows.append((name, ok, result, bound))
        print(f"{'pass' if ok else 'FAIL'} {name}: {result}")

    @property
    def passed(self):
        return all(ok for _, ok, _, _ in self.rows)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--run-dir", required=True)
    p.add_argument("--out")
    args = p.parse_args()
    run = args.run_dir
    P = kv_file(os.path.join(run, "params"))
    start, end = float(P["start_mono"]), float(P["end_mono"])
    soak = int(P["soak_seconds"])
    rooms = P["rooms"].split(",")
    g = Grade()

    endpoints = {}  # id -> room, launch mono
    with open(os.path.join(run, "launch")) as f:
        for line in f:
            ident, room, at = line.split()
            endpoints[ident] = {"room": room, "launch": float(at)}

    # --- 1. the wall clock and the window --------------------------------------
    duration = end - start
    load = jsonl(os.path.join(run, "commands.jsonl"))
    done = next((c for c in load if c.get("done")), None)
    g.add("duration", duration >= soak and done is not None,
          f"{duration:.1f} s of load window by CLOCK_MONOTONIC ({P['start_wall']} to "
          f"{P['end_wall']}); the load itself ran {done['end'] - done['start']:.1f} s"
          if done else f"{duration:.1f} s; the load never finished",
          f"at least {soak} s")

    # --- 2. the thread population against /proc ---------------------------------
    pop0 = population(os.path.join(run, "population-start"))
    pop1 = population(os.path.join(run, "population-end"))
    shape = 6 + 2 * int(P["max_clients"]) + int(P["control_workers"])
    s0, s1 = pop0["server"]["threads"], pop1["server"]["threads"]
    g.add("server thread population", s0 == s1 == shape,
          f"{s0} at the start, {s1} at the end",
          f"unchanged and equal to the documented 6 + 2N + M = {shape} "
          f"(N = --max-clients {P['max_clients']}, M = --control-workers {P['control_workers']})")
    moved = [f"{e} {pop0[e]['threads']}->{pop1.get(e, {}).get('threads')}"
             for e in endpoints if pop0.get(e, {}).get("threads") != pop1.get(e, {}).get("threads")]
    per = sorted({pop0[e]["threads"] for e in endpoints if e in pop0})
    g.add("endpoint thread populations", not moved,
          "every endpoint unchanged (" + ", ".join(str(n) for n in per) + " threads)"
          if not moved else "changed: " + "; ".join(moved),
          "each endpoint's count at the end equals its count at the start")

    # --- 3. RSS ----------------------------------------------------------------
    series = {}
    with open(os.path.join(run, "rss.tsv")) as f:
        for line in f:
            parts = line.split()
            if len(parts) == 5 and parts[4].isdigit() and int(parts[4]) > 0:
                series.setdefault(parts[1], []).append((float(parts[0]), int(parts[4])))
    for name, row in pop1.items():
        if row["rss_kb"] > 0:
            series.setdefault(name, []).append((end, row["rss_kb"]))
    rss_rows = []
    for name in ["server"] + sorted(endpoints):
        pts = sorted(t for t in series.get(name, []) if t[0] >= start + RSS_WARMUP_S)
        if len(pts) < 2:
            # Too few samples to say anything: graded as a failure, not skipped.
            rss_rows.append((name, None, None, None, None))
            continue
        first, last = pts[0][1], pts[-1][1]
        rss_rows.append((name, first, last, max(v for _, v in pts), last - first))
    sampled = all(r[4] is not None for r in rss_rows)
    worst = max(r[4] for r in rss_rows) if sampled else None
    rss_ok = sampled and worst <= RSS_GROWTH_BOUND_KB
    server_row = rss_rows[0]
    g.add("RSS growth", rss_ok,
          f"largest growth {worst} KiB; server {server_row[1]} -> {server_row[2]} KiB "
          f"(peak {server_row[3]})" if sampled else "too few samples after the warm-up",
          f"every process at most {RSS_GROWTH_BOUND_KB} KiB (ASSUMED) from its first sample "
          f"after a {RSS_WARMUP_S:.0f} s warm-up (ASSUMED) to its last")

    # --- 4. the control fanout --------------------------------------------------
    with open(os.path.join(run, "report-end")) as f:
        report_line = f.read().strip()
    rep = fields(report_line)
    drops = {k: int(rep.get(k, -1)) for k in
             ("dropped_subscribers", "dropped_messages", "stalled_dropped", "turned_away")}
    states = []
    closed_early = False
    with open(os.path.join(run, "states.log")) as f:
        for line in f:
            at, _, body = line.rstrip("\n").partition(" ")
            if body == "closed":
                closed_early = closed_early or float(at) < end
                continue
            try:
                states.append((float(at), json.loads(body)))
            except ValueError:
                pass
    g.add("control fanout drops", all(v == 0 for v in drops.values()) and not closed_early,
          ", ".join(f"{k}={v}" for k, v in drops.items())
          + f"; the soak's own subscriber received {len(states)} states and "
          + ("was cut off before the end" if closed_early else "held its stream to the end"),
          "all zero (GET /api/report at the end of the window), the subscriber never dropped")

    # --- 5. volume never above its effective limit -------------------------------
    state_bad = []
    capped_states = 0
    for at, s in states:
        for z in s.get("zones", []):
            if z["volume"] > z["effective_limit"] + 1e-9 or z["effective_limit"] > z["limit"] + 1e-9:
                state_bad.append(f"serial {s.get('serial')} {z['id']} volume {z['volume']} "
                                 f"effective_limit {z['effective_limit']} limit {z['limit']}")
            if z["effective_limit"] < z["limit"]:
                capped_states += 1
    rv_total = rv_capped = 0
    rv_bad = []
    ramps = 0
    delay = {}
    for ident, e in endpoints.items():
        events = []
        path = os.path.join(run, f"endpoint-{ident}.log")
        if os.path.exists(path):
            with open(path) as f:
                for line in f:
                    if not line.startswith("event "):
                        continue
                    ev = fields(line)
                    ev["at"] = e["launch"] + int(ev.get("mono_us", 0)) / 1e6
                    events.append(ev)
                    if ev.get("kind") == "room-volume":
                        rv_total += 1
                        gain, limit = int(ev["gain"]), int(ev["limit"])
                        settles = float(ev["settles_at"])
                        rv_capped += limit < 1000
                        ramps += int(ev.get("ramp_ms", 0)) > 0
                        if gain > limit or settles > limit / 1000 + LIMIT_SLACK:
                            rv_bad.append(f"{ident} gain {gain} limit {limit} settles_at {settles}")
        delay[ident] = events
    g.add("volume within the effective limit", not state_bad and not rv_bad and rv_total > 0,
          f"{len(states)} states x {len(rooms)} rooms (a room under an active quiet-hours cap "
          f"{capped_states} times), {rv_total} room_volume messages taken by the endpoints "
          f"({rv_capped} with a limit below 1000, {ramps} with a ramp); violations: "
          + ("; ".join((state_bad + rv_bad)[:5]) if state_bad or rv_bad else "none"),
          "every state's volume <= effective_limit <= limit, and every room_volume an "
          "endpoint logged has gain <= limit and settles at or below it")

    # --- 6. underruns and resyncs, attributed or not ------------------------------
    switches = jsonl(os.path.join(run, "switches.jsonl"))
    by_room = {}
    for sw in switches:
        by_room.setdefault(sw["room"], []).append(sw["mono"])
    counts = {"underrun": [0, 0, 0], "hard-resync": [0, 0, 0]}  # before, attributed, not
    unexplained = []
    for ident, events in delay.items():
        room = endpoints[ident]["room"]
        for ev in events:
            kind = ev.get("kind")
            if kind not in counts:
                continue
            if ev["at"] < start:
                counts[kind][0] += 1
            elif any(abs(ev["at"] - t) <= ATTRIBUTION_WINDOW_S for t in by_room.get(room, [])):
                counts[kind][1] += 1
            elif ev["at"] <= end:
                counts[kind][2] += 1
                unexplained.append(f"{ident} {kind} at +{ev['at'] - start:.1f} s")
    summaries = {}
    for ident in endpoints:
        path = os.path.join(run, f"endpoint-{ident}.out")
        text = open(path).read() if os.path.exists(path) else ""
        m = re.search(r"chorus-client: underruns=(\d+)", text)
        summaries[ident] = int(m.group(1)) if m else None
    g.add("underruns and resyncs", not unexplained,
          f"underruns: {counts['underrun'][1]} at a logged source switch, "
          f"{counts['underrun'][2]} not ({counts['underrun'][0]} before the window); "
          f"hard resyncs: {counts['hard-resync'][1]} at a switch, {counts['hard-resync'][2]} "
          f"not ({counts['hard-resync'][0]} before the window); {len(switches)} source switches "
          f"logged; the endpoints' own underrun totals sum to "
          f"{sum(v for v in summaries.values() if v is not None)}"
          + ("; unattributed: " + "; ".join(unexplained[:5]) if unexplained else ""),
          f"none inside the window that is not within {ATTRIBUTION_WINDOW_S:.0f} s (ASSUMED) "
          "of a logged source switch of the endpoint's own room")

    # --- 7. sessions -------------------------------------------------------------
    with open(os.path.join(run, "server.log")) as f:
        server_log = f.read()
    with open(os.path.join(run, "state-end.json")) as f:
        body = f.read()
    final = json.loads(body.split("\r\n\r\n", 1)[-1])
    present = {z["id"]: z.get("present", []) for z in final.get("zones", [])}
    session_bad = []
    for ident, e in endpoints.items():
        n = len(re.findall(rf"client session peer=\S+ id={re.escape(ident)} ", server_log))
        text = open(os.path.join(run, f"endpoint-{ident}.out")).read()
        clean = re.search(r"stopped reason=run-length-reached sessions=1 ", text) is not None
        if n != 1 or ident not in present.get(e["room"], []) or not clean:
            session_bad.append(f"{ident}: {n} sessions, present={ident in present.get(e['room'], [])}, "
                               f"clean stop={clean}")
    g.add("sessions", not session_bad,
          f"all {len(endpoints)} endpoints present in their rooms at the end of the window, "
          "each on the one session it opened, each stopped by its run length"
          if not session_bad else "; ".join(session_bad),
          "every endpoint up at the end: one session for the whole run, present in its room, "
          "a clean stop")

    # --- 8. the command load ------------------------------------------------------
    cmds = [c for c in load if "kind" in c]
    tally = {}
    for c in cmds:
        tally.setdefault(c["kind"], {"applied": 0, "refused": 0, "unknown": 0, "other": 0})
        v = c["verdict"]
        tally[c["kind"]][v if v in ("applied", "refused", "unknown") else "other"] += 1
    other = sum(t["other"] for t in tally.values())
    applied = sum(t["applied"] for t in tally.values())
    refused = sum(t["refused"] for t in tally.values())
    unknown = sum(t["unknown"] for t in tally.values())
    expected = -(-soak * 1000 // int(P["interval_ms"]))
    g.add("command load answered", other == 0 and len(cmds) == expected,
          f"{len(cmds)} sent: {applied} applied, {refused} refused by the catalog's rules, "
          f"{unknown} unknown to this server, {other} unanswered or answered otherwise; the "
          f"server counts applied={rep.get('applied')} refused={rep.get('refused')} "
          "(its count includes the setup and every endpoint's attach)",
          f"{expected} commands (one every {P['interval_ms']} ms), every one answered 200, 400 "
          "or 426")
    setup = jsonl(os.path.join(run, "setup.jsonl"))
    g.add("house setup", all(s["ok"] for s in setup),
          f"{sum(1 for s in setup if s['ok'])} of {len(setup)} as expected, including the "
          "wireless room's bond refused naming the room (K91)",
          "the bonded endpoints attached wired, both bonds applied, the wireless bond refused")

    # --- the raw files ---------------------------------------------------------------
    raw = []
    for name in sorted(os.listdir(run)):
        path = os.path.join(run, name)
        if os.path.isfile(path):
            raw.append((name, os.path.getsize(path), sha256(path)))

    print("house soak: " + ("PASS" if g.passed else "FAIL") + f" ({P['label']}, {duration:.1f} s)")
    if args.out:
        write(args.out, run, P, g, tally, rss_rows, raw, duration, endpoints, report_line)
    return 0 if g.passed else 1


def write(out, run, P, g, tally, rss_rows, raw, duration, endpoints, report_line):
    short = P["label"] != "one-hour soak"
    rooms = P["rooms"].split(",")
    bonded = P["bonded"].split(",")
    L = []
    title = "House soak: 8 rooms, one --slots 8 server, ALSA null"
    if short:
        title += f" ({P['label']})"
    L += [f"# {title}", "", "Source: host", f"Build measured: `{P['build']}`",
          f"Date: {P['start_wall'][:10]}",
          f"Duration: {duration:.1f} s of load window, measured by the harness on "
          f"CLOCK_MONOTONIC ({P['start_wall']} to {P['end_wall']})",
          f"Run label: {P['label']}",
          "Generated by: `tools/house-soak-run.sh` (`make verify-house-soak`), report by "
          "`tools/house-soak/report.py`", ""]
    L += ["**HOST SOFTWARE SOAK ON ALSA NULL, NOT A HARDWARE MEASUREMENT AND NOT TIMING "
          "EVIDENCE** (BRIEF.md section 3.1 rule 3). Every process ran on one development host "
          "over loopback; every endpoint played into ALSA's `null` device, which accepts every "
          "frame at once and reports a delay of zero, so nothing here says when a sample would "
          "have reached a DAC, and no sync bound is graded. What it grades is that a whole house "
          "keeps working under a steady command load for as long as it ran: threads, memory, "
          "fanout, limits and sessions. The three-day hardware soak (AC-4, `tools/soak-run.sh`) "
          "is a different run on real endpoints and is not this.", ""]
    if short:
        L += [f"**This is the harness's {P['label']} ({int(P['soak_seconds'])} s), the test the "
              "harness landed with. It is NOT the one-hour soak goal 11's line E asks for**; that "
              "run is `CHORUS_HOUSE_SOAK_SECONDS=3600 make verify-house-soak`, written to "
              "`docs/measurements/house-soak-8-rooms.md`.", ""]
    L += ["## Result", "", "| criterion | result | bound | verdict |", "|---|---|---|---|"]
    for name, ok, result, bound in g.rows:
        L.append(f"| {name} | {result} | {bound} | {'pass' if ok else 'FAIL'} |")
    L += ["", f"Overall: **{'PASS' if g.passed else 'FAIL'}**.", ""]
    if P.get("build_same_tree") == "yes" and P["build"] != P["head"]:
        L += [f"The binaries were built from `{P['head']}`, whose `crates/`, `Cargo.toml`, "
              f"`Cargo.lock`, `rust-toolchain.toml` and `config/` are identical to the build "
              "named above (the harness checks it with `git diff --quiet`); only files outside "
              "the build differ.", ""]
    L += ["## What ran", "",
          f"- One `chorus-server` ({P['profile']} build): `--slots 8 --control-listen "
          f"--serve-forever --source tone --max-clients {P['max_clients']} --control-workers "
          f"{P['control_workers']} --civil-time {P['civil_time']} --ephemeral-identity`, the "
          f"repository's stream contract from `config/verification.conf`, and "
          f"`{P['contract_args']}` (this host grants no real-time priority and less locked "
          "memory than the server wants, so it says so in every status line).",
          f"- Eight rooms (ASSUMED names, the house of `docs/measurements/sim-house-8-rooms.md`; "
          f"the owner's room list is an open Needs item): {', '.join(rooms)}; `{P['wireless']}` "
          "declared wireless (`--zone " + P["wireless"] + "=wireless`).",
          f"- {len(endpoints)} `chorus-client` endpoints on ALSA `{P['device']}` (the client "
          "has no fake sink by design), each with `--rejoin`, its own delay log and "
          "`--ephemeral-identity`: " + ", ".join(
              f"{e} ({v['room']})" for e, v in sorted(endpoints.items())) + ". "
          f"{' and '.join(bonded)} each hold two, attached `link: wired` and bonded FL/FR "
          f"before they played; `{P['wireless']}`'s endpoint runs `--transport wireless`.",
          f"- libasound: `{P['libasound']}`.",
          f"- One event-stream subscriber for the whole run, and the seeded command load "
          f"(`tools/house-soak/load.py`): seed {P['seed']}, one command every "
          f"{P['interval_ms']} ms for {P['soak_seconds']} s, each built from the state the last "
          "answer carried. Commands the server does not know would be counted as unknown and "
          "the load would carry on.",
          "- RSS and thread counts of every process off `/proc/<pid>/status` every 10 s "
          "(ASSUMED period).", "",
          "## Host", "",
          f"- Kernel {P['kernel']}; {P['cpus_online']} CPUs online; cgroup `cpu.max` "
          f"`{P['cpu_quota']}`; {P['cpu_model']}; {int(P['mem_total_kb']) // 1024} MiB of memory.",
          f"- Every process on loopback; the soak shared the host with whatever else ran on it.",
          "", "## Parameters", "", "| parameter | value |", "|---|---|"]
    for k in ("label", "soak_seconds", "bound_seconds", "seed", "interval_ms", "civil_time",
              "rooms", "bonded", "wireless", "slots", "max_clients", "control_workers",
              "device", "profile", "build", "head", "build_same_tree", "dirty"):
        L.append(f"| {k} | `{P[k]}` |")
    L += ["", "## The command load", "",
          "| command | applied | refused | unknown to the server | other |", "|---|---|---|---|---|"]
    for kind in sorted(tally):
        t = tally[kind]
        L.append(f"| {kind} | {t['applied']} | {t['refused']} | {t['unknown']} | {t['other']} |")
    L += ["", "A refusal is the catalog doing its job (an alarm or saved group that is not there, "
          "a join a room is already in): each is answered `400` naming the field, and the state is "
          "unchanged.", "", f"The server's own report at the end of the window: `{report_line}`.",
          "", "## Memory", "", "| process | RSS after warm-up KiB | RSS at end KiB | peak KiB | growth KiB |",
          "|---|---|---|---|---|"]
    for name, first, last, peak, growth in rss_rows:
        L.append(f"| {name} | {first} | {last} | {peak} | {growth} |")
    L += ["", "## Not graded here", "",
          "- Any timing: inter-device error, latency, the audio thread's tick against its "
          "deadline (the server does not count tick overruns yet; ADR 0077 leaves measuring the "
          "tick at S = 32 as a follow-up). ALSA `null` reports no delay.",
          "- The alarms, sleep and line-in runtime as they fire: the load configures alarms and "
          "sleep timers, and whether they fire is the runtime's (the civil time is held fixed).",
          "- Real endpoints, a real network, Wi-Fi, the C endpoint.", "",
          "## Raw files", "",
          f"Kept in the run directory, `{run}`, not committed (the delay logs alone are megabytes "
          "an hour). Each file's sha256:", "", "| file | bytes | sha256 |", "|---|---|---|"]
    for name, size, digest in raw:
        L.append(f"| {name} | {size} | `{digest}` |")
    L.append("")
    text = "\n".join(L)
    with open(out, "w") as f:
        f.write(text)


if __name__ == "__main__":
    sys.exit(main())
