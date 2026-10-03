# Amendment to the 2026-09-chorus program: CI is the whole gate (the owner, 2026-10-03)

The owner decided this on 2026-10-03, in a session's chat: the repository was made public ("Just
chorus"), then **"Holdfast and chorus NEED to use the public CI and nothing local"**. These are the
owner's later words, so they beat the brief and the earlier amendments where they disagree (spec
amendment.md, "Precedence"). Like the 2026-10-02 owner amendment, this one changes the remaining
goal files themselves, by the owner's choice, and it applies at once: goal 17, in flight, included.

- **Spec version:** 1.1 (spec 1.1.7, gates.md "CI-only repos", holds for every pin).
- **In force for:** goal 17 from this merge on, and goals 18 to 27.

## What changes

1. **The merge rule (§0.3).** A PR merges when its CI (`make gate`, the one job of
   `.github/workflows/ci.yml`) is green on the branch up to date with `origin/main`, with the run's
   link, step timings and wall-clock in the PR body. The local `make tier-fast` under `goals-heavy`
   and `chorus-heavy` is gone from the rule.
2. **The gates (§0.3).** CI runs `make gate` on every PR, on every push to main and nightly on main;
   a goal's last merge needs the same green run. `make tier-fast` and `make gate-fast` stay as
   targets and gate nothing.
3. **Nothing runs here (§0.4).** No gate, tier, whole-workspace cargo build or test, or ESP-IDF image
   build on the development host; one crate's focused test is the inner loop. `chorus-heavy` covers
   the heavy work that stays (a QEMU run, a browser test, a soak).
4. **CI as it really behaves (§0.3).** Public again, so runs are free; the identity term list comes
   from the repository secret `CHORUS_IDENTITY_TERMS_LIST`, masked. CI is the merge condition.
5. **The goals supervisor.** Its drain runs nothing for chorus and takes no lock: GitHub merges main
   into the branch, the drain waits for `make gate` on that head and squashes exactly that head. Its
   nightly no longer runs chorus's full tier here.
6. **Goal files 17 to 27.** "self-merged only after the local gate passes with its tail in the PR"
   now reads "self-merged only after the PR's CI (`make gate`) is green on its branch up to date with
   origin/main, the run's link in the PR, and no gate or tier runs on this host (owner amendment
   2026-10-03)". Goal 21's line A now ends "pass in CI's make gate (run link)"; goal 27's line A now
   reads "make gate passes on CI on origin/main's head (run link, result and wall-clock)". The brief's
   sections for those goals carry the same lines.
7. **Recorded** in `docs/decisions/0135-ci-is-the-gate.md`; `CLAUDE.md` and `README.md` say the same.

## Left in force

Every other line of goals 17 to 27, `pull --rebase` before a PR, the 3 fix rounds and DROPPED, the
commit rules, the identity rules, the clean-room rule, the agent budget, and every earlier
amendment's top note.
