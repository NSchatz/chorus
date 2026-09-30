//! The Linux front panel end to end, on fakes (goal 10, brief section 14
//! item 3; K65, K70, K74, K96; docs/decisions/0067-*).
//!
//! A fake evdev device (a pipe carrying `input_event` records, written in
//! real time as a person would press) and a fake LED (a directory shaped like
//! `/sys/class/leds/<name>`) drive the Linux client's real front panel
//! (`chorus_client_linux::front_panel`, the shared `chorus-controls` model)
//! over the client's real protocol v2 session (`session::open`) to the real
//! `chorus-server` binary over loopback. What is checked is on the server:
//! the zone's volume and group, read back from its control plane, and the
//! server's own log of what each controller command did. Nothing about a
//! button bypasses the server: every change is the control plane's.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chorus_audio::MonotonicTimeline;
use chorus_client_linux::config::ClientConfig;
use chorus_client_linux::control::{ControlLink, ZoneWatch};
use chorus_client_linux::front_panel::{
    FrontPanel, InputEvent, LedWriter, PanelConfig, EV_KEY, EV_SYN,
};
use chorus_client_linux::session::{self, EndpointIdentity};
use chorus_protocol::v2::roles;

// Key codes as the `evdev` crate 0.13.2 (Apache-2.0 OR MIT) lists them,
// https://docs.rs/evdev/0.13.2/src/evdev/scancodes.rs.html, read 2026-09-30.
const KEY_VOLUMEDOWN: u16 = 114;
const KEY_VOLUMEUP: u16 = 115;
const KEY_PLAYPAUSE: u16 = 164;
const KEY_CONNECT: u16 = 218;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "chorus-front-panel-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("a loopback port")
        .local_addr()
        .unwrap()
        .port()
}

struct Server {
    child: Child,
    lines: mpsc::Receiver<String>,
    seen: Vec<String>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if thread::panicking() {
            while let Ok(line) = self.lines.recv_timeout(Duration::from_millis(200)) {
                self.seen.push(line);
            }
            eprintln!("the server said:\n{}", self.seen.join("\n"));
        }
    }
}

impl Server {
    fn start(audio: u16, control: u16, state: &Path) -> Server {
        let mut child = Command::new(env!("CARGO_BIN_EXE_chorus-server"))
            .args([
                "--listen",
                &format!("127.0.0.1:{}", audio),
                "--source",
                "tone",
                "--serve-forever",
                "--allow-non-realtime",
                "--allow-unlocked-memory",
                "--ephemeral-identity",
                "--control-listen",
                &format!("127.0.0.1:{}", control),
                "--zone",
                "rack",
                "--state-file",
                state.to_str().unwrap(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the server binary runs");
        let (tx, lines) = mpsc::channel();
        for pipe in [
            Box::new(child.stdout.take().unwrap()) as Box<dyn Read + Send>,
            Box::new(child.stderr.take().unwrap()),
        ] {
            let tx = tx.clone();
            thread::spawn(move || {
                for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                    if tx.send(line).is_err() {
                        return;
                    }
                }
            });
        }
        let mut server = Server {
            child,
            lines,
            seen: Vec::new(),
        };
        server.wait_for("control listening on=");
        server.wait_for("chorus-server: listening on=");
        server
    }

    fn wait_for(&mut self, what: &str) -> String {
        if let Some(line) = self.seen.iter().find(|l| l.contains(what)) {
            return line.clone();
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if let Ok(line) = self.lines.recv_timeout(Duration::from_millis(100)) {
                self.seen.push(line.clone());
                if line.contains(what) {
                    return line;
                }
            }
        }
        panic!("the server never said {:?}", what);
    }
}

/// A fake evdev device: records written to a pipe, in real time.
struct FakeKeys(std::io::PipeWriter);

impl FakeKeys {
    fn edge(&mut self, code: u16, pressed: bool) {
        let mut bytes = Vec::new();
        for e in [
            InputEvent {
                kind: EV_KEY,
                code,
                value: i32::from(pressed),
            },
            InputEvent {
                kind: EV_SYN,
                code: 0,
                value: 0,
            },
        ] {
            bytes.extend_from_slice(&e.encode());
        }
        self.0
            .write_all(&bytes)
            .expect("the fake device takes a record");
    }

    /// Hold `code` for `ms`, then let go and wait 150 ms.
    fn press(&mut self, code: u16, ms: u64) {
        self.edge(code, true);
        thread::sleep(Duration::from_millis(ms));
        self.edge(code, false);
        thread::sleep(Duration::from_millis(150));
    }
}

fn fake_led(dir: &Path) -> PathBuf {
    let led = dir.join("sys/class/leds/rack:rgb:status");
    std::fs::create_dir_all(&led).unwrap();
    std::fs::write(led.join("max_brightness"), "255\n").unwrap();
    std::fs::write(led.join("multi_index"), "red green blue\n").unwrap();
    std::fs::write(led.join("multi_intensity"), "0 0 0\n").unwrap();
    std::fs::write(led.join("brightness"), "0\n").unwrap();
    led
}

fn wait_until(what: &str, f: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if f() {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for {}", what);
}

fn rack(link: &ControlLink) -> (String, u32) {
    let watch = ZoneWatch::new();
    watch.absorb(&link.state().expect("the control plane answers"), "rack");
    let facts = watch.facts();
    (facts.group, facts.volume_thousandths)
}

#[test]
fn rack_amp_buttons_change_the_room_on_a_real_server_through_the_client_session() {
    let dir = scratch("e2e");
    let (audio, control) = (free_port(), free_port());
    let mut server = Server::start(audio, control, &dir.join("zones.state"));

    // The endpoint is attached to the rack zone, as its control link does.
    let link = ControlLink {
        address: format!("127.0.0.1:{}", control),
        zone: "rack".to_string(),
        endpoint: "rack-amp".to_string(),
    };
    link.attach().expect("the endpoint attaches");
    assert_eq!(rack(&link), ("rack".to_string(), 1_000));

    // The panel: the ASSUMED rack-amp map, pointed at the fakes.
    let led_dir = fake_led(&dir);
    let panel_file = dir.join("front-panel.conf");
    std::fs::write(
        &panel_file,
        format!(
            "class streaming-amp\nroom rack\njoin-target downstairs\ninput {}\n\
             key {} play-pause\nkey {} volume-up\nkey {} volume-down\nkey {} pairing\nled {}\n",
            dir.join("fake-event0").display(),
            KEY_PLAYPAUSE,
            KEY_VOLUMEUP,
            KEY_VOLUMEDOWN,
            KEY_CONNECT,
            led_dir.display()
        ),
    )
    .unwrap();
    let panel_config = PanelConfig::load(&panel_file).expect("the panel configuration loads");
    assert_eq!(panel_config.roles(), roles::CONTROLLER | roles::VISUALIZER);
    let (keys_out, keys_in) = std::io::pipe().expect("a pipe");
    let mut keys = FakeKeys(keys_in);
    let timeline = MonotonicTimeline::new();
    let now: Arc<dyn Fn() -> u64 + Send + Sync> = Arc::new(move || timeline.now_ns());
    let said = Arc::new(Mutex::new(Vec::<String>::new()));
    let log: Arc<dyn Fn(&str) + Send + Sync> = {
        let said = Arc::clone(&said);
        Arc::new(move |line: &str| said.lock().unwrap().push(line.to_string()))
    };
    let panel = FrontPanel::start(
        &panel_config,
        "rack",
        vec![keys_out],
        Some(LedWriter::open(&led_dir).expect("the fake LED opens")),
        Arc::clone(&now),
        now,
        log,
    )
    .expect("the panel starts");

    // The client's session, declaring the roles the panel adds.
    let config = ClientConfig {
        endpoint: "rack-amp".to_string(),
        zone: "rack".to_string(),
        extra_roles: panel_config.roles(),
        ..ClientConfig::default()
    };
    let stream = TcpStream::connect(("127.0.0.1", audio)).expect("the server listens");
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let mut me = EndpointIdentity::ephemeral("rack-amp").unwrap();
    let secure = session::open(stream, &mut me, &config).expect("the session opens");
    let session::Session {
        mut reader,
        writer,
        announced,
        ..
    } = secure;
    session::also_hand(&mut reader, &announced, panel.server_messages());
    let writer = Arc::new(Mutex::new(writer));
    {
        let writer = Arc::clone(&writer);
        panel.connect(Box::new(move |m| writer.lock().unwrap().send(m)));
    }
    panel.set_playing(true);
    let session_line = server.wait_for("client session");
    assert!(
        session_line.contains(&format!(
            "roles={}",
            roles::PLAYER | roles::CONTROLLER | roles::VISUALIZER
        )),
        "the server saw the controller role declared: {session_line}"
    );
    // Keep reading the session, so the server's controller_state reaches
    // the panel through the handler (the audio itself is dropped).
    thread::spawn(move || {
        let mut sink = [0u8; 16 * 1024];
        loop {
            match reader.read(&mut sink) {
                Ok(0) => return,
                Ok(_) => {}
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(_) => return,
            }
        }
    });

    // The steady playing colour, on the fake LED class.
    let read = |name: &str| {
        std::fs::read_to_string(led_dir.join(name))
            .unwrap()
            .trim()
            .to_string()
    };
    wait_until("the LED shows playing", || {
        read("multi_intensity") == "255 255 255" && read("brightness") == "24"
    });

    // Volume down twice: 1.000 - 2 x 0.050 on the server.
    keys.press(KEY_VOLUMEDOWN, 100);
    keys.press(KEY_VOLUMEDOWN, 100);
    wait_until("the room's volume is 0.900", || rack(&link).1 == 900);
    let line = server.wait_for("command=volume_step value=-5");
    assert!(
        line.contains("id=rack-amp zone=rack") && line.contains("applied"),
        "{line}"
    );

    // Volume up once, held long enough to repeat once (600 ms): +10 points.
    keys.press(KEY_VOLUMEUP, 700);
    wait_until("the room's volume is back at 1.000", || {
        rack(&link).1 == 1_000
    });

    // A short press of play/pause is a toggle, which waits for an input.
    keys.press(KEY_PLAYPAUSE, 100);
    let line = server.wait_for("command=toggle");
    assert!(
        line.contains("transport-toggle-waits-for-an-input"),
        "{line}"
    );

    // Held 1.5 s while alone: join the configured target.
    keys.press(KEY_PLAYPAUSE, 1_500);
    wait_until("the room joined downstairs", || {
        rack(&link).0 == "downstairs"
    });
    // The server answered with controller_state; the panel applied it ...
    wait_until("the panel heard the group", || {
        said.lock()
            .unwrap()
            .iter()
            .any(|l| l.contains("controller-state") && l.contains("group=downstairs"))
    });
    // ... so the next long press leaves.
    keys.press(KEY_PLAYPAUSE, 1_500);
    wait_until("the room left the group", || rack(&link).0 == "rack");
    server.wait_for("command=leave");

    // Pairing is local: the light turns the pairing colour, nothing is sent.
    let sent_before = panel.counters().commands_sent.load(Ordering::Relaxed);
    keys.press(KEY_CONNECT, 100);
    wait_until("the LED shows pairing", || {
        read("multi_intensity") == "0 96 255" && read("brightness") == "64"
    });
    assert_eq!(
        panel.counters().commands_sent.load(Ordering::Relaxed),
        sent_before
    );

    // A key the map does not name is counted and sends nothing.
    keys.press(1, 100);
    wait_until("the unmapped key is counted", || {
        panel.counters().keys_unmapped.load(Ordering::Relaxed) >= 1
    });
    // Two presses, one repeat, a toggle, a join and a leave.
    assert_eq!(panel.counters().commands_sent.load(Ordering::Relaxed), 7);
    assert_eq!(panel.counters().commands_unsent.load(Ordering::Relaxed), 0);
    for line in said.lock().unwrap().iter() {
        println!("client: {line}");
    }
    for line in server.seen.iter().filter(|l| l.contains("controller")) {
        println!("server: {line}");
    }
    panel.disconnect();
    panel.stop();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_controller_command_from_an_endpoint_that_did_not_declare_the_role_changes_nothing() {
    let dir = scratch("no-role");
    let (audio, control) = (free_port(), free_port());
    let mut server = Server::start(audio, control, &dir.join("zones.state"));
    let link = ControlLink {
        address: format!("127.0.0.1:{}", control),
        zone: "rack".to_string(),
        endpoint: "plain".to_string(),
    };
    link.attach().unwrap();
    let config = ClientConfig {
        endpoint: "plain".to_string(),
        zone: "rack".to_string(),
        ..ClientConfig::default()
    };
    let stream = TcpStream::connect(("127.0.0.1", audio)).unwrap();
    let mut me = EndpointIdentity::ephemeral("plain").unwrap();
    let mut s = session::open(stream, &mut me, &config).expect("the session opens");
    s.writer
        .send(&chorus_protocol::v2::Message::ControllerCommand(
            chorus_protocol::v2::ControllerCommand {
                command: chorus_protocol::v2::Command::VolumeSet,
                value: 0,
                target: String::new(),
            },
        ))
        .unwrap();
    let line = server.wait_for("controller refused");
    assert!(line.contains("reason=no-controller-role"), "{line}");
    assert_eq!(rack(&link).1, 1_000, "the volume did not move");
    let _ = std::fs::remove_dir_all(&dir);
}
