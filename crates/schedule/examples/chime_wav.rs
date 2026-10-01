//! Write a chime as a WAV file, to listen to it before changing a digest:
//! `cargo run -p chorus-schedule --example chime_wav -- bell 48000 /scratch/bell.wav`
//! (mono, 16-bit). A tool for a person; no test or gate runs it.

use chorus_schedule::{render, Chime, PcmFormat};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: chime_wav <bell|ding-dong|triad> <rate_hz> <out.wav>");
        std::process::exit(2);
    }
    let Some(chime) = Chime::from_name(&args[1]) else {
        eprintln!("no chime named {:?}", args[1]);
        std::process::exit(2);
    };
    let rate: u32 = args[2].parse().unwrap_or(0);
    let pcm = match render(chime, rate, 1, PcmFormat::S16Le) {
        Ok(pcm) => pcm,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    // The canonical 44-byte RIFF/WAVE header for 16-bit PCM.
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    wav.extend_from_slice(&pcm);
    if let Err(e) = std::fs::write(&args[3], wav) {
        eprintln!("{}: {e}", args[3]);
        std::process::exit(1);
    }
}
