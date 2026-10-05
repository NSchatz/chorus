# feat(network): chorus speaker network (chorus, never merged by agents)

Branch: `chorus-g7/speaker-network` (the owner's homelab repo), commit `0d03e7c`, based on `main` at `d82e2ae`. Drafted by the chorus program, goal 7; not opened as a PR then, because the owner deferred the speaker-network decision at chorus's checkpoint. The owner accepted this placement on 2026-10-05, so it opens with this body. What remains is the owner's: the VLAN tag, the subnet, the pool and the server's address, filled in on this branch (step 1 of the owner action below).

## What changes

A network of its own for chorus speakers, which the server joins through its unused `eno1`. The speaker protocol and its time sync then stay on one switched segment, off the firewall and the trunk. chorus's own reasons: speakers are adopted automatically, trust-on-first-use, when they appear on this network (chorus K92), and chorus's control plane has no login of its own (chorus K40). So this network is the adoption boundary.

- `ansible/inventory/host_vars/opnsense.yml`: an `audio` entry in `opnsense_networks`, taking its values from a new `opnsense_audio` (tag, cidr, pool: **null, the owner's to fill in**). `audio` joins `opnsense_house_ifs`, which gives it DNS and NTP from the firewall (DHCP comes from Dnsmasq's default rules). Its subnet joins the `house_nets` alias, so no "internet" rule (`destination_not house_nets`) routes Trusted, Servers or the tunnels into it. No other rule names it (not to the server, the internet or any network, and none into it), and it stays out of the mDNS repeater.
- `ansible/roles/opnsense/tasks/main.yml`: an assert that refuses to run while `opnsense_audio` has a null tag, cidr or pool.
- the server's host_vars file: `base_audio_leg` (`eno1`, static, no gateway, **address null, the owner's**) and `firewall_audio_iface`.
- `ansible/roles/base` (tasks, handlers, new `templates/audio-leg.j2`): the leg's `interfaces.d` file and an ifup handler, the same pattern as the admin link, plus an assert that refuses to run while the address is null.
- `ansible/roles/firewall` (`defaults/main.yml`, `templates/host.nft.j2`): `firewall_audio_iface` (empty by default, which adds nothing), `firewall_audio_tcp: [4010]` and `firewall_audio_udp: [5353]`. When the interface is set, the input chain jumps to `audio_in` right after `lo` and ahead of every other rule. `audio_in` accepts 4010/tcp and 5353/udp and drops the rest, so the any-source 32400 rule and the `@docker4` source rule never answer on the leg. A `prerouting` chain at priority `raw` (ahead of Docker's DNAT) drops traffic arriving on the leg if it is addressed anywhere but the leg itself (`fib daddr . iif type != { local, broadcast, multicast }`) or comes from a source that does not route back out of it (`fib saddr . iif oif missing`). A speaker therefore cannot use the host as a router, and cannot reach the ports Docker publishes on `LAN_IP` through the leg.
- `docs/network.md`: the topology, a Networks row, a Firewall policy row and a sentence under it, mDNS (not repeated), Switch and Wi-Fi (the VLAN, the trunk and AP tagged lists, the server's leg port and the speaker ports, the 2.4 GHz speaker SSID and the UniFi fourth-SSID caveat), the edge table's host-firewall row, The server's NICs (`eno1` gets a role), and a new section, "The chorus speaker network".
- `docs/security.md`: a new section, "The chorus speaker network", with the reasoning (chorus K92, chorus K40) and what enforces it. The host firewall is the speaker-to-server filter, and the owner check below is its proof.
- `home-automation/mdns-reflector/.env.example`: a comment saying `MDNS_INTERFACES` never includes the leg.

chorus's speaker ports come from chorus's own source. 4010/tcp is the speaker protocol: the audio stream and its time sync share one connection per speaker, and the audio port is chorus decision record 0027. 5353/udp is mDNS. The control plane (4020) is not a speaker port and stays off the leg. chorus has no OTA listener yet; when it adds one, its port joins `firewall_audio_tcp`.

**Placeholders.** The PR adds no address: the tag, subnet, pool and the server's address on the network are null, marked `# OWNER:`. CI checks YAML, the Ansible syntax and lint, not values, so it accepts nulls. Both playbooks refuse to run until the values are filled in (the asserts), so merging it unfilled applies nothing half-configured. The netmask `255.255.255.0` is a default that matches homelab's /24 numbering, not an address.

**Depends on:** OPNsense's phase B (the VLANs exist) and chorus-server's deploy, #237 (the host network; its 4010-from-LAN rule stays for speakers still on the house networks). This branch is based on `main`, not on #237, and merges cleanly with it. Once speakers have moved, #237's LAN rule for 4010 and chorus-server's `--listen 0.0.0.0:4010` can narrow to the leg in a follow-up to the chorus-server stack.

## Owner action

**Owner: the chorus speaker network**

**Why**
Creates the chorus speaker network and puts the server on it (this PR). It applies after phase B has created the VLANs and after #237 has deployed chorus-server.

**Run** (in your own terminal; sudo asks for your password). Numbered steps, because the switch and UniFi steps are web UIs:

1. Pick the numbers from homelab's plan (a free VLAN tag, a /24, a pool in `.100-.249`, the server at a free `.2-.99`, the firewall at `.1`). Commit them on this branch: `opnsense_audio` (tag, cidr, pool) in `ansible/inventory/host_vars/opnsense.yml`, and `base_audio_leg.address` in the server's host_vars file. Optionally, put them in the "chorus speakers" row of `docs/network.md`.
2. A session in the owner's homelab repo (or you) runs `make ci` on the branch, merges it, and deploys with `scripts/update-all.sh`.
3. OPNsense:
   ```bash
   bash /opt/homelab/scripts/opnsense-apply.sh --check   # dry run: the VLAN, assignment, DHCP range, alias and two rule interface lists
   bash /opt/homelab/scripts/opnsense-apply.sh --apply
   ```
   Then, in the GUI (Interfaces > audio): enable it and set static IPv4 `.1/24` on the chosen subnet (26.7's API cannot). Run `--check` again, which should report no changes.
4. The server (eno1 still unplugged):
   ```bash
   sudo bash /opt/homelab/scripts/host-apply.sh --check          # dry run: interfaces.d/eno1 and the new chains in host.nft
   sudo bash /opt/homelab/scripts/host-apply.sh --apply TOKEN    # the token the check printed
   ```
   From a new SSH session, prove access, then keep the ruleset: `sudo systemctl stop homelab-rollback-firewall.timer`.
5. Switch (ES228GP UI): add the VLAN with the chosen tag. Tag it on port 1 (the trunk) and port 2 (the AP). Set a free port, then each speaker port, untagged on it (PVID = the tag). Only then, patch `eno1` into that free port. Save the configuration backup to `/opt/docker/config/network/`.
6. UniFi: Settings > Networks: a **VLAN Only** network with the tag (DHCP off). Add a 2.4 GHz WPA2 speaker SSID on it. If the controller refuses a fourth SSID on 2.4 GHz with meshing on, say so on the issue.

**Changes**
It creates a VLAN, an OPNsense interface, a DHCP range, one entry in `house_nets`, and `audio` in the DNS and NTP rules' interface lists. On the server it adds a static address on `eno1` (no gateway, no route change on `eno3`) and two chains in `table inet host`. Nothing is deleted. No container restarts. The firewall's 10-minute rollback timer is armed as usual.

**Check**
- The server: `ip -4 -br addr show eno1` shows the chosen address, and `ip route` shows no default route via `eno1`. `sudo nft list chain inet host audio_in` and `sudo nft list chain inet host audio_prerouting` show the rules.
- From a laptop on a speaker port (it takes a DHCP lease on the new subnet):
  - `nc -zv <leg address> 4010` connects (with chorus-server running).
  - `nc -zv -w 3 <leg address> 22`, `... 32400` and `... 4020` time out.
  - `nc -zv -w 3 <server's LAN address> 443` times out.
  - `ping` of a Trusted or Servers address and of an internet address fails.
  - `dig @<firewall .1 on the new subnet> example.com` answers.
- `journalctl -k | grep nft-host-audio-drop` shows the refused probes.
- From Trusted: the laptop's new address is unreachable.
- Say "done" here or in a session; the session reads the logs, checks the effect, and closes this.

**Unblocks**
chorus speakers' placement: auto-adoption (chorus K92) trusts only this network. Speakers use the leg's address as their configured server address.

## Rootless checks (on the branch at `0d03e7c`)

`make ci` itself runs each tool in a Docker container. It was not run here; these are the same tools at the owner's homelab repo's pins, run rootless.

yamllint (1.38.0, the workflow's `YAMLLINT_VERSION`), `uvx yamllint@1.38.0 --strict .`:
```
yamllint --strict . exit 0
```

Ansible syntax and lint (ansible-core 2.19.11, ansible-lint 26.9.0, Python 3.13, the workflow's pins and commands, on a copy of `ansible/`):
```
playbook: site.yml

playbook: opnsense.yml

Passed: 0 failure(s), 0 warning(s) in 105 files processed of 151 encountered. Profile 'production' was required, and it passed.
ansible exit 0
```

gitleaks 8.30.1 (`mise exec aqua:gitleaks/gitleaks@8.30.1 -- gitleaks`), the branch's commits and the working tree:
```
gitleaks git . --log-opts=origin/main..HEAD --redact --no-banner
INF scanned ~13492 bytes (13.49 KB) in 361ms
INF no leaks found
gitleaks git exit 0
gitleaks dir . --redact --no-banner
INF scanned ~4318510 bytes (4.32 MB) in 649ms
INF no leaks found
gitleaks dir exit 0
```

CI's inline Python checks ("Router policy" and "Compose security baseline", extracted from `.github/workflows/ci.yml`, PyYAML 6.0.2, Python 3.13, on a `git archive HEAD` export):
```
== Router policy
routers: 22, behind tinyauth: 17, tinyauth apps: 17
exit 0
== Compose security baseline
services checked: 45
exit 0
```

shellcheck: no shell script changed. Python syntax: no `.py` changed.

Template render (ansible-core 2.19.11, the role defaults under the host_vars, with example numbers from the documentation ranges): `host.nft.j2` renders `audio_prerouting` and `audio_in` for `eno1` and `iifname "eno1" jump audio_in` right after `iif "lo" accept`. `audio-leg.j2` renders a static stanza with no gateway. With the values null, the assert fails with its message. `nft -c` could not run rootless (netlink: "cache initialization failed: Operation not permitted"). The firewall role's `validate: nft -c -f %s` checks the file on the host before loading it.

Added-line scan (every `+` line of `git diff origin/main..HEAD`, 212 lines): no private address, MAC, SSID name or hostname was added. There are three hits, none new:
- line 14 is the existing `house_nets` list, which gains only `"{{ opnsense_audio.cidr }}"`;
- line 24 is the netmask `255.255.255.0`;
- line 106 is the existing "At a glance" row, which changes only its words.

No en or em dash, and chorus's proposal is named only by topic.

## Related open PRs

Checked against every open PR to the owner's homelab repo on 2026-09-30. `git merge-tree` of this branch with each PR's head: **no conflict with any of them**. Line numbers below are this branch's hunks against `main`, and each stacked PR's own hunks against its base branch. #229-#233 are stacked on #228.

- **#227** feat(ansible): OPNsense plugins (mDNS repeater among them). `host_vars/opnsense.yml`: it adds the plugin block after line 25, including `opnsense_mdns_ifs: [trusted, iot, servers]`; mine touches lines 59 (after the networks), 81 (`house_nets`) and 142 (`opnsense_house_ifs`). `roles/opnsense/tasks/main.yml`: it appends plugin tasks after line 38; mine inserts the assert after line 15. Semantic: `audio` must stay out of `opnsense_mdns_ifs`, and neither PR adds it.
- **#228** DNS moves to Unbound. `host_vars/opnsense.yml` line 43 (LEGACY's DNS) against mine at 59/81/142. the server's host_vars lines 11 and 13-14 (`base_dns`) against mine after line 20 (after `base_admin_link`). They are separate hunks.
- **#229** the server readdress. `host_vars/opnsense.yml` 20-22 and 43; the server's host_vars 8, 10-12, 14, 75 and 77 (address, gateway, firewall ranges); mine at 20 in the server's host_vars, and 59/81/142 in `opnsense.yml`. Separate hunks. Semantic: the leg's address is independent of the uplink's readdress (the "second address to keep straight" in phase B step 4).
- **#230** retire Technitium. `roles/firewall/defaults/main.yml` 5-9 (53 leaves the trusted lists); mine appends after line 14. `docs/network.md` 261-262 (the host firewall paragraph); mine touches 21, 37-41, 53, 68, 121, 126, 166, 234, 314, 322-335, 537-541 and a new section at the end. `docs/security.md` 19-20, 79-80 and 83; mine appends after line 120.
- **#231** retire the household WireGuard container. `host_vars/opnsense.yml` 105-112, 127-140 and 216 (WireGuard, port aliases, forwards, a rule); the server's host_vars 77-80 and 83 (`firewall_vpn4`, `firewall_docker4`). `docs/network.md` 244 and 267-268; `docs/security.md` 114. `firewall_audio_iface` sits beside `base_audio_leg` (line 29), not beside `firewall_docker4`, to keep this hunk clear of it.
- **#233** retire wireguard-friends. the server's host_vars 79-81 (`firewall_docker4` comment); `docs/network.md` 191-192, 247-249, 267-269 and 303. Separate hunks, as above.
- **#237** (chorus) deploy chorus-server on the host network. `roles/firewall/defaults/main.yml`: it inserts `firewall_lan_tcp_chorus` after line 13; mine appends after 14. `roles/firewall/templates/host.nft.j2`: it changes line 48 (the LAN TCP rule); mine adds chains before `chain input` (line 27) and the jump after `iif "lo"` (line 33). `docs/network.md` 229 (the Published ports row) against mine at 234 (the Host firewall row). `docs/security.md` 35 (the host network exception) against mine appended after 120. Semantic: this change assumes #237's host-networked chorus-server; the leg's `audio_in` handles 4010 on `eno1` before #237's LAN rule is reached.

Open PRs touching none of these files: #191, #192, #208, #224, #225.

The owner or a session in the owner's homelab repo runs `make ci` and merges (chorus K28). No agent merges, applies or deploys this.

🤖 Generated with [Claude Code](https://claude.com/claude-code)

https://claude.ai/code/session_01L4eqGA4w4V1k26wrVSgoKn

## What was read

Drafted by a chorus goal-7 agent on 2026-09-30. In the owner's homelab repo at `d82e2ae`: `CLAUDE.md`,
`README.md`, `docs/network.md`, `docs/security.md`, `docs/owner-actions.md`, the opnsense and
server host_vars files, `ansible/roles/base`, `ansible/roles/firewall` (`defaults/main.yml`,
`templates/host.nft.j2`), `ansible/roles/opnsense/tasks/main.yml`,
`home-automation/mdns-reflector/.env.example`, `.github/workflows/ci.yml`, and the file lists of
open PRs of the owner's homelab repo, #191, #192, #208, #224, #225, #227-#233 and #237. In NSchatz/chorus:
`docs/proposals/P3-speaker-network.md`, `crates/server/src/config.rs`, `crates/server/src/main.rs`,
`crates/discovery/src/net.rs`, `docs/protocol.md`, decision record 0027. No GPL source; homelab is
the owner's own repository.
