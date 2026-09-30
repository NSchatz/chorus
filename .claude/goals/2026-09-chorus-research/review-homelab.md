# Survey: NSchatz/homelab, for chorus planning

Read-only survey, 2026-09-29. Fresh clone of origin/main at `d82e2ae` (merge of #226), depth 50,
in `/cache/tmp/plan-2026-09-chorus/homelab-ro`. Nothing was pushed, commented, or opened.
Real IPs, MACs, key fingerprints, domain names and personal names in the repo are NOT copied
here; the domain is written `<domain>`. "Measured" means read live from the `claude-chorus`
container, which runs on this same host and shares its kernel (uname string identical to
README.md:16).

## 0. Headline for chorus

1. **There is no Proxmox and never was.** One bare-metal Dell R730xd runs Debian 13 with
   Docker Compose directly. No hypervisor, VM, LXC, KVM or libvirt anywhere in the tree. The only
   hit for "proxmox" is Homepage's stock empty `utilities/homepage/config/proxmox.yaml`. GitHub
   commit search for "proxmox" in the repo returns 0 (search coverage of private repos may be
   partial). chorus BRIEF.md:26, :37, :103 and chorus CLAUDE.md:38 ("deploys onto the Proxmox
   host") are wrong about this.
2. **The kernel is stock Debian `6.12.107+deb13-amd64`, `PREEMPT_DYNAMIC`, not PREEMPT_RT.**
   The command line is `root=ZFS=... ro quiet`, with no isolcpus, nohz_full or threadirqs. Nothing
   in the repo uses rtprio, memlock, SYS_NICE, cpusets or CPU pinning. There is no precedent.
3. **The homelab baseline forbids host networking** unless the service is on a named
   exception list, which `make ci` enforces name for name. Today only 2 services are on it (plex,
   mdns-reflector). BRIEF.md:238's recipe needs an owner decision to add chorus-server to that list.
   `cap_add: SYS_NICE` and `ulimits:` pass CI mechanically (CI does not inspect them), but the
   convention requires a justifying comment.
4. **The network in the brief (VLANs, Omada switch) is designed and staged, but NOT live.**
   Today the house is one flat /24 behind a UniFi USG, with an unmanaged switch and a single UniFi
   AC LR (Wi-Fi 5) AP. In the target design, IoT devices can reach the server ONLY on MQTT
   (1883/8883). General multicast is never routed between VLANs: only an mDNS repeater and an SSDP
   relay cross them.
5. **Music Assistant, Sendspin, Snapcast, Sonos, Cast, AirPlay receivers and every other audio
   service are absent.** The only media players in HA are 3 Roku TVs. HA runs as a container
   (2026.9.3, pinned by digest), not HA OS, so there are no add-ons.
6. **How other programs' PRs land.** They open homelab PRs titled "(<program>, never merged by
   agents)". They validate rootless, because a program container has no Docker daemon and
   `make ci` needs Docker. The owner or a homelab session then runs `make ci` and merges.

## 1. What the homelab is

### Hardware and OS
- One host, no cluster: Dell PowerEdge R730xd, BIOS 2.19.0 (CLAUDE.md:29-37; README.md:13-22).
- CPU: 2 x Xeon E5-2680 v4 (Broadwell-EP), 28c/56t, base 2.40 GHz, turbo 3.30 GHz. 2 NUMA nodes:
  node0 is the even CPUs, node1 the odd ones (docs/host-inventory.md:114-118; measured with lscpu).
- 160 GB DDR4 RDIMM. NVIDIA Quadro P2200 for Plex NVENC, with the driver pinned to the 580
  branch (590+ forbidden, CLAUDE.md:33-34). Coral M.2 TPU for Frigate. iDRAC8 (host-inventory.md:65).
- Storage: ZFS mirror `rpool` (2 x 2 TB HDD) holds the OS and all service data. Six separate ext4
  media and download disks hold about 39 TB. Docker's data-root is on md0 (README.md:21).
- NICs: 4 x Broadcom BCM5720 1 GbE. `eno3` is the ONLY uplink (1000 Mb/s, no bonding). `eno4`
  becomes the admin link to OPNsense. eno1 and eno2 are unused (host-inventory.md:173-186;
  docs/network.md:535-541).
- **Power: PSU1 has no AC input.** The host runs on PSU2 alone, with no redundancy
  (host-inventory.md:95-99, :415).
- OS: Debian 13 trixie, "stable kernels only, never backports" (README.md:16). systemd with
  cgroup v2 unified. Time sync is systemd-timesyncd; chrony is inactive (host-inventory.md:285-292).
- Docker CE 28.5.0, containerd 1.7.28, runc 1.3.3, Compose v2.39.4 (host-inventory.md:297-315).
  The `nvidia` runtime is registered. Sysbox has been purged (ansible/roles/docker/tasks/main.yml:1-66).
- Planned host disruptions from the owner's "overhaul" phases (the overhaul brief lives outside
  this repo):
  - P3: OPNsense and the VLAN cutover, including a server readdress. In progress.
  - P4: an offsite Pi.
  - P5: an SSD rebuild that reinstalls onto a new SSD mirror (docs/disaster-recovery.md:8).
  - P6: Docker `userland-proxy: false` (docs/network.md:285-291).

### How services are defined and deployed
- **Docker Compose only.** Each service is `<category>/<service>/docker-compose.yml`, with a
  committed `.env.example` and a gitignored `.env` (CLAUDE.md:59-69).
  - Categories: networking, monitoring, home-automation, media, utilities.
  - Count: 26 stacks and 45 services (docs/architecture.md:262). `claude/` holds generated
    session stacks that are never hand-edited (architecture.md:264-267).
  - Persistent data goes in `/opt/docker/config/<service>/`.
- **Host configuration is Ansible** (`ansible/site.yml`, with roles: apt, base, docker, firewall,
  nvidia, storage, sysctl, zfs, timers, textfile_collectors and more). The owner applies it with
  `sudo scripts/host-apply.sh --check`, then `--apply TOKEN` (README.md:68).
- **OPNsense is also Ansible**, driven through its API by `scripts/opnsense-apply.sh`, run as the
  owner's user over the admin link. It uses a custom module with no third-party collection
  (docs/network.md:343-376).
- **Deploys are GitOps.** `/opt/homelab` is a deploy-only checkout (CLAUDE.md:17-20).
  `scripts/update-all.sh` runs `git pull --ff-only`, then redeploys only the RUNNING stacks whose
  files changed, waiting on healthchecks with `--wait` (default timeout 600 s).
  - It never touches `claude/` or holdfast (scripts/update-all.sh:13-35).
  - **A new stack is not started by it** (update-all.sh:15, "A stopped stack stays stopped"), so
    the first `up -d` is a separate step.
- No Kubernetes, no Terraform, no Portainer. Arcane is a read and manage UI with a socket proxy.
- Renovate is configured (`renovate.json`: digest updates weekly, 3-day minimum release age), but
  the app is NOT installed. Image bumps are hand-made PRs (README.md:55-59; docs/security.md:47-52).

### Where the rules live
- `CLAUDE.md` (213 lines): operating rules, host, storage, layout, CI, deploy, network, services,
  security baseline and conventions. The conventions include "plain hyphens only, no en or em
  dashes" (CLAUDE.md:211), the same rule as chorus.
- `README.md`, plus 14 files in `docs/`: architecture, services, network, security,
  home-automation, owner-actions, backup-restore, disaster-recovery, host-inventory, and others.
- `.claude/settings.json`: a PreToolUse hook, `.claude/hooks/no-sudo.py`, refuses
  sudo/su/doas/pkexec/run0. The `owner-action` skill and `.github/ISSUE_TEMPLATE/owner-action.md`
  cover what sessions must hand off.
- **Owner-action contract** (docs/owner-actions.md:1-75; CLAUDE.md:9-16):
  - Anything needing root, hardware, money or an owner account is prepared, never done.
  - Root work becomes `scripts/owner/<verb>-<object>.sh` on `lib.sh`: a dry run unless `--apply`,
    a plan token before anything destructive, and a log in `~/homelab-logs/`.
  - Each such step gets one `owner-action` issue.
  - "Root never comes through Docker: no `--privileged`, `--cap-add` or `--net=host` trick"
    (CLAUDE.md:14-15). This is about sessions, but it colours how any host-network or
    capability request will be read.
- Open owner-action issues now: 8, all about the network cutover (#199, #202, #203, #211, #216,
  #217, #219, #222).

### Gate, CI, PRs, merging
- **`make ci` is the only gate.** GitHub Actions is off by owner decision.
  - Makefile:1-60 replays each `run:` step of `.github/workflows/ci.yml` in order, against a clean
    `git archive` export, with every tool in a pinned `ci-*` container. It needs a Docker daemon.
  - Steps: shellcheck, dotfiles tests, py_compile, `docker compose config -q` (with LAN_IP set to
    a documentation address), yamllint --strict, the router policy, the compose security
    baseline, ansible syntax and ansible-lint (production profile), promtool check and test, alloy
    validate and fmt, borgmatic config validate, recyclarr, HA `check_config` in the pinned HA
    image, actionlint, and gitleaks `dir` (ci.yml:36-330).
- No branch protection is possible: the repo is private on GitHub Free, and the API returns 403.
  The gate is procedural.
- **The repo is PRIVATE**, created 2026-03-24.
- **PR record** (last 120 PRs):
  - 114 merged, 6 open, 0 closed-unmerged in the recent window. Earlier closures: #51, #33, #34.
  - All are authored and merged under the single owner account, because agents use it, so
    `mergedBy` cannot tell a human from a session.
  - Branch prefixes: feat 58, fix 30, docs 15, chore 9, home 2, dev 2, holdfast-g2 1, and a few
    others.
- **Homelab's own sessions merge their own PRs after `make ci`.** Example: #226 says
  "`make ci`: PASSED in 224s".
- **Cross-program PR pattern** (#191 and #192 from `home/*`; #224 and #225 from `dev/*`; #208
  from `holdfast-g2/*`):
  - Title suffix "(<program>, never merged by agents)". #208 says "Do not merge from a session:
    the owner merges this".
  - Body sections:
    - What changes, and What to test in HA.
    - An owner action written in homelab's issue format. The foreign agent opens NO issue; a
      homelab session may.
    - "Related open PRs touching the same files", with a hunk-overlap analysis.
    - "Validation (rootless; this container has no Docker daemon)": yamllint 1.38.0, shellcheck
      0.10.0, the inline Python of the baseline and router-policy steps, `check_config` run from
      pip, gitleaks 8.28.0, and a self-run "address scan" of added lines (IPv4/IPv6, LAN
      hostnames, URLs, dashes).
  - The owner approves through PR comments that reference that program's checkpoint (#191
    comment, 2026-09-29).
- **Confirmed: the chorus container has no Docker daemon** (`docker ps` fails), so chorus is in
  the same position.
- The homelab session is DORMANT right now:
  `claude/personal/personal.conf:27-32` says "homelab set dormant 2026-09-29 (brought down to make
  room for chorus)".

## 2. Networking

### Today
- One flat /24 behind a UniFi USG, with an unmanaged switch and ONE UniFi AC LR AP (Wi-Fi 5,
  802.11ac) on one untagged SSID (docs/network.md:7-27).
- DNS is Technitium on the server. DHCP is the USG. The server is static on that /24.

### Design (staged, not live; phases A and B, network.md:378-505)
- **OPNsense 26.7** on a Celeron J6412 box with 4 x Intel i226.
- **TP-Link Omada ES228GP**: 24 PoE+ ports, run standalone from its own web UI, with NO Omada
  controller (decision 8; network.md:306-326). **The PoE budget is not recorded anywhere in the repo.**
- UniFi stays for Wi-Fi only.

### Networks (network.md:59-73)
- Mgmt: untagged, a /24.
- Trusted: VLAN 10, a /24 (phones, laptops, Roku TVs).
- Servers: VLAN 20, a /24 (the R730xd, static).
- IoT: VLAN 30, a /24 (printers, ecobee, **ESPHome devices**).
- Cameras: VLAN 40.
- Guest: VLAN 50.
- Three WireGuard tunnels on OPNsense (road warrior, friends, offsite), each a /24.
- A /30 admin link and a /24 rescue port.
- DHCP reservations by MAC live in `ansible/inventory/host_vars/opnsense.yml`.
- Wired house devices go on Trusted ports (switch port 5 onward, network.md:326).

### Firewall policy (network.md:100-124)
These rows matter for chorus:
- `Trusted -> server`: 443, 32400, 22, 32469, plus relayed SSDP.
- `Trusted -> IoT`: any.
- `Servers -> IoT`: any (server-initiated flows are fine).
- **`IoT -> server`: 1883 and 8883 ONLY.**
- Everything unlisted is blocked.

**Consequence:** a speaker on IoT cannot open a flow to chorus-server (control or audio UDP)
without a new OPNsense rule in `host_vars/opnsense.yml`. That is a homelab PR plus an
`opnsense-apply.sh` run. Server-initiated unicast to IoT works, and so do its stateful replies.

### Multicast and discovery (network.md:157-172)
- OPNsense's mDNS repeater joins Trusted, IoT and Servers (never Guest, Cameras or Mgmt).
- The UDP broadcast relay carries SSDP 239.255.255.250:1900 between Trusted and Servers.
- The plugins PR is #227, OPEN, draft until the WAN is online.
- No IGMP proxy, PIM or general multicast routing is designed. **Multicast audio cannot cross
  VLANs; only mDNS (5353) and SSDP are repeated.**
- The docs say "Discovery is a convenience; every integration points at a reserved address"
  (network.md:168-169). ESPHome's `use_address` is used precisely "so mDNS across VLANs is not
  needed" (home-automation.md:374-376).
- In Docker, `home-automation/mdns-reflector` (flungo/avahi, pinned by digest, `network_mode:
  host`) reflects 5353 between the LAN interface and the `homeassistant` bridge ONLY (compose file
  lines 1-16).

### Host firewall
- nftables `table inet host`, policy drop (ansible/roles/firewall/templates/host.nft.j2:28-62).
- **It governs only sockets on the host itself: host-network containers and sshd.** Docker
  publishes ports by DNAT and forward, so they never pass the input hook
  (host_vars/r730xd.yml:71-73).
- The LAN set accepts UDP 1024-65535 for Plex (host.nft.j2:50). Broadcast and multicast are
  dropped quietly after that (:60).
- `firewall_lan4` is the flat /24 today. After phase B it becomes Trusted plus Servers, **not IoT**
  (network.md:259-261). A host-network chorus-server would therefore need an IoT source rule in
  the Ansible firewall role, applied by the owner through `host-apply`.
- The trusted ports list is `firewall_trusted_tcp: [22, 53, 80, 443, 1883, 8883]`
  (firewall/defaults/main.yml:8-14).

### Other network pieces
- **Server edge** (network.md:225-235): every published port binds `${LAN_IP:?}` or 127.0.0.1,
  never 0.0.0.0, and CI enforces this (ci.yml:242-248).
- **Traefik v3.7.13** (networking/traefik/docker-compose.yml:3):
  - File provider only (`networking/traefik/dynamic/*.yml`). No Docker labels, and labels are
    rejected by CI (ci.yml:171-173).
  - Let's Encrypt through the Cloudflare DNS-01 challenge. TLS 1.2 minimum, X25519MLKEM768 first,
    HSTS.
  - **CI refuses any `tcp:` or `udp:` routers** (ci.yml:109-111), so chorus audio and control over
    raw TCP or UDP cannot go through Traefik. An HTTP or WebSocket UI can.
  - Every non-public router needs `lan-only` + `tinyauth` + an app entry in
    `networking/identity/tinyauth.yml` with an explicit group (Pocket ID passkeys), or CI fails
    (ci.yml:91-179).
- **Internal DNS**: Technitium today; Unbound on OPNsense in the design, with a wildcard override
  `*.services.<domain>` to the server and the local domain `lan.<domain>` (network.md:130-155).
- **NTP**: OPNsense will serve NTP to every internal network. The server keeps the Debian pool
  through timesyncd (network.md:163-164). No PTP, no chrony, no GPS reference.
- **UniFi Network Application 10.6.106** (LinuxServer image, with mongo 8.0.32), self-hosted, AP
  only after cutover. HA has a View-Only UniFi account.
- **IPv6**: none (decision 28).

## 3. Home Assistant, Music Assistant, audio

### Home Assistant
- **HA is a Docker container**: `ghcr.io/home-assistant/home-assistant:2026.9.3@sha256:...`
  (home-automation/homeassistant/docker-compose.yml:4). It is not HA OS or Supervised, so **there
  are no add-ons**. HACS is installed and used for Frigate and Moonraker.
- It is on bridge networks `homeassistant` + `proxy` (:80-82), not the host network. It is not
  privileged and keeps Docker's default capabilities, which is a listed exception.
  Limits: 4 CPUs, 4 GB (:64-66).
- **HA as code** (docs/home-automation.md:8-19):
  - `configuration.yaml` is a read-only bind, with `packages/`, `dashboards/` and `themes/`.
  - `integrations.json` (config-flow integrations) is applied by `scripts/ha-sync.py`.
  - `registry.json` (areas, labels, entities, Assist exposure) is applied by
    `scripts/ha-registry-sync.py`.
  - Both scripts are owner-run with sudo.
  - HA `check_config` is part of CI.
- **What is integrated** (14 entries in integrations.json): mqtt, sonarr, radarr, tautulli,
  qbittorrent, unifi, frigate (disabled), 3 x roku, ipp, homekit_controller (the ecobee), moonraker,
  and esphome (the garage door, disabled until the hardware exists).
- `registry.json` holds 10 areas and 2 floors ("Main floor", "Outside"). Open #224 and #225 add
  "lower level" devices, with area names pending.
- `registry.json` removes and ignores: plex, dlna_dms (Plex), smartthings, portainer, overseerr,
  wled, paperless.
- Assist and MCP exposure uses `expose_new: false` with an allowlist of regexes.
- HA is public at `homeassistant.<domain>` with its own login. **Anyone logged in can operate
  every enabled entity** (home-automation.md:188-196). A chorus `media_player` would be
  controllable from the internet-facing HA.

### MQTT
- `eclipse-mosquitto:2.0.22@sha256` (home-automation/mqtt/docker-compose.yml:3).
- Plaintext on `${LAN_IP}:1883`, password file, `allow_anonymous false`. Users: `homeassistant`
  and `frigate`. No websockets.
- Containers use `mosquitto:1883` on the `homeassistant` network.
- Plan: a TLS listener on 8883 with a 10-year private CA that devices pin, then unpublish 1883
  (docs/network.md:507-533).
- "ESPHome devices use Home Assistant's native API, not MQTT" (:533).

### Audio
- **None of these exists**: Music Assistant, Sendspin, Snapcast, shairport-sync, librespot or
  Spotify Connect, Mopidy, Jellyfin, Navidrome, LMS or Squeezebox, a Sonos integration, a Cast
  integration, PulseAudio or PipeWire. An rg sweep of the whole tree found none.
- Media playback today is Plex only, reached indirectly through Tautulli, plus 3 Roku TVs as
  `media_player` entities (home-automation.md:151, :181).
- The ecobee's AirPlay switch is disabled (registry.json).
- **Voice was tried and shelved.** #51 (Wyoming speech-to-phrase + Piper) was closed 2026-09-23.
  The owner's comment: "voice isn't worth running until the house has devices to control and a
  voice speaker". Also "No local LLM" (CLAUDE.md:154-155).

## 4. Real-time suitability for chorus-server

### Kernel and CPU (measured 2026-09-29 from `claude-chorus`, same host kernel)
- `uname -v` gives `#1 SMP PREEMPT_DYNAMIC Debian 6.12.107-1`. `/sys/kernel/realtime` is absent,
  so this is **not PREEMPT_RT**.
- `/proc/cmdline` is `root=ZFS=rpool/ROOT/debian ro quiet`. GRUB has
  `GRUB_CMDLINE_LINUX_DEFAULT="quiet"` (ansible/roles/zfs/files/grub.default). So there is no
  isolcpus, nohz_full, rcu_nocbs, threadirqs or `preempt=full`.
- cpufreq: intel_pstate in **passive** mode with the **schedutil** governor.
- cpuidle: intel_idle / menu. C1 (2 us), C1E (10 us) and **C6 (133 us exit latency) are
  enabled**; C3 is disabled.
- THP is `always`. The clocksource is `tsc`.
- `sched_rt_runtime_us` is 950000 of 1000000, the default RT throttling.
- The container reports `ulimit -r` = 0 and `ulimit -l` = 8192 KiB. CapEff is 0, since the
  session box is not a service.
- Kernel policy: "stable kernels only, never backports" (README.md:16; CLAUDE.md:30).
- Outside knowledge, not from the repo: Debian ships `linux-image-rt-amd64` in main, so it is not
  a backport. Any kernel change would also have to survive the ZFS dkms and the pinned NVIDIA 580
  dkms. That is an owner decision through an Ansible role plus `host-apply`.

### Container baseline vs BRIEF.md:238
The baseline is CLAUDE.md:195-207 and docs/security.md:5-37, and CI enforces it at
ci.yml:181-258.
- **Host network**: a deviation. It must be added to BOTH `docs/security.md` "Baseline
  exceptions" and ci.yml's `EXCEPT["network_mode host"]` (ci.yml:196). Today that list is plex
  (owner decision) and mdns-reflector.
- **`cap_add: SYS_NICE`**: CI checks only that `cap_drop: [ALL]` is present, not what `cap_add`
  holds. Convention requires a "minimal cap_add justified in a comment". Current use across the
  26 stacks:
  - DAC_OVERRIDE 17; CHOWN, FOWNER, SETUID and SETGID 16 each.
  - KILL 11, NET_BIND_SERVICE 3, NET_ADMIN 2, SYS_CHROOT 1.
  - **SYS_NICE, IPC_LOCK and SYS_RESOURCE: 0.**
  - security.md:17-19 adds that "images that run as a non-root user get none".
- **`ulimits: rtprio / memlock`**: CI does not check them, and **no service uses `ulimits:`,
  `cpuset`, `cpu_rt_*` or `sysctls`** except the two WireGuard stacks' sysctls.
  - Analysis, not repo text: a non-root process with RLIMIT_RTPRIO > 0 can take SCHED_FIFO up to
    that ceiling without CAP_SYS_NICE, and RLIMIT_MEMLOCK permits mlockall without IPC_LOCK.
  - Both work under `no-new-privileges`. So chorus-server could plausibly meet the baseline with
    **ulimits only and no extra cap**.
  - chorus's own `deploy/run-server.sh` already passes `--ulimit rtprio=20`
    (chorus docs/verification-record.md:793-795).
  - cgroup v2 has no Docker `--cpu-rt-runtime`.
- **`cpus:`, `mem_limit` and `pids_limit` are mandatory.** `cpus` becomes cgroup v2 `cpu.max`,
  which throttles SCHED_OTHER threads per 100 ms period. It does not cap SCHED_FIFO threads when
  RT group scheduling is off. Worth measuring: non-RT helper threads can stall up to one period.
- **Other rules**: healthcheck required; json-file logs 10m x 3; image `name:tag@sha256`; port
  binds on `${LAN_IP:?}` or 127.0.0.1.
- **Precedents**:
  - Host network: plex and mdns-reflector.
  - Privileged: smartctl-exporter and cadvisor.
  - Device passthrough: Frigate (Coral apex), wireguard (/dev/net/tun), ipmi-exporter (/dev/ipmi0).
  - GPU: Plex, via `runtime: nvidia`.
  - **None is real-time.**
- **Noisy neighbours on the same 56 threads**:
  - holdfast: `cpus: 24`, continuous ffmpeg + VMAF re-encoding of about 39 TB.
  - `cpus: 8` each: Plex (NVENC), Frigate (stopped), Kometa, Stirling-PDF, UniFi, cadvisor.
  - Claude session boxes: maker 12 CPUs; chorus 2 CPUs and 16 GB (claude/personal/docker-compose.yml:141-247).
  - This is expected to hurt tail latency: L3 and memory-bandwidth contention on a 2-socket NUMA
    host, plus C6 wakeups. The expectation is analysis; measuring it would be chorus's job.
- The only 1 GbE uplink is shared with Plex remote streams and every other service.

## 5. Secrets, conventions, adding a service

### Secrets
- There is no SOPS, age or Vault.
- Values live in each stack's gitignored `.env` (`.gitignore`: `.env*`, `!.env.example`) or in
  root-owned 0400 host files under `/opt/docker/config/<x>/` (ha-sync, oidc, homepage-secrets).
  Compose passes them as `credentials_file` or `file:` references (docs/security.md:54-64).
- gitleaks runs in an opt-in pre-commit hook (`.githooks/pre-commit`) and in `make ci`.
- `docs/secret-exposures.md` records past leaks by commit only: 3 generations of the Grafana admin
  password and 2 Gitea runner tokens. Its lesson: "Every past leak ... happened" by pasting a
  credential into a commit, PR or summary.
- There is no identity or PII scan in CI. Foreign programs self-scan for addresses in their PR
  bodies (#224).

### Adding a service
There is no template file. The conventions are CLAUDE.md plus CI:
- Files: `<category>/<service>/docker-compose.yml` and a `.env.example` holding generation
  instructions and no values.
- Image pinned `tag@sha256`. **First-party precedent**: `ghcr.io/nschatz/holdfast:v0.3.0@sha256:...`,
  pinned by the INDEX digest from `docker buildx imagetools inspect`
  (media/holdfast/docker-compose.yml:12-17).
- The baseline above, plus a healthcheck.
- Config in `/opt/docker/config/<service>/`, usually bound with `create_host_path: false`.
- A Traefik route in `dynamic/<svc>.yml`, plus a tinyauth app and group.
- A row in the `docs/architecture.md` inventory, and in docs/services.md.
- Any database gets a borgmatic dump hook (CLAUDE.md:114).
- A static Prometheus scrape job reaching the container by name on a shared network, plus alert
  rules with promtool tests.
- `ContainerDown` covers every container by name except an exclusion regex (rules.yml:127-129).
- The first `up -d` is manual, since update-all only redeploys running stacks.
- holdfast is the precedent for "deploy by hand, never by update-all" (CLAUDE.md:90-103).

### Backups
- borgmatic runs nightly at 03:00 into a local repository. The offsite repository is pending P4.
- A failure pushes through HA. `BackupStale` fires after 30 hours (CLAUDE.md:105-115).
- sanoid takes `rpool/opt` snapshots hourly.

## 6. Observability chorus could feed
- **Prometheus v3.13.3**: 60 s scrape and evaluation interval, **7-day retention**
  (monitoring/stack/docker-compose.yml:5, :27; prometheus.yml:2-3). Jobs are `static_configs` by
  container name.
- **Grafana 12.4.11**: file-provisioned dashboards (6 JSON) and alerting.
- **Loki 3.7.8**: 168 h retention (loki-config.yml:28).
- **Alloy v1.19.2**: auto-discovers EVERY Docker container's stdout through the socket, plus the
  systemd journal (config.alloy:10-78). **chorus-server's logs would land in Loki with no config.**
- Exporters: node-exporter (with textfile collectors), cAdvisor, blackbox, ipmi, smartctl,
  nvidia-gpu, exportarr. HA's `/api/prometheus` is scraped with a token file.
- **Alert path**: Prometheus `ALERTS` goes to ONE provisioned Grafana rule, then a webhook into
  HA, then a phone push. Critical and warning push at once; `info` goes into the Sunday digest.
  There is no Alertmanager, ntfy, Gotify or email, by owner decision (CLAUDE.md:180-185).
- There is no Uptime Kuma, OpenTelemetry collector or long-term metrics store. The 7-day retention
  means chorus soak measurements must be archived in chorus `docs/measurements/`, not left in
  Prometheus.

## 7. Other things that constrain or help chorus
- **No firmware tooling.** There is no ESPHome dashboard (owner decision), no OTA hosting, and
  nothing on the server builds or flashes firmware (home-automation.md:369-386).
  - ESPHome configs are archived in `/opt/docker/config/esphome-archive/` (`common/base.yaml`:
    IoT Wi-Fi, encrypted API, OTA password, `use_address`).
  - The server has no USB serial device and no Bluetooth adapter (HA compose :54-58).
  - A chorus OTA server would be a new service or a new decision.
- **No registry, no runners.** There is no container registry on the host (images come from GHCR
  and Docker Hub) and no CI runners (Actions is off; the Gitea runner was removed).
- **Device inventory is scattered**: OPNsense DHCP reservations (host_vars/opnsense.yml), HA
  `registry.json` devices, host-inventory.md for the server, and the devices program's PRs (#224,
  #225).
- **The chorus dev box** (claude/personal/docker-compose.yml:141-247) has 2 CPUs, 16 GB,
  `cap_drop ALL` plus the standard set, no devices and no USB, so it cannot flash an ESP32 or hold
  rtprio.
  - There is a generator precedent for passthrough: `--gpu` through CDI (personal.conf:34-39;
    claude/goals/gpu-passthrough.md).
  - A USB or rtprio flag would be a change to claude-containers, which lives outside this repo.
- **Wi-Fi reality check**: one Wi-Fi 5 AP, with no Wi-Fi 6 or 7 hardware recorded. This matters
  for chorus's wireless tier (chorus 1ea9f2a, S0051 wifi-7).
- **The brief vs the repo**:
  - BRIEF.md:36 ("Omada PoE+ switch; UniFi Wi-Fi; VLANs") describes the DESIGN, not today.
  - BRIEF.md:37 ("HA and an MQTT broker already running") is true.
  - Proxmox (BRIEF.md:26, :37, :103) is false.
- **Timing**: the network cutover (phases A and B, including the server moving to the Servers
  VLAN with new addresses) and the P5 SSD reinstall are both ahead. A chorus deploy should
  parameterize addresses (`LAN_IP` pattern) and expect a readdress.
