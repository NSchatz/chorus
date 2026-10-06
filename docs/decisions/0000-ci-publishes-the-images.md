# 0000: CI publishes the chorus-server and chorus-soloist images to ghcr.io, from a commit on main, with the job's own token

- Status: decided by the owner, 2026-10-06 (harness task 17, the homelab PR #237 deploy)
- Recorded by: the owner's agent harness, in the pull request that adds this record
- Implemented in: `.github/workflows/publish-images.yml`; `docs/release.md` ("Pushing the images
  to a registry"), `tools/image.sh` and `tools/soloist-image.sh` say the same

## Context

Homelab PR #237 (the chorus-server stack) was merged on 2026-10-03, and #248 (the Spotify
receivers) after it. homelab `main` pins `ghcr.io/nschatz/chorus-server:g17-a835a5f` by digest
(`sha256:4a5a6e51...3909`). On 2026-10-06 the owner ran the host steps and the pull was refused:
`ghcr.io` has no `chorus-server` package at all. Neither v0.1.0 nor `g17-a835a5f` was ever pushed,
because pushing was written as the owner's step (`docs/release.md`, K4, K41) and needed the
chorus tree, its pinned tools and a personal registry login on a workstation.

Asked whether to push by hand now, add a CI publish job, or wait, the owner chose the CI job.

## Decision

1. **`.github/workflows/publish-images.yml` builds and pushes both images.** It runs `make image`
   and `make soloist-image`, the same targets the gate builds and tests, then pushes each OCI
   layout unchanged with the pinned `crane`, so the digest the build prints is the digest served.
2. **Only a commit on main is published.** A release tag `v<ver>` publishes under `<ver>`; by hand
   (`workflow_dispatch`), `ref` and `tag` name the commit and the registry tag.
3. **A pinned digest is checked before anything is pushed.** The optional `server-digest` and
   `soloist-digest` inputs are the digests a homelab compose file pins; a build that differs fails
   the run with nothing pushed. A pin is never edited to match a build the owner did not ask for.
4. **The job's `GITHUB_TOKEN` (`packages: write`, this job only) authenticates.** No personal
   registry credential is stored or created; the rest of the repository's workflows stay
   `contents: read`.
5. **The packages and deploys stay the owner's.** A package is private when first pushed; making
   it public, or logging the host in, and every homelab deploy remain owner actions (K28).

## Not chosen

- **The owner pushes by hand** (the old rule): it left the images unpushed for a week and needs a
  personal write token on a workstation.
- **A personal access token as a repository secret:** a long-lived credential where an
  ephemeral, job-scoped one does the same work.
- **Building with `docker build` and a buildx action:** a second build path that could drift
  from the gate's daemonless, tested one.

## Sources

- crane push reference, "If the PATH is a directory, it will be read as an OCI image layout",
  https://github.com/google/go-containerregistry/blob/main/cmd/crane/doc/crane_push.md (read
  2026-09-30, as `docs/release.md` cites it).
- GitHub Docs, "Publishing and installing a package with GitHub Actions": `GITHUB_TOKEN` with
  `packages: write` pushes to `ghcr.io`,
  https://docs.github.com/en/packages/managing-github-packages-using-github-actions-workflows/publishing-and-installing-a-package-with-github-actions
  (read 2026-10-06).
- homelab PRs #237 and #248, their bodies and `media/chorus-server/docker-compose.yml` on homelab
  `main` (read 2026-10-06).
