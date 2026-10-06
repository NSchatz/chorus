# 0235: CI cuts the release from a version tag on main, building with a read-only token and publishing from a separate job

- Status: decided, 2026-10-06 (harness task 259, project 37; the plan the owner approved that day)
- Recorded by: the owner's agent harness, in the pull request that adds this record
- Implemented in: `.github/workflows/release.yml`; `docs/release.md` ("Cutting one") says the same

## Context

Goal 27 (the finale) needs a release carrying every artifact. `docs/release.md` still cut one by
running `make release` on a workstation under the heavy lock, then `gh release create` by hand.
chorus CLAUDE.md forbids release and image builds on the agent host, and CI is already the gate
(ADR 0140) and already publishes the images from a release tag (ADR 0222). A release built by
hand was also one nobody else could see being built.

## Decision

1. **`.github/workflows/release.yml` builds the release.** A pushed tag `v<x.y.z>` runs
   `make release VERSION=<x.y.z>` on the tagged commit, with the same pinned toolchains
   `nightly.yml` provisions: mise from `mise.toml` and `mise.lock` (zig, cargo-zigbuild),
   `rust-toolchain.toml`, and ESP-IDF v6.1 at the commit `firmware/config/endpoint.conf` pins.
   `tools/release.sh` keeps every refusal it had (dirty tree, commit not on main, version not
   `Cargo.toml`'s, ESP-IDF not the pinned one, a directory that is not `--list`); the workflow
   adds none of its own beyond the tag being a version.
2. **Two jobs, and only the second can write.** The build job runs with `contents: read` and no
   credential left in `.git/config`, since the build runs third-party build scripts; it uploads
   `dist/v<ver>/` as the artifact `chorus-v<ver>`. The publish job, `contents: write` and nothing
   else, downloads that artifact, runs `gh release create v<ver> --verify-tag` with `NOTES.md` as
   the body and every other file attached, and fails unless the release lists exactly them. It
   never creates or moves a tag.
3. **A dry run by hand.** `workflow_dispatch` with `dry-run` (the default) builds the release of
   the ref it runs on with `Cargo.toml`'s version and stops after the upload. `dry-run: false`
   publishes only when run on a `v<x.y.z>` tag, to retry a release whose tag run failed.
4. **Tagging stays a deliberate act.** Pushing the tag is what publishes, as before; the same push
   has `publish-images.yml` push the images.

## Not chosen

- **A job in `publish-images.yml`:** it holds `packages: write`, and a release job beside it would
  either share that or need its own permissions block anyway; a separate workflow also keeps a
  release retry from pushing images again.
- **One job that builds and publishes:** the build would run third-party build scripts with a
  token that can write releases.
- **The workflow creating the tag:** the tag would then be made by whoever can dispatch, from
  whatever ref they name; a pushed annotated tag is the existing, visible step.

## What was read

- GitHub CLI manual, `gh release create`: "--verify-tag Abort in case the git tag doesn't already
  exist in the remote repository", https://cli.github.com/manual/gh_release_create (read
  2026-10-06).
- GitHub Docs, "Controlling permissions for GITHUB_TOKEN": permissions set on a job apply to that
  job alone,
  https://docs.github.com/en/actions/writing-workflows/choosing-what-your-workflow-does/controlling-permissions-for-github_token
  (read 2026-10-06).
