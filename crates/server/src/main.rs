//! `chorus-server`: take PCM in, put timestamped chunks on a socket, under a
//! host contract it states out loud.
//!
//! # Exit codes, which are the contract a script grades
//!
//! - `0`  the source ended cleanly, the end-of-stream signal was sent, and the
//!   client was served.
//! - `2`  the configuration was refused, including an unsupported format, a
//!   channel count with no channel map, a chunk too large for one protocol
//!   v2 record, and a server with no identity to present (no
//!   `--identity-dir`, no `--state-file`, no `--ephemeral-identity`) or one
//!   whose key or adoption file cannot be read.
//! - `3`  the host contract was refused: the granted rtprio ceiling is zero,
//!   the real-time policy was denied, or locking memory was denied. The
//!   message names the limit that was read and what was wanted.
//! - `4`  the socket could not be bound, or the client connection failed.
//! - `5`  the PCM source failed mid-stream. No end-of-stream signal is sent in
//!   this case, on purpose.
//! - `6`  a thread runs under a real-time policy that was not reported.
//! - `7`  this process could not take a complete inventory of its own threads,
//!   so it cannot say whether `6` holds. Refusing here rather than
//!   starting is the point: the contract is graded on that report, and a
//!   report built from a list that lost a thread reads clean for the
//!   wrong reason. A thread that was created and never reported itself is
//!   the same failure from the other end and exits the same way.
//! - `8`  the control channel could not be bound, or was denied, or the
//!   persisted zone state could not be read. The message names the
//!   address or the file and the reason. This happens BEFORE any audio
//!   thread exists and before the audio socket is bound, so a server that
//!   cannot be controlled never serves audio while reporting itself as
//!   controllable.
//! - `9`  this server was told to advertise itself by multicast DNS and could
//!   not. The message names what failed. Refusing is the point: a server
//!   that quietly did not advertise looks exactly like one whose
//!   endpoints have not asked yet.
//! - `10` this server was told to be UPnP AV media renderers (`--upnp`) and
//!   could not open their sockets: the HTTP port or the SSDP port is held by
//!   another program, or the multicast group could not be joined. The
//!   message names which. Before any thread exists, like `8` and `9`.
//! - `11` this server was told to run Soloist receivers
//!   (`--soloist-receivers`) and cannot read the receiver directory
//!   (`--soloist-dir`): it is not there, or is not a directory this user may
//!   list. The message names it. Before any thread exists, like `8` to `10`.
//!   (A receiver whose supervisor has not started yet is NOT this: its FIFO
//!   and socket are looked for again for as long as the server runs.)
//!
//! `--health-check <addr:port>` is a separate mode for container healthchecks
//! (`chorus_server::health`): it starts nothing, probes a running server's
//! control plane with `GET /api/state`, and exits `0` on a 200 answer and `1`
//! otherwise, which is Docker's healthcheck contract.
//!
//! `stage-firmware --image <file.bin> --board <profile> --firmware-dir <dir>
//! [--name <name>]` is another (goal 14, `chorus_server::firmware::stage`):
//! it starts nothing, copies one image into a firmware directory with the
//! manifest the server verifies it against, prints what it staged and exits
//! `0`, or names what is wrong and exits `2`. It writes local files only.
//!
//! # The shape of the process, and why the report can be taken once
//!
//! Every thread this process will ever run is created here, before the
//! scheduling report is taken, and none is created after it:
//!
//! - the **supervisor**, this thread, which opens a source for each stream and
//!   waits;
//! - the **audio** thread, which is the only one that touches PCM, the only
//!   one that asks for a real-time policy, and the only one that spawns
//!   nothing;
//! - the **acceptor**, which does nothing but accept connections and hand them
//!   to a slot;
//! - two **client** threads per slot, `--max-clients` slots of them, created
//!   whether or not anybody has connected;
//! - with `--control-listen`, the **control acceptor**, one **control
//!   worker** per `--control-workers` slot, created whether or not any
//!   subscriber has connected, the **event writer** that holds every event
//!   stream (`chorus_server::events`) and the **conductor** that carries
//!   every change to the audio sessions (`chorus_server::conductor`), and
//!   with `--advertise` one **advertiser**;
//! - with `--slots` (the line-ins) and the control plane, the **tv-relay**
//!   (goal 13, `chorus_server::tvrelay`), which receives, restamps and sends
//!   on the TV path's datagrams;
//! - with `--mqtt-broker` (goal 15, and it needs the control plane), the
//!   **mqtt-publisher** (`chorus_server::mqtt`), which connects to the
//!   broker, publishes and reconnects, all on itself. Without the flag it
//!   does not exist;
//! - with `--players P` (goal 16, and it needs `--slots`), one **player**
//!   thread per player, `player-0` to `player-<P-1>`
//!   (`chorus_server::player`), each the one writer of its player port. They
//!   exist from the start whether or not anything plays, because a thread
//!   made when a stream starts would be a thread the report never saw.
//!   Without the flag there is none. (goal 17) An alarm whose source is a
//!   stored stream URL plays through one of them and adds no thread: the
//!   players' reports are taken by the conductor, on the thread it already
//!   is, when there is no `--upnp`, and by `upnp-manager` when there is;
//! - with `--upnp` (goal 16, and it needs the control plane, `--slots` and
//!   `--players`), the UPnP AV media renderers' `4 + W` threads
//!   (`chorus_server::upnp`): `upnp-ssdp`, `upnp-acceptor`, `upnp-events`,
//!   `upnp-manager` and one `upnp-worker-<i>` per `--upnp-workers` W.
//!   Without the flag there is none;
//! - with `--soloist-receivers R` (goal 17, and it needs the control plane
//!   and `--slots`), `R + 1` threads: one **soloist-reader-<i>** per
//!   receiver (`chorus_server::soloistreader`), the one reader of that
//!   receiver's FIFO and the one writer of its port, and one
//!   **soloist-manager** (`chorus_server::soloist`), which holds every
//!   receiver's supervisor socket. They exist from the start whether or not
//!   any receiver container is running. Without the flag there is none.
//!
//! So the population is `3 + 2N` without the control plane and
//! `6 + 2N + M` with it, plus one for the advertiser, one for the TV
//! relay with `--slots`, one for the MQTT publisher with
//! `--mqtt-broker`, P for `--players P`, `4 + W` for `--upnp` and `R + 1` for
//! `--soloist-receivers R`, and NOT ONE of those
//! numbers is a function of how many endpoints or browsers are switched on,
//! of how many streams are playing, nor of `--slots`: every stream slot is
//! cut by the one audio thread (`chorus_server::slots`).
//! `crates/server/tests/control_thread_population.rs` grades that against
//! `/proc` while subscribers come and go.
//!
//! That shape is deliberate and it is a safety property rather than a style.
//! `std::thread::spawn` inherits the creating thread's scheduling policy
//! (`PTHREAD_INHERIT_SCHED`), so a connection handler spawned from a thread
//! holding `SCHED_FIFO` is real-time too, at the same priority, and
//! `deploy/run-server.sh` runs this binary with `--ulimit rtprio=20`. Taking
//! the real-time policy on the thread that does the audio work, and on no
//! thread that creates another, is what keeps the socket handlers ordinary.
//! Creating them all before the report is what lets one report describe the
//! whole run: `crates/hostctl` grades the report against `/proc/self/task`,
//! and a thread created after that comparison is a thread nobody checked.

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chorus_audio::{MonotonicTimeline, StreamFormat};
use chorus_control::transport::{Transport, ZoneTransports};
use chorus_control::zones::Zone;
use chorus_discovery::dnssd::{Advertisement, AUDIO_SERVICE, CONTROL_SERVICE};
use chorus_discovery::net::{advertisable_addresses, Advertiser};
use chorus_hostctl::ThreadRegistry;
use chorus_server::clients::ClientPool;
use chorus_server::conductor::{self, CivilClock, Clocks, Conductor, Schedule};
use chorus_server::config::{ServerConfig, ServerConfigError};
use chorus_server::control::{initial_state, ControlPlane, ControlState};
use chorus_server::firmware::Firmware;
use chorus_server::hostreport::{
    decide_memory_lock, register_ordinary_thread, scheduling_report, take_contract_for_this_thread,
    ContractRefused, RealTimeOutcome, SchedulingVerdict,
};
use chorus_server::linein::LineIns;
use chorus_server::playerport::PlayerPort;
use chorus_server::router::Router;
use chorus_server::schedule_runtime::Runtime;
use chorus_server::serve::{serve_stream, ServeError, ServeParams, ServeReport};
use chorus_server::session::{
    identity_source, load_identity, IdentitySource, Offer, OfferRefused, SessionContext,
};
use chorus_server::slots::{serve_slots, SlotCommand, SlotEvent, SlotMedia};
use chorus_server::source::{self, PcmSource};
use chorus_server::stream::FanoutSink;
use chorus_server::tvrelay::{RelaySetup, TvRelay};

const EXIT_CONFIG: u8 = 2;
const EXIT_CONTRACT: u8 = 3;
const EXIT_TRANSPORT: u8 = 4;
const EXIT_SOURCE: u8 = 5;
const EXIT_UNDECLARED_THREAD: u8 = 6;
const EXIT_INCOMPLETE_INVENTORY: u8 = 7;
const EXIT_CONTROL: u8 = 8;
const EXIT_ADVERTISE: u8 = 9;
const EXIT_UPNP: u8 = 10;
const EXIT_SOLOIST: u8 = 11;

/// Every status line carries the contract phrases, so a run that is missing
/// part of the contract says so every time it says anything.
#[derive(Clone)]
struct Status {
    real_time: String,
    memory: String,
}

impl Status {
    fn say(&self, line: &str) {
        println!("chorus-server: {} {} {}", line, self.real_time, self.memory);
    }
}

/// One tier line per zone this server SERVES, and one line per declaration that
/// reaches no zone it serves.
///
/// WIFI-7's AC-6 asks for the transport in force and the bound it is held to
/// "for every zone it serves, so that no zone's tier is implicit", so the list
/// walked is the served zone state and never the `--zone` command line. The two
/// differ in both directions, and both are the same defect:
///
/// - a server restarted against its persisted state is given its zones by the
///   file and serves zones the command line never named;
/// - a `--zone` naming a zone the state does not hold declares a tier for a
///   zone nobody serves.
///
/// The second is reported rather than dropped, because a declaration that
/// silently reaches nothing reads exactly like one that took effect. It is
/// deliberately NOT spelled `zone id=`: that shape is the tier of a zone being
/// served, and this is the absence of one.
fn report_zone_tiers(
    transports: &ZoneTransports,
    served: &[Zone],
    declared: &[(String, Transport)],
) {
    for zone in served {
        println!("chorus-server: {}", transports.report(&zone.id));
    }
    for (id, transport) in declared {
        if !served.iter().any(|zone| zone.id == *id) {
            println!(
                "chorus-server: zone-declaration id={} transport={} \
                 applies_to=no-zone-this-server-serves",
                id, transport
            );
        }
    }
}

fn report(what: &str, detail: &str) {
    let mut err = std::io::stderr();
    let _ = writeln!(err, "chorus-server: {}: {}", what, detail);
}

/// The port out of a `host:port`, for the SRV record that advertises it.
///
/// A listen address with no port in it cannot be advertised, and zero is what
/// says so: an SRV record naming port 0 is one nothing can connect to, which is
/// the honest answer where the port is unknown.
/// The TV relay's UDP address: the audio listener's address, on
/// `--low-latency-port` or, by default, the audio port plus one (ASSUMED; an
/// ephemeral audio port gives an ephemeral UDP port).
fn udp_address(listen: &str, port: Option<u16>) -> Option<std::net::SocketAddr> {
    use std::net::ToSocketAddrs;
    let mut audio = listen.to_socket_addrs().ok()?.next()?;
    let port = match (port, audio.port()) {
        (Some(p), _) => p,
        (None, 0) => 0,
        (None, p) => p.checked_add(1)?,
    };
    audio.set_port(port);
    Some(audio)
}

fn port_of(address: &str) -> u16 {
    address
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse().ok())
        .unwrap_or(0)
}

/// What `--help` prints. The exit codes above are the contract; this is only
/// the list of options, so an operator (or the image test) can ask the binary
/// what it takes without starting it.
const USAGE: &str = "\
usage: chorus-server [options]

audio:
  --listen <addr:port>        where endpoints connect for audio (default 127.0.0.1:4010)
  --source <tone|chirp|path|fifo:path>  PCM in: a test tone, the rig's chirp, a file (ends at its
                              last byte) or a named pipe (held open; a closed writer is silence)
                              (default tone)
  --measure-config <path>     where the chirp is declared (default config/measure.conf)
  --chirp-amplitude <x>       the chirp's amplitude, at most the declared ceiling (default the ceiling)
  --format <name>             pcm_s16le, pcm_s24le or pcm_f32le (default pcm_s16le)
  --rate <hz> --channels <n>  stream shape (default 48000, 2)
  --chunk-us <us>             chunk duration (default 20000)
  --tone-ms <ms> --rate-skew-ppm <ppm>  tone length and a deliberate rate skew, for tests
  --serve-forever             keep serving after a client ends (default: serve once)
  --max-clients <n>           audio clients at once (default 4)
  --slots <n>                 stream slots: serve every group's stream from this process, each
                              session routed to its group's (needs --control-listen; default 0,
                              one stream for everyone; at most 32)

identity (protocol v2; every audio connection is an encrypted session):
  --identity-dir <dir>        server.key and adopted-endpoints live here
                              (default: the directory of --state-file; with neither, refused)
  --server-id <id>            the id endpoints pin this server's key to (default chorus-server)
  --ephemeral-identity        a key for this process only and adoptions in memory (tests)

firmware (goal 14; docs/firmware-updates.md):
  --firmware-dir <dir>        staged images (<name>.bin + <name>.manifest), verified at start and
                              on firmware_rescan; installed only by firmware_install (needs
                              --control-listen). A transfer to a speaker that is not on this
                              host is refused unless the owner's bench variable is set
  stage-firmware --image <file.bin> --board <profile> --firmware-dir <dir> [--name <name>]
                              stage one image with its manifest, then exit (starts nothing)

host contract:
  --rt-priority <n> --rttime-us <us> --memlock-wanted-bytes <bytes>
  --allow-non-realtime --no-lock-memory --allow-unlocked-memory

control plane:
  --control-listen <addr:port>  serve the control API and page (off by default)
  --control-workers <n>         control worker threads (default 8)
  --state-file <path>           persist zone state here
  --zone <id[=transport]>       declare a zone (repeatable)
  --group-audio <group=addr>    where a group's stream is served (repeatable; not with --slots)
  --event-streams <n>           GET /api/events streams held at once (default 64)
  --civil-time <day-HH:MM>      hold the civil clock at this weekday and time (tests)
  --civil-time-from <instant>   run the civil clock from this UTC instant,
                                YYYY-MM-DDTHH:MM:SSZ (tests)
  --schedule-time-scale <n>     run the schedule's durations n times faster, 1-60 (tests;
                                never the audio)
  --tz <path>                   the TZif file civil time is kept in (default: $TZ, then
                                /etc/localtime, else UTC)
  --players <n>                 network media players, 0-16 (default 0: none): n player
                                threads and n player ports, made at start; a group plays one
                                as the source player:p<i> (needs --slots of at least 1)
  --media-allow-loopback        let the players fetch media from this machine's loopback
                                (tests and development only, never a deployment; needs
                                --players; the server's own ports stay refused)
  --announce-origin <origin>    where the announce command's URLs may come from, as
                                scheme://host[:port] (the home automation's own address);
                                repeatable; with none every announce is refused (needs
                                --control-listen, and --players to play one)
  --advertise --instance <label>  advertise by multicast DNS

soloist (goal 17; docs/soloist.md; off unless --soloist-receivers is above 0: Spotify Connect
through the owner's own Spotify Soloist binary, which runs in receiver containers and is never
part of this program):
  --soloist-receivers <n>       receivers, 0-32 (default 0: none): n reader threads, n ports
                                and one manager thread, made at start; a group plays one as
                                the source soloist:r<i> (needs --control-listen, --slots and
                                --soloist-dir)
  --soloist-dir <dir>           the receiver directory shared with the receiver containers:
                                r<i>.pcm (a FIFO) and r<i>.sock (the supervisor's socket)
  --soloist-grace <seconds>     how long a dissolved, idle live group keeps its receiver
                                (default 60)
  --soloist-volume <chorus|receiver>
                                which gain stage a Spotify volume drives (default chorus: it
                                sets the room's or group's volume; receiver: Soloist's own
                                volume is the gain and chorus only clamps it to the limits)
  --soloist-alarms              let an alarm play a stored Spotify URI on its room's receiver
                                (off by default: read docs/soloist.md first)

upnp (goal 16; docs/upnp.md; off unless --upnp is given: every room, saved group and live
group is a UPnP AV media renderer a control point can play to):
  --upnp                        turn the renderers on (needs --control-listen, --slots and
                                --players of at least 1 each, and a persisted identity:
                                --identity-dir or --state-file, not --ephemeral-identity)
  --upnp-listen <addr:port>     the renderers' one HTTP port: descriptions, control and
                                eventing (default 0.0.0.0:4030; open it in the host firewall)
  --upnp-workers <n>            HTTP worker threads, 1-16 (default 4)
  --upnp-callback-subnet <cidr> where event callbacks may point (repeatable; default: the
                                subnets this host is attached to). A callback on loopback is
                                allowed only when --upnp-listen is itself a loopback address
  --upnp-ssdp-port <port>       the UDP port of the discovery socket. 1900 is the standard;
                                anything else is for tests, or for a host where another
                                program holds 1900 (docs/upnp.md says what is lost then)
  --upnp-ssdp-group <addr:port> where discovery notifications are sent. The standard is
                                239.255.255.250:1900; anything else is for tests
  --upnp-openhome <on|off>      (goal 17) also offer the OpenHome services Product, Volume,
                                Info, Time and Playlist on every renderer, so a control point
                                can hand over a queue and sleep (default on with --upnp)

mqtt (goal 15; docs/mqtt.md; off unless --mqtt-broker is given; read-only: it publishes
state and events, never subscribes, has no command topic and no Home Assistant discovery):
  --mqtt-broker <host:port>     publish to this MQTT 3.1.1 broker over plain TCP (needs
                                --control-listen); a broker that is down never stops the server
  --mqtt-user <name>            the user name to connect with
  --mqtt-password-file <path>   read the password from this file at start (needs --mqtt-user);
                                there is no flag that takes the password itself
  --mqtt-prefix <prefix>        the topic prefix (default chorus/v1)
  --mqtt-client-id <id>         the client id, 1-23 of 0-9 a-z A-Z, one per server on a broker
                                (default chorus)
  --mqtt-keepalive-s <n>        MQTT keep alive in seconds, 1-65535 (default 60)

health:
  --health-check <addr:port>  probe a running server's control plane (GET /api/state);
                              exit 0 on 200, 1 otherwise; starts nothing

decoders (goal 16; docs/decoders.md):
  --probe-media <path>        decode one local file (MP3, FLAC, Ogg Vorbis, Ogg Opus, ALAC in
                              MP4, WAV) and print one line: codec, rate, channels, bits, frames,
                              tags and a hash of the decoded samples; exit 0. A file this build
                              does not decode exits 1 naming why (AAC: `unsupported: aac`).
                              Starts nothing: no thread, no socket

  -h, --help                  print this and exit 0
";

/// Where the schedule's civil time comes from: `--tz <path>`, else `$TZ` (a
/// zoneinfo name checked with the schedule library's safe-name rule and read
/// under `/usr/share/zoneinfo`, an absolute path, or a POSIX TZ string), else
/// `/etc/localtime`, else UTC. A file named and unreadable, or not a zone, is
/// a refusal (exit 2): an alarm kept in a zone nobody chose rings at the
/// wrong hour.
fn load_zone(flag: Option<&str>) -> Result<(chorus_schedule::Zone, String), String> {
    use chorus_schedule::zone::is_safe_zoneinfo_name;
    use chorus_schedule::Zone;
    let read = |path: &str, source: &str| -> Result<(Zone, String), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{} ({}): {}", path, source, e))?;
        let zone = Zone::from_tzif(&bytes)
            .map_err(|e| format!("{} ({}) is not a TZif zone: {}", path, source, e))?;
        Ok((zone, format!("tz={} source={}", path, source)))
    };
    if let Some(path) = flag {
        return read(path, "--tz");
    }
    if let Ok(tz) = std::env::var("TZ") {
        let tz = tz.strip_prefix(':').unwrap_or(&tz).to_string();
        if !tz.is_empty() {
            if tz.starts_with('/') {
                return read(&tz, "TZ");
            }
            if is_safe_zoneinfo_name(&tz) {
                let path = format!("/usr/share/zoneinfo/{}", tz);
                if std::path::Path::new(&path).exists() {
                    return read(&path, "TZ");
                }
            }
            return match Zone::from_posix(&tz) {
                Ok(zone) => Ok((zone, format!("tz={} source=TZ-posix", tz))),
                Err(_) => Err(format!(
                    "$TZ is '{}', which is neither a safe zoneinfo name under \
                     /usr/share/zoneinfo nor a POSIX TZ string",
                    tz
                )),
            };
        }
    }
    if std::path::Path::new("/etc/localtime").exists() {
        return read("/etc/localtime", "localtime");
    }
    Ok((Zone::utc(), "tz=UTC source=default".to_string()))
}

/// The UTC instant a `--civil-time <day>-<HH:MM>` names in `zone`: that
/// weekday and time in the week of Monday 2024-01-01 (any week would do; this
/// one has no daylight-saving change in the northern hemisphere's zones).
fn fixed_civil_instant(zone: &chorus_schedule::Zone, at: chorus_control::rooms::CivilTime) -> i64 {
    let monday = chorus_schedule::civil::days_from_civil(2024, 1, 1);
    zone.instant_of(
        monday + i64::from(at.weekday),
        i64::from(at.time.minutes()) * 60,
    )
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if args.first().map(String::as_str) == Some("stage-firmware") {
        return match chorus_server::firmware::stage_command(&args[1..]) {
            Ok(line) => {
                println!("chorus-server: {}", line);
                ExitCode::SUCCESS
            }
            Err(e) => {
                report("staging refused", &e);
                ExitCode::from(2)
            }
        };
    }
    if args.first().map(String::as_str) == Some("--health-check") {
        let Some(address) = args.get(1).filter(|_| args.len() == 2) else {
            // 1, not EXIT_CONFIG: Docker reserves a healthcheck's exit 2.
            report(
                "configuration refused",
                "--health-check takes exactly one <addr:port>",
            );
            return ExitCode::from(1);
        };
        return match chorus_server::health::probe(address, chorus_server::health::PROBE_TIMEOUT) {
            Ok(status) => {
                println!("chorus-server: healthy: {address} answered `{status}`");
                ExitCode::SUCCESS
            }
            Err(e) => {
                println!("chorus-server: unhealthy: {e}");
                ExitCode::from(1)
            }
        };
    }
    if args.first().map(String::as_str) == Some("--probe-media") {
        // A diagnostic that runs to its end here, before any thread or socket.
        let Some(path) = args.get(1).filter(|_| args.len() == 2) else {
            report(
                "configuration refused",
                "--probe-media takes exactly one <path>",
            );
            return ExitCode::from(1);
        };
        return match chorus_server::probe_media::probe(path) {
            Ok(line) => {
                println!("chorus-server: {line}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                report("probe-media refused", &e);
                ExitCode::from(1)
            }
        };
    }
    let config = match ServerConfig::from_args(args) {
        Ok(c) => c,
        Err(e) => {
            report("configuration refused", &e.to_string());
            // AC-5 asks for "serves no audio and no control state" as an
            // observable and not as an absence, so the zone-tier refusal says
            // so positively. Nothing has been bound at this point: the audio
            // socket, the control socket and the zone state are all still
            // ahead of this line.
            if matches!(e, ServerConfigError::NotATransport { .. }) {
                println!(
                    "chorus-server: stopped reason=unknown-transport chunks_sent=0 zones_served=0"
                );
            }
            return ExitCode::from(EXIT_CONFIG);
        }
    };

    // (goal 15) The MQTT publisher's settings, with the password read from
    // its file now: a file that cannot be read is a configuration refused
    // before anything is bound, and the password is in no argument and no
    // line this process prints.
    let mqtt_settings = match &config.mqtt.broker {
        None => None,
        Some(broker) => {
            let password = match config.mqtt.password_file.as_deref() {
                None => None,
                Some(path) => match chorus_server::mqtt::read_password(path) {
                    Ok(p) => Some(p),
                    Err(e) => {
                        report(
                            "configuration refused",
                            &format!("--mqtt-password-file {}", e),
                        );
                        return ExitCode::from(EXIT_CONFIG);
                    }
                },
            };
            let topics = match chorus_mqtt::topic::Topics::new(&config.mqtt.prefix) {
                Ok(t) => t,
                Err(e) => {
                    report("configuration refused", &format!("--mqtt-prefix {}", e));
                    return ExitCode::from(EXIT_CONFIG);
                }
            };
            Some(chorus_server::mqtt::Settings {
                broker: broker.clone(),
                client_id: config.mqtt.client_id.clone(),
                user: config.mqtt.user.clone(),
                password,
                topics,
                keep_alive_s: config.mqtt.keepalive_s,
            })
        }
    };

    // The format is validated before anything else happens, and before a
    // single chunk exists, so an unsupported one can never be interpreted
    // under an assumption.
    let format = match StreamFormat::new(
        config.sample_rate_hz,
        config.channels,
        &config.sample_format,
    ) {
        Ok(f) => f,
        Err(e) => {
            report("stream format refused", &e.to_string());
            println!("chorus-server: stopped reason=unsupported-format chunks_sent=0");
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    if let Err(e) = format.frames_in(config.chunk_us) {
        report("chunk duration refused", &e.to_string());
        println!("chorus-server: stopped reason=unsupported-format chunks_sent=0");
        return ExitCode::from(EXIT_CONFIG);
    }
    // Protocol v2 announces every stream with an explicit channel map, and
    // carries every chunk inside one record, so a channel count with no layout
    // and a chunk too large for a record are refused here, by name, like a
    // format.
    let offer = match Offer::new(&format, config.chunk_us) {
        Ok(o) => o,
        Err(e) => {
            report("stream format refused", &e.to_string());
            println!(
                "chorus-server: stopped reason={} chunks_sent=0",
                match e {
                    OfferRefused::NoChannelMap { .. } => "no-channel-map",
                    OfferRefused::ChunkTooLarge { .. } => "chunk-too-large-for-a-record",
                }
            );
            return ExitCode::from(EXIT_CONFIG);
        }
    };

    // `--source chirp` is built HERE, before a socket or a thread exists, from
    // the rig's committed values and through its amplitude ceiling. The chirp
    // leaves every endpoint through a real amplifier, so a level above the
    // ceiling, or a configuration that cannot be read, is refused before
    // anything could be played rather than discovered once it is.
    let chirp = if config.source == "chirp" {
        match source::rig_chirp(
            std::path::Path::new(&config.measure_config),
            config.chirp_amplitude,
        ) {
            Ok(chirp) => Some(chirp),
            Err(e) => {
                report("the chirp source was refused", &e);
                println!("chorus-server: stopped reason=chirp-refused chunks_sent=0");
                return ExitCode::from(EXIT_CONFIG);
            }
        }
    } else {
        None
    };

    // WIFI-7's AC-6: "SHALL report, for every zone it serves, the transport in
    // force and the bound that transport is held to, so that no zone's tier is
    // implicit".
    //
    // The zones this server SERVES are the ones its zone state holds, and that
    // set is not `config.zones`: `initial_state` ignores the command line
    // entirely whenever a state file loads, which is the documented restart
    // (`docs/decisions/0018-the-persisted-zone-state.md` -- `--zone` is read
    // only when there is no persisted state). So the report is taken below,
    // from the state itself, once it exists. Reporting from `config.zones` here
    // would say nothing about a restarted house and would print a tier for a
    // zone nobody serves, which is the same implicit tier from both ends.
    let transports = ZoneTransports::new(&config.zone_transports);

    // The control channel is bound HERE: before a thread exists, before the
    // audio socket is bound, and before anything could be served. AC-9 asks
    // that a server which cannot bind its control address exits non-zero
    // naming the address and the reason and does not serve audio while
    // reporting itself as controllable, and doing it first is what makes that
    // true by construction rather than by care.
    let mut control = None;
    let mut schedule_zone = None;
    if let Some(address) = config.control_listen.clone() {
        // Civil time for the schedule, loaded before any thread exists.
        match load_zone(config.tz.as_deref()) {
            Ok((zone, said)) => {
                println!(
                    "chorus-server: civil {} clock={} schedule_time_scale={}",
                    said,
                    if config.civil_time.is_some() {
                        "fixed"
                    } else if config.civil_time_from.is_some() {
                        "from"
                    } else {
                        "system"
                    },
                    config.schedule_time_scale
                );
                schedule_zone = Some(zone);
            }
            Err(e) => {
                report("the time zone was refused", &e);
                println!("chorus-server: stopped reason=tz-refused chunks_sent=0 played=0");
                return ExitCode::from(EXIT_CONFIG);
            }
        }
        let default_audio = config.listen.clone();
        let (mut zones, state_path, from_file) = match initial_state(
            config.state_file.as_deref(),
            &config.zones,
            &config.group_audio,
            &default_audio,
        ) {
            Ok(v) => v,
            Err(e) => {
                report("the control plane refused to start", &e.to_string());
                println!("chorus-server: stopped reason=control-refused chunks_sent=0 played=0");
                return ExitCode::from(EXIT_CONTROL);
            }
        };
        // AC-6, over the zones this server actually serves: the state that was
        // just loaded, whether it came off the command line or off the file.
        report_zone_tiers(&transports, zones.zones(), &config.zone_transports);
        // WIFI-7's AC-7: a group holding any wireless zone is held to the
        // wireless buffer policy and the wireless bound, and the report names
        // the zone whose declaration set it. The groups come from the state
        // that was just loaded, so a group a restart reloaded is reported the
        // same way a configured one is.
        for group in ZoneTransports::groups(zones.zones()) {
            println!(
                "chorus-server: {}",
                transports.group_tier(zones.zones(), &group).report()
            );
        }
        // Catalog v2: a room declared wireless cannot hold a bonded set (K91),
        // so the room model is told each room's declared tier.
        zones.set_transports(transports.clone());
        // (goal 18) Where an announcement's URL may come from.
        zones.set_announce_origins(config.announce_origins.clone());
        if !config.announce_origins.is_empty() {
            println!(
                "chorus-server: announce origins={}",
                config
                    .announce_origins
                    .iter()
                    .map(|origin| origin.literal())
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
        let zone_count = zones.zones().len();
        let mut state = ControlState::new(zones, config.state_file.as_ref().map(|_| state_path));
        state.set_event_streams(config.event_streams);
        // A fixed civil time, when one is configured, before any group is
        // planned onto a slot: the quiet hours it activates clamp first.
        state.set_civil_time(config.civil_time);
        if config.slots > 0 {
            for group in state.serve_on_slots(config.slots) {
                println!(
                    "chorus-server: slots group={} source=none reason=every-slot-in-use slots={}",
                    group, config.slots
                );
            }
        }
        let state = Arc::new(state);
        match ControlPlane::bind(&address, Arc::clone(&state)) {
            Ok(plane) => {
                println!(
                    "chorus-server: control listening on={} workers={} zones={} \
                     state={} catalog_version={}",
                    plane.address(),
                    config.control_workers,
                    zone_count,
                    if from_file { "reloaded" } else { "configured" },
                    chorus_control::CATALOG_VERSION
                );
                control = Some((plane, state));
            }
            Err(e) => {
                report("the control plane refused to start", &e.to_string());
                println!("chorus-server: stopped reason=control-refused chunks_sent=0 played=0");
                return ExitCode::from(EXIT_CONTROL);
            }
        }
    } else {
        // No control channel, so this server holds no zone state and serves no
        // zone: there is no tier in force anywhere for a reader to have to
        // guess at. A `--zone` declaration made anyway is reported as what it
        // is rather than being silently dropped.
        report_zone_tiers(&transports, &[], &config.zone_transports);
    }

    // (goal 16) The control port as it was bound (a configured port 0 has a
    // number by now): one of the server's own listeners for the players'
    // fetch policy.
    let control_port = control
        .as_ref()
        .map_or(0, |(plane, _)| port_of(plane.address()));

    // The server's protocol v2 identity: its long-term key and the endpoints
    // it has adopted. Loaded before any client thread exists, and refused by
    // name when there is nowhere to keep it.
    let Some(source) = identity_source(
        config.identity_dir.as_deref(),
        config.state_file.as_deref(),
        config.ephemeral_identity,
    ) else {
        report(
            "configuration refused",
            "this server has no identity to present: pass --identity-dir <dir> (or --state-file, \
             whose directory is then used) so its key and adopted endpoints survive a restart, \
             or --ephemeral-identity for a throwaway run",
        );
        println!("chorus-server: stopped reason=no-identity chunks_sent=0 played=0");
        return ExitCode::from(EXIT_CONFIG);
    };
    if config.server_id.is_empty() || config.server_id.len() > 255 {
        report(
            "configuration refused",
            &format!(
                "--server-id is 1 to 255 bytes, not {}",
                config.server_id.len()
            ),
        );
        println!("chorus-server: stopped reason=identity-refused chunks_sent=0 played=0");
        return ExitCode::from(EXIT_CONFIG);
    }
    let (identity, adoptions) = match load_identity(&source, &config.server_id) {
        Ok(v) => v,
        Err(e) => {
            report("the server identity could not be loaded", &e);
            println!("chorus-server: stopped reason=identity-refused chunks_sent=0 played=0");
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    // (goal 18) Who this server is to a controller that has to recognise it
    // again at another address (Home Assistant's zeroconf discovery): `id`,
    // derived from the fingerprint of the server's public key, the identity
    // the renderers' UDNs derive from too. It survives a restart exactly
    // when the key does: with `--ephemeral-identity` the key is made anew
    // at every start, and so is the id (a throwaway run is a new server).
    let server_id = chorus_server::announce::server_id(&identity.keypair.public);
    if let Some((_, state)) = &control {
        state.set_server(
            &server_id,
            &format!("chorus-server {}", env!("CARGO_PKG_VERSION")),
        );
    }

    // The multicast socket, opened before any thread too, and for the same
    // reason: a server told to advertise and unable to must say so rather than
    // start and be quietly undiscoverable.
    let mut advertiser = None;
    if config.advertise {
        let control_address = control
            .as_ref()
            .map(|(plane, _)| plane.address().to_string())
            .unwrap_or_else(|| config.listen.clone());
        let advertisements = vec![
            Advertisement {
                instance: config.instance.clone(),
                service: AUDIO_SERVICE.to_string(),
                host: format!("{}.local.", config.instance),
                port: port_of(&config.listen),
                addresses: advertisable_addresses(&config.listen),
                txt: vec![
                    ("v".to_string(), chorus_control::CATALOG_VERSION.to_string()),
                    ("ctl".to_string(), port_of(&control_address).to_string()),
                ],
            },
            Advertisement {
                instance: config.instance.clone(),
                service: CONTROL_SERVICE.to_string(),
                host: format!("{}.local.", config.instance),
                port: port_of(&control_address),
                addresses: advertisable_addresses(&control_address),
                txt: chorus_server::announce::control_txt(&server_id),
            },
        ];
        match Advertiser::open(advertisements) {
            Ok(open) => {
                println!(
                    "chorus-server: advertising instances={}",
                    open.instances().join(" ")
                );
                advertiser = Some(open);
            }
            Err(e) => {
                report("this server could not advertise itself", &e.to_string());
                println!("chorus-server: stopped reason=advertise-refused chunks_sent=0 played=0");
                return ExitCode::from(EXIT_ADVERTISE);
            }
        }
    }

    // (goal 16) The UPnP AV media renderers' sockets, bound before any thread
    // too: a server told to be renderers and unable to listen says which
    // port it could not have rather than starting without them.
    let mut upnp_sockets = None;
    if config.upnp.on {
        match chorus_server::upnp::Sockets::bind(&config.upnp) {
            Ok(sockets) => upnp_sockets = Some(sockets),
            Err(e) => {
                report("the UPnP AV media renderers could not start", &e);
                println!("chorus-server: stopped reason=upnp-refused chunks_sent=0 played=0");
                return ExitCode::from(EXIT_UPNP);
            }
        }
    }

    // (goal 17) The receiver directory, read before any thread too: a server
    // told to run receivers through a directory it cannot list says so
    // rather than waiting for ever for sockets that cannot appear.
    if config.soloist.receivers > 0 {
        let dir = config.soloist.dir.as_deref().unwrap_or("");
        if let Err(e) = std::fs::read_dir(dir) {
            report(
                "the Soloist receivers could not start",
                &format!(
                    "the receiver directory '{}' cannot be read ({}); it is the directory this \
                     server shares with the receiver containers (--soloist-dir)",
                    dir, e
                ),
            );
            println!("chorus-server: stopped reason=soloist-refused chunks_sent=0 played=0");
            return ExitCode::from(EXIT_SOLOIST);
        }
    }

    // Every thread registers itself, from inside itself, exactly once. This one
    // does it here: it supervises, it does no audio work, and it asks for no
    // real-time policy, and a report with no row for a running thread is a
    // report that cannot be checked.
    let registry = Arc::new(ThreadRegistry::new());
    register_ordinary_thread("supervisor", &registry);

    let memory = match decide_memory_lock(
        config.lock_memory,
        config.memlock_wanted_bytes,
        config.allow_unlocked_memory,
    ) {
        Ok(m) => m,
        Err(e) => {
            report("host contract refused", &e.to_string());
            println!("chorus-server: stopped reason=memory-lock-denied chunks_sent=0 played=0");
            return ExitCode::from(EXIT_CONTRACT);
        }
    };

    // ONE timeline for the whole process, ONE fanout, and one source and one
    // chunker per stream. Two clients attached at the same time are on the same
    // timeline and get the same presentation timestamp for the same content; a
    // chunker each would give them two streams that merely sound alike.
    //
    // The timeline outlives a stream on purpose. A second stream on a fresh
    // epoch would step the presentation timestamps of anybody still attached
    // backwards, while the `t1`/`t2` stamps their exchanges are answered with
    // came from the epoch they attached on - two clocks in one connection, and
    // the one thing this whole phase exists to avoid. Monotonic time already
    // gives the next stream an origin later than the last one's, so nothing is
    // gained by restarting it.
    let timeline = MonotonicTimeline::new();
    // One stream and its one fanout, or `--slots S` of them and the silent
    // one; either way the router is what attaches each session to one.
    let router = Arc::new(if config.slots > 0 {
        Router::slotted(config.slots)
    } else {
        Router::single(Arc::new(chorus_server::stream::Fanout::new()))
    });
    let fanout = Arc::clone(&router.fanouts()[0]);
    // The visualizer stream's tap for HTTP subscribers (`GET /api/visualizer`,
    // chorus_server::lights): the router owns it, the control plane hands a
    // subscriber to it, and its frames are stamped on this timeline.
    if let Some((_, state)) = &control {
        state.set_lights(Arc::clone(router.lights()), timeline);
    }
    let keep = Arc::new(AtomicBool::new(true));
    // What the slots' own inputs are made of, before the audio thread
    // exists: every chime rendered at this server's format, and one line-in
    // port per slot. (goal 17) An input may play in any number of groups,
    // and every group that plays one holds a slot, so at most S inputs play
    // at once and S ports are enough; a port is one input's, however many
    // slots cut it.
    let mut media = SlotMedia::default();
    let mut line_ins = None;
    if config.slots > 0 {
        if let Some(pcm) = chorus_schedule::PcmFormat::from_name(&config.sample_format) {
            for chime in chorus_schedule::chime::CHIMES {
                if let Ok(bytes) =
                    chorus_schedule::render(chime, format.sample_rate_hz, format.channels, pcm)
                {
                    media.chimes.push(Arc::from(bytes));
                }
            }
        }
        if let Some((_, state)) = &control {
            let state = Arc::clone(state);
            let l = Arc::new(LineIns::new(
                format,
                config.slots,
                Box::new(move || state.wake_conductor()),
            ));
            media.ports = l.ports().to_vec();
            line_ins = Some(l);
        }
        // (goal 16) The player ports, `--players` of them, each a ring at
        // this server's format, allocated here before the audio thread
        // exists. None without the flag, and then nothing below mentions
        // them.
        media.players = (0..config.players)
            .map(|_| Arc::new(PlayerPort::for_format(&format)))
            .collect();
        // (goal 17) The Soloist receivers' ports, `--soloist-receivers` of
        // them, allocated here like the player ports. None without the
        // flag.
        media.soloists = (0..config.soloist.receivers)
            .map(|_| Arc::new(chorus_server::soloistport::SoloistPort::for_format(&format)))
            .collect();
        println!(
            "chorus-server: slot-media chimes={} rendered_bytes={} line_in_ports={}",
            media.chimes.len(),
            media.chimes.iter().map(|c| c.len()).sum::<usize>(),
            media.ports.len()
        );
        if !media.players.is_empty() {
            println!(
                "chorus-server: players count={} ids={} ring_ms={} ring_frames={}",
                media.players.len(),
                chorus_server::player::player_list(media.players.len()),
                chorus_server::playerport::PLAYER_RING_MS,
                media.players[0].capacity()
            );
        }
    }
    // The audio thread takes the media; the player threads, created below
    // with the rest, each take the producer's side of one port.
    let player_ports = media.players.clone();
    if let Some((_, state)) = &control {
        state.set_players(player_ports.len());
    }
    // (goal 17) The Soloist receivers: what the conductor, the control plane
    // and the manager thread share. `None` without `--soloist-receivers`,
    // and then no Soloist code runs.
    let soloist_readers: Vec<Arc<chorus_server::soloistreader::ReaderStats>> = media
        .soloists
        .iter()
        .map(|_| Arc::new(chorus_server::soloistreader::ReaderStats::default()))
        .collect();
    let soloist_link = (!media.soloists.is_empty()).then(|| {
        Arc::new(chorus_server::soloist::Link::new(
            media.soloists.clone(),
            soloist_readers.clone(),
        ))
    });
    if let (Some((_, state)), Some(link)) = (&control, &soloist_link) {
        state.soloist_through(Arc::clone(link));
        println!(
            "chorus-server: soloist receivers={} dir={} grace_s={} volume={} alarms={} \
             ring_ms={} fill_target_ms={}",
            link.receivers(),
            config.soloist.dir.as_deref().unwrap_or(""),
            config.soloist.grace_s,
            config.soloist.volume.name(),
            if config.soloist.alarms { "on" } else { "off" },
            chorus_server::soloistport::RING_MS,
            chorus_server::soloistport::FILL_TARGET_MS
        );
    }
    let params = ServeParams {
        format,
        chunk_us: config.chunk_us,
        rate_skew_ppm: config.rate_skew_ppm,
    };

    // One unit per thread that came up, so the report below is taken over a
    // thread population that is complete rather than one that is still
    // arriving. Every thread drops its sender the moment it has sent, so
    // reading this to exhaustion terminates whatever happens.
    let (ready, came_up) = mpsc::channel::<()>();

    // The audio thread: the only one that reads PCM, cuts chunks and paces
    // them, the only one that asks the host for a real-time policy, and the
    // only one that creates no thread. It takes the contract for ITSELF, which
    // is the difference between a real-time policy on the thread doing the
    // audio work and a real-time policy on a thread that goes on to make five
    // more that inherit it.
    let (sources, stream_jobs) = mpsc::channel::<Box<dyn PcmSource>>();
    let (outcomes, stream_outcomes) = mpsc::channel::<Result<ServeReport, ServeError>>();
    let (contract, contract_taken) = mpsc::channel::<Result<RealTimeOutcome, ContractRefused>>();
    // The slot shape's two channels: what each slot plays, from the
    // conductor (bounded, drained at every chunk boundary), and what the
    // audio thread says about the configured stream, to this thread.
    let (slot_commands, slot_inbox) = mpsc::sync_channel::<SlotCommand>(4 * config.slots.max(1));
    let (slot_events, slot_outcomes) = mpsc::channel::<SlotEvent>();
    let (slots_failed, slots_stopped) = mpsc::channel::<ServeError>();
    {
        let registry = Arc::clone(&registry);
        let fanout = Arc::clone(&fanout);
        let router = Arc::clone(&router);
        let keep = Arc::clone(&keep);
        let rt_priority = config.rt_priority;
        let rttime_us = config.rttime_us;
        let allow_non_realtime = config.allow_non_realtime;
        let slotted = config.slots > 0;
        thread::spawn(move || {
            let taken = take_contract_for_this_thread(
                "audio",
                rt_priority,
                rttime_us,
                allow_non_realtime,
                &registry,
            );
            let refused = taken.is_err();
            if contract.send(taken).is_err() || refused {
                return;
            }
            if slotted {
                // Every slot, on one grid, until the run stops. Only a
                // failed source ends it early.
                if let Err(e) = serve_slots(
                    params,
                    timeline,
                    &router,
                    &media,
                    &slot_inbox,
                    &stream_jobs,
                    &slot_events,
                    &keep,
                ) {
                    let _ = slots_failed.send(e);
                }
                return;
            }
            for mut pcm in stream_jobs {
                let mut read = move |buf: &mut [u8]| pcm.read(buf);
                let mut sink = FanoutSink::new(Arc::clone(&fanout));
                let go = {
                    let keep = Arc::clone(&keep);
                    move || keep.load(Ordering::SeqCst)
                };
                let served = serve_stream(params, timeline, &mut read, &mut sink, &go);
                if outcomes.send(served).is_err() {
                    return;
                }
            }
        });
    }

    let real_time = match contract_taken.recv() {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(e)) => {
            report("host contract refused", &e.to_string());
            println!("chorus-server: stopped reason=real-time-denied chunks_sent=0 played=0");
            return ExitCode::from(EXIT_CONTRACT);
        }
        Err(_) => {
            report(
                "host contract refused",
                "the audio thread stopped before it could say whether it had taken the real-time \
                 policy, so this run cannot say that it holds",
            );
            println!("chorus-server: stopped reason=real-time-denied chunks_sent=0 played=0");
            return ExitCode::from(EXIT_CONTRACT);
        }
    };

    let status = Status {
        real_time: match &real_time {
            RealTimeOutcome::Granted {
                ceiling,
                priority,
                rttime_us,
                ..
            } => format!(
                "scheduling=real-time rtprio_ceiling={} rtprio_obtained={} rttime_us={}",
                ceiling, priority, rttime_us
            ),
            RealTimeOutcome::RunningWithout { ceiling, wanted } => format!(
                "scheduling=no-real-time-policy-by-configuration rtprio_ceiling={} \
                 rtprio_wanted={}",
                ceiling, wanted
            ),
        },
        memory: memory.phrase(),
    };

    status.say(&format!(
        "identity id={} key={} store={}",
        identity.id,
        chorus_protocol::v2::noise::fingerprint(&identity.keypair.public),
        match &source {
            IdentitySource::Ephemeral => "ephemeral".to_string(),
            IdentitySource::Directory(dir) => dir.display().to_string(),
        }
    ));
    // (goal 16) What the renderers' UDNs are derived from: the fingerprint
    // of the persisted public key, which a restart keeps and no other server
    // shares (`--upnp` refuses `--ephemeral-identity`, config.rs).
    let upnp_identity = chorus_protocol::v2::noise::fingerprint(&identity.keypair.public);

    // One unit per client whose session is up goes down `arrivals`, which is
    // how the supervisor waits for somebody to play to without owning the
    // listener. It is sent from the slot's reader once the handshake and the
    // negotiation succeed, so a refused peer (a v1 client, a changed key, a
    // port probe) never starts a stream.
    let (arrived, arrivals) = mpsc::channel::<()>();

    // The TV relay (goal 13, `chorus_server::tvrelay`): with line-ins (the
    // slot shape and a control plane), one UDP socket beside the audio
    // listener and one thread, created here with the rest of the
    // population. Without line-ins there is no TV to relay and neither
    // exists.
    let mut relay_threads = 0usize;
    let tv_relay = match (&line_ins, &control) {
        (Some(_), Some((_, state))) => {
            if let Err(e) = config.check_low_latency() {
                report("configuration refused", &e.to_string());
                println!(
                    "chorus-server: stopped reason=configuration-refused chunks_sent=0 played=0"
                );
                return ExitCode::from(EXIT_CONFIG);
            }
            let address = match udp_address(&config.listen, config.low_latency_port) {
                Some(a) => a,
                None => {
                    report(
                        "the low-latency socket could not be bound",
                        &format!("{} names no address", config.listen),
                    );
                    println!("chorus-server: stopped reason=bind-failed chunks_sent=0 played=0");
                    return ExitCode::from(EXIT_TRANSPORT);
                }
            };
            let socket = match std::net::UdpSocket::bind(address) {
                Ok(s) => s,
                Err(e) => {
                    report(
                        "the low-latency socket could not be bound",
                        &format!("{}: {}", address, e),
                    );
                    println!("chorus-server: stopped reason=bind-failed chunks_sent=0 played=0");
                    return ExitCode::from(EXIT_TRANSPORT);
                }
            };
            let relay = {
                let router = Arc::clone(&router);
                let wake = Arc::clone(state);
                let say = status.clone();
                TvRelay::new(RelaySetup {
                    socket,
                    plan: config.low_latency_plan(),
                    shape: (format.sample_rate_hz, format.channels, format.sample_format),
                    timeline,
                    send: Box::new(move |id, m| router.push_message(id, m)),
                    wake: Box::new(move || wake.wake_conductor()),
                    say: Box::new(move |line| say.say(line)),
                    key: Box::new(|| chorus_server::session::random_32().ok()),
                    loss: config.udp_loss,
                })
            };
            match relay {
                Ok(r) => Some(Arc::new(r)),
                Err(e) => {
                    report("the low-latency relay could not start", &e.to_string());
                    println!("chorus-server: stopped reason=bind-failed chunks_sent=0 played=0");
                    return ExitCode::from(EXIT_TRANSPORT);
                }
            }
        }
        _ => None,
    };
    if let Some(relay) = &tv_relay {
        relay_threads = 1;
        let plan = relay.plan();
        status.say(&format!(
            "low-latency listening port={} chunk_frames={} fec_k={} fec_depth={} l_tv_ns={} \
             floor_ns={} udp_loss_ppm={}",
            relay.port(),
            plan.chunk_frames,
            plan.fec_k,
            plan.fec_depth,
            plan.l_tv_ns,
            plan.floor_ns(),
            config.udp_loss.map_or(0, |(ppm, _)| ppm)
        ));
        let relay = Arc::clone(relay);
        let keep = Arc::clone(&keep);
        let registry = Arc::clone(&registry);
        let ready = ready.clone();
        thread::spawn(move || {
            register_ordinary_thread("tv-relay", &registry);
            if ready.send(()).is_err() {
                return;
            }
            drop(ready);
            relay.run(&keep);
        });
    }

    // (goal 14) The control plane lists every speaker the pins already hold
    // and forgets a pin on `speaker_forget`: one store, shared.
    let adoptions = Arc::new(adoptions);
    if let Some((_, state)) = control.as_ref() {
        for line in state.adopt_through(Arc::clone(&adoptions)) {
            status.say(&line);
        }
    }
    // (goal 14) The firmware images and installs: the directory is read now
    // and listed; nothing is offered to anybody by starting. Only with a
    // control plane, whose firmware_install is the one way to start one.
    let firmware = match control.as_ref() {
        Some((_, state)) => {
            let seed = chorus_server::session::random_32()
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .unwrap_or(1);
            let firmware = Arc::new(Firmware::new(
                config.firmware_dir.as_ref().map(std::path::PathBuf::from),
                Arc::clone(&router),
                seed,
                {
                    let status = status.clone();
                    Box::new(move |line: &str| status.say(line))
                },
            ));
            state.firmware_through(Arc::clone(&firmware));
            Some(firmware)
        }
        None => None,
    };
    let session = Arc::new(SessionContext {
        identity,
        adoptions,
        offer,
        log: {
            let status = status.clone();
            Box::new(move |line: &str| status.say(line))
        },
        on_session: {
            let arrived = arrived.clone();
            Box::new(move || {
                let _ = arrived.send(());
            })
        },
        hellos: Default::default(),
        telemetry: Default::default(),
        // An endpoint's buttons change its zone through the control plane,
        // when this server runs one (ADR 0063, docs/decisions/0067-*).
        control: control.as_ref().map(|(_, state)| Arc::clone(state)),
        router: Arc::clone(&router),
        line_ins: line_ins.clone(),
        tv_relay: tv_relay.clone(),
        firmware,
    });
    drop(arrived);

    // Every client thread the process will run, created now, while this thread
    // holds no real-time policy for any of them to inherit.
    let pool = ClientPool::spawn(
        config.max_clients,
        timeline,
        Arc::clone(&keep),
        Arc::clone(&registry),
        ready.clone(),
        Arc::clone(&session),
    );
    let client_threads = pool.threads();
    let busy = pool.busy();

    // The acceptor. It is handed the listener rather than binding one, so that
    // the socket is still bound after the host contract has been reported and
    // graded, exactly as it was when this loop ran on the main thread.
    let (bound, listening) = mpsc::channel::<TcpListener>();
    {
        let registry = Arc::clone(&registry);
        let keep = Arc::clone(&keep);
        let ready = ready.clone();
        let status = status.clone();
        thread::spawn(move || {
            register_ordinary_thread("acceptor", &registry);
            if ready.send(()).is_err() {
                return;
            }
            drop(ready);
            let listener = match listening.recv() {
                Ok(listener) => listener,
                Err(_) => return,
            };
            while keep.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, peer)) => {
                        attach(stream, peer, &pool, &status);
                    }
                    Err(e) => {
                        report("a client connection failed", &e.to_string());
                        return;
                    }
                }
            }
        });
    }
    // (goal 16) The players, `--players` of them. Each player thread (made
    // further down, with the rest) runs a media player (`mediaplayer`): it
    // fetches, decodes and writes into its port when it is told to, and
    // waits otherwise. The fetch policy is built here, once, from this
    // server's own listeners (brief section 4.8): a media URL may never name
    // this machine's loopback or the ports this server listens on.
    // The renderers' HTTP port, when there is one, is one more entry in the
    // list. `--media-allow-loopback` (tests and development) lets a URL name
    // loopback; the server's own ports stay refused either way.
    let media_policy = chorus_server::mediaplayer::fetch_policy(
        &[
            port_of(&config.listen),
            control_port,
            upnp_sockets.as_ref().map_or(0, |s| s.http_port()),
        ],
        config.media_allow_loopback,
    );
    if config.media_allow_loopback {
        status.say(
            "note media-allow-loopback: the players may fetch from this machine's loopback \
             (tests and development only)",
        );
    }
    let (players, player_drivers) =
        chorus_server::mediaplayer::Players::new(player_ports.len(), media_policy);
    let players = Arc::new(players);
    // (goal 17) The player sessions: ONE table for every caller that plays a
    // URL in a room (the UPnP renderers, and an alarm whose source is a
    // stored stream URL), made here, before the conductor, so the alarm path
    // works with `--upnp` off. The players have one report stream: with
    // `--upnp` the renderers' manager takes it (below); without, the
    // conductor does, on the thread it already is. No thread is added.
    let player_sessions = match (control.as_ref(), players.is_empty()) {
        (Some((_, state)), false) => {
            let status = status.clone();
            let prefix = if upnp_sockets.is_some() {
                "upnp"
            } else {
                "media"
            };
            Some(Arc::new(
                chorus_server::playersessions::PlayerSessions::new(
                    Arc::clone(state),
                    Arc::clone(&players),
                    Box::new(move |line: &str| status.say(&format!("{} {}", prefix, line))),
                ),
            ))
        }
        _ => None,
    };

    // (goal 18) The announcer: what carries out the `announce` command, on
    // the worker that takes the command (the start) and on the conductor
    // (the end). No thread. A server without `--players` has one too, so
    // the command is refused by name (`no-players`) after the same checks.
    let announcer = control.as_ref().map(|(_, state)| {
        let status = status.clone();
        let announcer = Arc::new(chorus_server::announce::Announcer::new(
            player_sessions.clone(),
            &config.announce_origins,
            Box::new(move |line: &str| status.say(line)),
        ));
        state.announce_through(Arc::clone(&announcer));
        announcer
    });

    // The control plane's whole thread population, created here, on this
    // thread, which holds no real-time policy for any of them to inherit, and
    // before the scheduling report below. Nothing a subscriber does creates a
    // thread after this point: `accept_loop` hands connections to workers that
    // already exist, and turns one away by name when they are all busy.
    let mut control_threads = 0usize;
    let control_state = control.as_ref().map(|(_, state)| Arc::clone(state));
    if let Some((mut plane, state)) = control.take() {
        plane.spawn_workers(config.control_workers, Arc::clone(&registry), ready.clone());
        // The acceptor, the workers, and the two threads that serve every
        // subscriber and every session: the event writer and the conductor.
        control_threads = plane.threads() + 3;
        {
            let state = Arc::clone(&state);
            let keep = Arc::clone(&keep);
            let registry = Arc::clone(&registry);
            let ready = ready.clone();
            thread::spawn(move || {
                register_ordinary_thread("event-writer", &registry);
                if ready.send(()).is_err() {
                    return;
                }
                drop(ready);
                chorus_server::events::run_writer(state, keep);
            });
        }
        {
            let mut conductor = Conductor::new(
                Arc::clone(&state),
                Arc::clone(&router),
                (config.slots > 0).then(|| slot_commands.clone()),
            )
            .with_transports(transports.clone())
            .with_players(player_ports.len());
            if let Some(sessions) = &player_sessions {
                // The reports are this thread's only when no renderer will
                // take them.
                let reports = if upnp_sockets.is_some() {
                    None
                } else {
                    players.take_reports()
                };
                conductor = conductor.with_stored_streams(
                    chorus_server::conductor::StoredStreams::new(Arc::clone(sessions), reports),
                );
            }
            if let Some(announcer) = &announcer {
                conductor = conductor.with_announcer(Arc::clone(announcer));
            }
            if let Some(relay) = &tv_relay {
                conductor = conductor.with_tv_relay(Arc::clone(relay));
            }
            if let Some(link) = &soloist_link {
                conductor = conductor.with_soloist(Arc::clone(link));
            }
            if let Some(zone) = schedule_zone.take() {
                let civil = match (config.civil_time, config.civil_time_from) {
                    (Some(at), _) => CivilClock::Fixed(fixed_civil_instant(&zone, at)),
                    (None, Some(from)) => CivilClock::From(i128::from(from) * 1_000_000_000),
                    (None, None) => CivilClock::Live,
                };
                // (goal 17) The Spotify alarm source ships switched off
                // (P7): only `--soloist-alarms` lets an alarm ask for it.
                let mut runtime = Runtime::new(zone);
                runtime.set_soloist_alarms(config.soloist.alarms);
                conductor = conductor.with_schedule(Schedule::new(
                    runtime,
                    Clocks::new(civil, config.schedule_time_scale),
                    line_ins.clone(),
                    transports.clone(),
                ));
            }
            let keep = Arc::clone(&keep);
            let registry = Arc::clone(&registry);
            let ready = ready.clone();
            thread::spawn(move || {
                register_ordinary_thread("conductor", &registry);
                if ready.send(()).is_err() {
                    return;
                }
                drop(ready);
                conductor::run(conductor, keep);
            });
        }
        let keep_for_acceptor = Arc::clone(&keep);
        let registry = Arc::clone(&registry);
        let ready = ready.clone();
        thread::spawn(move || {
            register_ordinary_thread("control-acceptor", &registry);
            if ready.send(()).is_err() {
                return;
            }
            drop(ready);
            plane.accept_loop(keep_for_acceptor);
        });
    }

    // (goal 15) The MQTT publisher: one more ordinary thread, only with
    // `--mqtt-broker`, created here with the rest and counted below. It
    // connects from inside its own loop, after it has reported itself, so a
    // broker that is down delays nothing. `_mqtt_farewell` is dropped when
    // this function returns, however it returns: that stops the publisher
    // and gives it a bounded moment to say `offline` and disconnect.
    let mut mqtt_threads = 0usize;
    let mut _mqtt_farewell = None;
    if let (Some(settings), Some(state)) = (mqtt_settings, control_state.as_ref()) {
        println!("chorus-server: {}", settings.describe());
        mqtt_threads = 1;
        let (tap, events) = chorus_server::mqtt::EventTap::pair();
        state.publish_events_through(tap);
        let (farewell, said) = chorus_server::mqtt::Farewell::pair(Arc::clone(&keep));
        _mqtt_farewell = Some(farewell);
        let state = Arc::clone(state);
        let keep = Arc::clone(&keep);
        let registry = Arc::clone(&registry);
        let ready = ready.clone();
        thread::spawn(move || {
            register_ordinary_thread("mqtt-publisher", &registry);
            if ready.send(()).is_err() {
                return;
            }
            drop(ready);
            chorus_server::mqtt::run(settings, state, events, keep, said);
        });
    }

    // (goal 16) The player threads: one ordinary thread per `--players`
    // port, created here with the rest and counted below, whether or not
    // anything ever plays. Their players and the fetch policy were made
    // above, before the conductor (goal 17).
    let player_threads =
        chorus_server::player::spawn(&player_ports, player_drivers, &keep, &registry, &ready);

    // (goal 17) The Soloist receivers' threads: one reader per receiver and
    // the manager, only with `--soloist-receivers`, created here with the
    // rest and counted below, whether or not any receiver container runs.
    let mut soloist_threads = 0usize;
    if let (Some(link), Some(state), Some(dir)) = (
        soloist_link.as_ref(),
        control_state.as_ref(),
        config.soloist.dir.as_deref(),
    ) {
        let dir = std::path::Path::new(dir);
        soloist_threads += chorus_server::soloistreader::spawn(
            dir,
            link.ports(),
            &soloist_readers,
            &keep,
            &registry,
            &ready,
        );
        soloist_threads += chorus_server::soloist::spawn(
            chorus_server::soloist::Settings {
                dir: dir.to_path_buf(),
                receivers: link.receivers(),
                grace: Duration::from_secs(config.soloist.grace_s),
                volume: config.soloist.volume,
                alarm_wait: chorus_server::soloist::ALARM_WAIT,
            },
            Arc::clone(state),
            Arc::clone(link),
            &keep,
            &registry,
            &ready,
        );
    }

    // (goal 16) The UPnP AV media renderers: `4 + W` more ordinary threads,
    // only with `--upnp`, created here with the rest and counted below.
    // `_upnp_farewell` is dropped when this function returns, however it
    // returns: that stops them and gives the discovery thread a bounded
    // moment to say byebye for every renderer.
    let mut upnp_threads = 0usize;
    let mut _upnp_farewell = None;
    if let (Some(sockets), Some(state), Some(sessions)) = (
        upnp_sockets.take(),
        control_state.as_ref(),
        player_sessions.as_ref(),
    ) {
        if let Some(reports) = players.take_reports() {
            let upnp = chorus_server::upnp::Upnp::new(
                sockets,
                &config.upnp,
                upnp_identity,
                Arc::clone(state),
                Arc::clone(sessions),
                reports,
                chorus_server::upnp::Hooks {
                    // (goal 17) What kind each offered input is, for the
                    // OpenHome source list: the state names inputs, only the
                    // line-ins know whether one is analogue, optical or HDMI.
                    input_kind: {
                        let line_ins = line_ins.clone();
                        Box::new(move |input: &str| {
                            let id = chorus_control::rooms::InputId::parse(input)?;
                            line_ins.as_ref()?.kind_of(&id)
                        })
                    },
                    log: {
                        let status = status.clone();
                        Box::new(move |line: &str| status.say(line))
                    },
                },
            );
            println!("chorus-server: {}", upnp.describe());
            let (threads, farewell) = upnp.spawn(&keep, &registry, &ready);
            upnp_threads = threads;
            _upnp_farewell = Some(farewell);
        }
    }

    // The advertiser, which answers browses for as long as the run lasts.
    let mut advertiser_threads = 0usize;
    if let Some(advertiser) = advertiser.take() {
        advertiser_threads = 1;
        let keep = Arc::clone(&keep);
        let registry = Arc::clone(&registry);
        let ready = ready.clone();
        thread::spawn(move || {
            register_ordinary_thread("advertiser", &registry);
            if ready.send(()).is_err() {
                return;
            }
            drop(ready);
            let _ = advertiser.announce();
            while keep.load(Ordering::SeqCst) {
                advertiser.answer_pending();
            }
        });
    }

    drop(ready);

    // The report is taken over the whole population or not at all.
    let expected = 1
        + client_threads
        + control_threads
        + advertiser_threads
        + relay_threads
        + mqtt_threads
        + player_threads
        + soloist_threads
        + upnp_threads;
    let mut up = 0usize;
    while came_up.recv().is_ok() {
        up += 1;
    }
    if up != expected {
        report(
            "the thread inventory could not be completed",
            &format!(
                "{} of {} threads reported themselves before the scheduling report was taken, so \
                 this run cannot say whether a thread is real-time without having been reported, \
                 and it will not claim that it can",
                up, expected
            ),
        );
        println!(
            "chorus-server: stopped reason=incomplete-thread-inventory chunks_sent=0 played=0"
        );
        return ExitCode::from(EXIT_INCOMPLETE_INVENTORY);
    }

    let (lines, verdict) = scheduling_report(&registry);
    for line in &lines {
        println!("chorus-server: {}", line);
    }
    match verdict {
        SchedulingVerdict::Agreed => {}
        SchedulingVerdict::Undeclared { count } => {
            report(
                "the scheduling report and the kernel disagree",
                &format!(
                    "{} threads run under a real-time policy that was not reported",
                    count
                ),
            );
            return ExitCode::from(EXIT_UNDECLARED_THREAD);
        }
        SchedulingVerdict::InventoryIncomplete { reason } => {
            report(
                "the thread inventory could not be completed",
                &format!(
                    "{}; so this run cannot say whether a thread is real-time without having \
                     been reported, and it will not claim that it can",
                    reason
                ),
            );
            println!(
                "chorus-server: stopped reason=incomplete-thread-inventory chunks_sent=0 played=0"
            );
            return ExitCode::from(EXIT_INCOMPLETE_INVENTORY);
        }
    }

    status.say(&format!(
        "starting listen={} source={} rate_hz={} channels={} sample_format={} chunk_us={} \
         rate_skew_ppm={} max_clients={}",
        config.listen,
        config.source,
        config.sample_rate_hz,
        config.channels,
        config.sample_format,
        config.chunk_us,
        config.rate_skew_ppm,
        config.max_clients
    ));
    if let Some(state) = &control_state {
        status.say(&state.report());
        if let Some(table) = state.slots_report() {
            status.say(&format!("slots count={} assigned={}", config.slots, table));
        }
    }
    if memory.is_unlocked() {
        status.say("note this run holds no locked memory");
    }
    if matches!(real_time, RealTimeOutcome::RunningWithout { .. }) {
        status.say("note this run has no real-time policy");
    }

    let listener = match TcpListener::bind(&config.listen) {
        Ok(l) => l,
        Err(e) => {
            report(
                "the listen address could not be bound",
                &format!("{}: {}", config.listen, e),
            );
            return ExitCode::from(EXIT_TRANSPORT);
        }
    };
    let address = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| config.listen.clone());
    status.say(&format!("listening on={}", address));
    if bound.send(listener).is_err() {
        report(
            "a client connection failed",
            "the acceptor stopped before the listening socket reached it",
        );
        return ExitCode::from(EXIT_TRANSPORT);
    }

    if config.slots > 0 {
        return serve_slot_shape(
            &config,
            format,
            chirp.as_ref(),
            &sources,
            &slot_outcomes,
            &slots_stopped,
            &keep,
            &status,
        );
    }
    drop(slot_commands);

    // One stream, or one stream after another. `--serve-forever` clears
    // `config.once`, and `deploy/run-server.sh` and `deploy/Dockerfile` both
    // pass it, so this loop is the deployed shape and the single stream is the
    // default one every `tools/` entry point takes.
    loop {
        // A client is waited for before a chunk is cut, so that nothing is
        // produced into an empty room and the stream a listener joins starts
        // where the audio does.
        if arrivals.recv().is_err() {
            report(
                "a client connection failed",
                "the acceptor stopped before a client attached",
            );
            break ExitCode::from(EXIT_TRANSPORT);
        }

        let pcm = match source::open(
            &config.source,
            format,
            config.chunk_us,
            config.tone_ms,
            chirp.as_ref(),
        ) {
            Ok(s) => s,
            Err(e) => {
                report(
                    "the PCM source could not be opened",
                    &format!("{}: {}", config.source, e),
                );
                break ExitCode::from(EXIT_SOURCE);
            }
        };
        status.say(&format!("source {}", pcm.describe()));

        if sources.send(pcm).is_err() {
            keep.store(false, Ordering::SeqCst);
            report("the stream stopped", "the chunk emitter is gone");
            status.say("stopped reason=stream-failed");
            break ExitCode::from(EXIT_TRANSPORT);
        }

        let outcome = match stream_outcomes.recv() {
            Ok(o) => o,
            Err(_) => {
                keep.store(false, Ordering::SeqCst);
                report("the stream stopped", "the chunk emitter panicked");
                status.say("stopped reason=stream-failed");
                break ExitCode::from(EXIT_TRANSPORT);
            }
        };

        match outcome {
            Ok(served) => {
                status.say(&format!(
                    "stream done chunks_sent={} frames_sent={} bytes_discarded={} \
                     ended_cleanly={} final_sequence={} clients={} dropped_for_slow_clients={}",
                    served.chunks_sent,
                    served.frames_sent,
                    served.bytes_discarded,
                    u8::from(served.ended_cleanly),
                    served.final_sequence,
                    fanout.subscribers(),
                    fanout.dropped()
                ));
                if let Some(state) = &control_state {
                    status.say(&state.report());
                }
                if config.once {
                    keep.store(false, Ordering::SeqCst);
                    // The end of the stream is queued for every client; the
                    // process does not exit under the writers delivering it.
                    let delivered = busy.wait_idle(Duration::from_secs(2));
                    status.say(&format!(
                        "stopped reason=stream-ended clients_finished={}",
                        u8::from(delivered)
                    ));
                    break ExitCode::SUCCESS;
                }
                // The clients of the stream that just ended have had their
                // end-of-stream signal and are leaving, so the arrivals they
                // registered are spent. Discarding them is what makes the next
                // pass wait for a NEW listener rather than replay into a room
                // that has emptied.
                while arrivals.try_recv().is_ok() {}
                status.say("stream ended, waiting for the next client");
            }
            Err(e) => {
                keep.store(false, Ordering::SeqCst);
                report("the stream stopped", &e.to_string());
                status.say("stopped reason=stream-failed");
                break match e {
                    ServeError::Source(_) => ExitCode::from(EXIT_SOURCE),
                    _ => ExitCode::from(EXIT_TRANSPORT),
                };
            }
        }
    }
}

/// Attach one connection to the stream: audio out, requests in, replies back.
///
/// Both directions run on threads the pool created before this process bound a
/// socket. Nothing here creates one, which is what makes the scheduling report
/// printed at startup a description of the whole run.
fn attach(
    stream: TcpStream,
    peer: std::net::SocketAddr,
    pool: &ClientPool,
    status: &Status,
) -> bool {
    let _ = stream.set_nodelay(true);
    let reader = match stream.try_clone() {
        Ok(r) => r,
        Err(e) => {
            report(
                "a client connection could not be split",
                &format!("{}: {}", peer, e),
            );
            return false;
        }
    };
    // A read timeout is what lets the request reader notice a stopped run
    // rather than sit in a blocking read forever.
    let _ = reader.set_read_timeout(Some(Duration::from_millis(200)));

    // `clients=` counts the slots serving a connection, this one included.
    if !pool.attach(stream, reader, peer) {
        status.say(&format!(
            "client refused peer={} reason=no-free-client-slot max_clients={} clients={}",
            peer,
            pool.max_clients(),
            pool.busy().count()
        ));
        return false;
    }
    status.say(&format!(
        "client connected peer={} clients={}",
        peer,
        pool.busy().count()
    ));
    true
}

/// The slot shape's supervision: the configured stream opened and handed to
/// the audio thread, and again each time it ends, until the run is stopped or
/// the source fails. A slot never ends, so there is no `stream_end` and no
/// "serve once": the process serves until it is stopped.
#[allow(clippy::too_many_arguments)]
fn serve_slot_shape(
    config: &ServerConfig,
    format: StreamFormat,
    chirp: Option<&chorus_measure::ChirpSpec>,
    sources: &mpsc::Sender<Box<dyn PcmSource>>,
    ended: &mpsc::Receiver<SlotEvent>,
    failed: &mpsc::Receiver<ServeError>,
    keep: &Arc<AtomicBool>,
    status: &Status,
) -> ExitCode {
    loop {
        let pcm = match source::open(
            &config.source,
            format,
            config.chunk_us,
            config.tone_ms,
            chirp,
        ) {
            Ok(s) => s,
            Err(e) => {
                keep.store(false, Ordering::SeqCst);
                report(
                    "the PCM source could not be opened",
                    &format!("{}: {}", config.source, e),
                );
                return ExitCode::from(EXIT_SOURCE);
            }
        };
        status.say(&format!("source {}", pcm.describe()));
        if sources.send(pcm).is_err() {
            keep.store(false, Ordering::SeqCst);
            report("the stream stopped", "the chunk emitter is gone");
            status.say("stopped reason=stream-failed");
            return ExitCode::from(EXIT_TRANSPORT);
        }
        // Wait for it to end, or for the audio thread to stop.
        loop {
            if let Ok(e) = failed.try_recv() {
                keep.store(false, Ordering::SeqCst);
                report("the stream stopped", &e.to_string());
                status.say("stopped reason=stream-failed");
                return match e {
                    ServeError::Source(_) => ExitCode::from(EXIT_SOURCE),
                    _ => ExitCode::from(EXIT_TRANSPORT),
                };
            }
            match ended.recv_timeout(Duration::from_millis(200)) {
                Ok(SlotEvent::StreamEnded { chunks }) => {
                    status.say(&format!(
                        "stream ended chunks={}; its slots play silence until it is reopened",
                        chunks
                    ));
                    // A source that ends at once (an empty file) is not
                    // reopened in a tight loop.
                    thread::sleep(Duration::from_millis(200));
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if let Ok(e) = failed.try_recv() {
                        keep.store(false, Ordering::SeqCst);
                        report("the stream stopped", &e.to_string());
                        status.say("stopped reason=stream-failed");
                        return ExitCode::from(EXIT_SOURCE);
                    }
                    keep.store(false, Ordering::SeqCst);
                    report("the stream stopped", "the chunk emitter is gone");
                    status.say("stopped reason=stream-failed");
                    return ExitCode::from(EXIT_TRANSPORT);
                }
            }
        }
    }
}
