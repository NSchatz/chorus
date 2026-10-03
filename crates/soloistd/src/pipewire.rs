//! PipeWire for one receiver: the configuration and the two processes.
//!
//! Soloist plays into PipeWire and nothing else ("plays audio through
//! PipeWire or PulseAudio"), so every receiver container runs a headless
//! PipeWire whose only sink is a pipe-tunnel node that writes the
//! receiver's FIFO. The configuration below is the one goal 17's probe ran
//! (PipeWire 1.4.2 and WirePlumber 0.5.8 from Debian 13; the report is
//! `docs/soloist.md`, "PipeWire"): a complete `pipewire.conf` that names no
//! ALSA, Bluetooth, D-Bus or real-time module, the sink as a fragment, a
//! client configuration for Soloist's own PipeWire context, and a
//! WirePlumber profile with the linking policy only (without a session
//! manager a client is never linked to the sink and the FIFO stays empty).
//!
//! `tunnel.may-pause` is left at its default (false for a sink): a full
//! FIFO then drops audio instead of stalling Soloist, so a dead reader
//! cannot freeze a Spotify session.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use chorus_soloist::{PCM_CHANNELS, PCM_RATE, PIPEWIRE_FORMAT};

/// The WirePlumber profile name.
pub const PROFILE: &str = "chorus-receiver";

/// The name of PipeWire's socket in the runtime directory (`core.name`).
pub const SOCKET: &str = "pipewire-0";

/// The daemon's whole configuration.
pub fn daemon_conf() -> String {
    format!(
        "# chorus receiver: headless PipeWire, no ALSA, no bluez, no D-Bus, no real-time.\n\
         context.properties = {{\n\
         \x20   core.daemon       = true\n\
         \x20   core.name         = {SOCKET}\n\
         \x20   support.dbus      = false\n\
         \x20   mem.allow-mlock   = false\n\
         \x20   mem.warn-mlock    = false\n\
         \x20   link.max-buffers  = 16\n\
         \x20   default.clock.rate          = {PCM_RATE}\n\
         \x20   default.clock.allowed-rates = [ {PCM_RATE} ]\n\
         \x20   default.clock.quantum       = 1024\n\
         \x20   default.clock.min-quantum   = 1024\n\
         \x20   default.clock.max-quantum   = 2048\n\
         }}\n\
         context.spa-libs = {{\n\
         \x20   audio.convert.* = audioconvert/libspa-audioconvert\n\
         \x20   support.*       = support/libspa-support\n\
         }}\n\
         context.modules = [\n\
         \x20   {{ name = libpipewire-module-protocol-native }}\n\
         \x20   {{ name = libpipewire-module-metadata }}\n\
         \x20   {{ name = libpipewire-module-spa-node-factory }}\n\
         \x20   {{ name = libpipewire-module-client-node }}\n\
         \x20   {{ name = libpipewire-module-access }}\n\
         \x20   {{ name = libpipewire-module-adapter }}\n\
         \x20   {{ name = libpipewire-module-link-factory }}\n\
         \x20   {{ name = libpipewire-module-session-manager }}\n\
         ]\n\
         context.objects = [\n\
         \x20   {{ factory = spa-node-factory\n\
         \x20       args = {{\n\
         \x20           factory.name    = support.node.driver\n\
         \x20           node.name       = Dummy-Driver\n\
         \x20           node.group      = pipewire.dummy\n\
         \x20           priority.driver = 20000\n\
         \x20       }}\n\
         \x20   }}\n\
         ]\n"
    )
}

/// The pipe-tunnel sink of one receiver: node `node`, writing `fifo` in the
/// FIFO format of `chorus_soloist`.
pub fn sink_fragment(fifo: &Path, node: &str) -> String {
    format!(
        "context.modules = [\n\
         \x20   {{ name = libpipewire-module-pipe-tunnel\n\
         \x20       args = {{\n\
         \x20           tunnel.mode     = sink\n\
         \x20           pipe.filename   = \"{}\"\n\
         \x20           audio.format    = {PIPEWIRE_FORMAT}\n\
         \x20           audio.rate      = {PCM_RATE}\n\
         \x20           audio.channels  = {PCM_CHANNELS}\n\
         \x20           audio.position  = [ FL FR ]\n\
         \x20           stream.props = {{\n\
         \x20               node.name        = {node}\n\
         \x20               node.description = \"chorus receiver\"\n\
         \x20           }}\n\
         \x20       }}\n\
         \x20   }}\n\
         ]\n",
        fifo.display()
    )
}

/// The configuration of a PipeWire client in the container (Soloist).
pub const CLIENT_CONF: &str = "\
context.properties = {
    support.dbus    = false
    mem.allow-mlock = false
    mem.warn-mlock  = false
}
context.spa-libs = {
    audio.convert.* = audioconvert/libspa-audioconvert
    support.*       = support/libspa-support
}
context.modules = [
    { name = libpipewire-module-protocol-native }
    { name = libpipewire-module-client-node }
    { name = libpipewire-module-adapter }
    { name = libpipewire-module-metadata }
    { name = libpipewire-module-session-manager }
]
stream.properties = {
    resample.quality = 4
}
";

/// The WirePlumber profile: linking policy only, no device monitors, no
/// D-Bus, no saved state.
pub fn wireplumber_fragment() -> String {
    format!(
        "wireplumber.profiles = {{\n\
         \x20 {PROFILE} = {{\n\
         \x20   inherits = [ policy, mixin.systemwide-session, mixin.stateless ]\n\
         \x20   support.dbus = disabled\n\
         \x20 }}\n\
         }}\n"
    )
}

/// The directories and files of one receiver's PipeWire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// The runtime directory (`XDG_RUNTIME_DIR`), mode 0700.
    pub runtime: PathBuf,
    /// `PIPEWIRE_CONFIG_DIR`.
    pub pipewire_config: PathBuf,
    /// The directory holding chorus's WirePlumber fragment; listed before
    /// the stock one in `WIREPLUMBER_CONFIG_DIR`.
    pub wireplumber_config: PathBuf,
}

impl Layout {
    /// The layout under a runtime directory.
    pub fn under(runtime: &Path) -> Layout {
        Layout {
            runtime: runtime.to_path_buf(),
            pipewire_config: runtime.join("conf"),
            wireplumber_config: runtime.join("wireplumber"),
        }
    }

    /// PipeWire's socket.
    pub fn socket(&self) -> PathBuf {
        self.runtime.join(SOCKET)
    }

    /// Write every configuration file.
    pub fn write(&self, fifo: &Path, node: &str) -> io::Result<()> {
        fs::create_dir_all(&self.runtime)?;
        fs::set_permissions(&self.runtime, fs::Permissions::from_mode(0o700))?;
        let fragments = self.pipewire_config.join("pipewire.conf.d");
        fs::create_dir_all(&fragments)?;
        fs::write(self.pipewire_config.join("pipewire.conf"), daemon_conf())?;
        fs::write(self.pipewire_config.join("client.conf"), CLIENT_CONF)?;
        fs::write(
            fragments.join("10-chorus-receiver.conf"),
            sink_fragment(fifo, node),
        )?;
        let wireplumber = self.wireplumber_config.join("wireplumber.conf.d");
        fs::create_dir_all(&wireplumber)?;
        fs::write(
            wireplumber.join("10-chorus-receiver.conf"),
            wireplumber_fragment(),
        )
    }

    /// The environment PipeWire, WirePlumber and Soloist all run with.
    pub fn environment(&self, stock_wireplumber: &Path) -> Vec<(&'static str, String)> {
        vec![
            ("XDG_RUNTIME_DIR", self.runtime.display().to_string()),
            (
                "PIPEWIRE_CONFIG_DIR",
                self.pipewire_config.display().to_string(),
            ),
            (
                "WIREPLUMBER_CONFIG_DIR",
                format!(
                    "{}:{}",
                    self.wireplumber_config.display(),
                    stock_wireplumber.display()
                ),
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sink_writes_the_fifo_in_the_fifo_format() {
        let text = sink_fragment(Path::new("/run/chorus/soloist/r3.pcm"), "chorus-r3");
        for needle in [
            "name = libpipewire-module-pipe-tunnel",
            "tunnel.mode     = sink",
            "pipe.filename   = \"/run/chorus/soloist/r3.pcm\"",
            "audio.format    = F32LE",
            "audio.rate      = 44100",
            "audio.channels  = 2",
            "node.name        = chorus-r3",
        ] {
            assert!(text.contains(needle), "{needle} in\n{text}");
        }
        // The default stays: a full FIFO drops, it never stalls Soloist.
        assert!(!text.contains("may-pause"));
        assert_eq!(text.matches('{').count(), text.matches('}').count());
        assert_eq!(text.matches('[').count(), text.matches(']').count());
    }

    #[test]
    fn the_daemon_names_no_device_dbus_or_realtime_module() {
        let text = daemon_conf();
        for absent in ["alsa", "bluez", "rt", "portal", "jack", "x11", "v4l2"] {
            assert!(
                !text.contains(&format!("module-{absent}")),
                "{absent} in\n{text}"
            );
        }
        assert!(text.contains("support.dbus      = false"));
        assert!(text.contains("default.clock.rate          = 44100"));
        assert!(text.contains("core.name         = pipewire-0"));
        assert_eq!(text.matches('{').count(), text.matches('}').count());
        assert!(wireplumber_fragment().contains("chorus-receiver = {"));
    }

    #[test]
    fn the_layout_is_written_under_a_private_runtime_directory() {
        let root = std::env::temp_dir().join(format!("chorus-soloistd-pw-{}", std::process::id()));
        let layout = Layout::under(&root);
        layout.write(Path::new("/x/r0.pcm"), "chorus-r0").unwrap();
        let mode = fs::metadata(&root).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        for file in [
            "conf/pipewire.conf",
            "conf/client.conf",
            "conf/pipewire.conf.d/10-chorus-receiver.conf",
            "wireplumber/wireplumber.conf.d/10-chorus-receiver.conf",
        ] {
            assert!(root.join(file).is_file(), "{file}");
        }
        let env = layout.environment(Path::new("/usr/share/wireplumber"));
        assert_eq!(env[0], ("XDG_RUNTIME_DIR", root.display().to_string()));
        assert_eq!(
            env[2].1,
            format!("{}/wireplumber:/usr/share/wireplumber", root.display())
        );
        assert_eq!(layout.socket(), root.join("pipewire-0"));
        fs::remove_dir_all(&root).unwrap();
    }
}
