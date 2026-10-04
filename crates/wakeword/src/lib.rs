//! The chorus wake-word detector: 16 kHz mono PCM in, "this phrase was said"
//! out.
//!
//! A pure library: no socket, no thread, no file read and no clock. Time is
//! counted in samples. It runs the streaming models the microWakeWord project
//! trains (Apache-2.0), which need two things computed exactly as they were in
//! training:
//!
//! 1. [`frontend`]: TensorFlow Lite's micro frontend, 40 mel features per
//!    10 ms, in fixed point.
//! 2. `interp`: an integer interpreter for the thirteen TFLite operators the
//!    models use, streaming state included.
//!
//! Both are held to the upstream implementation's output on committed
//! fixtures (`fixtures/wakeword/`, `tests/reference.rs`). Why chorus runs the
//! models with its own interpreter, and where the model and the fixtures come
//! from, is `docs/decisions/0000-the-wake-word-runtime.md`.
//!
//! # Which models
//!
//! Only models in `third_party/wakeword/`, each listed with its source,
//! checksum and licence in `third_party/wakeword/LICENCES.md`, and none whose
//! phrase is another party's trademark (`tools/conventions/check-wakeword.sh`
//! holds both). [`builtin`] returns them, compiled into the crate.
//!
//! # Use
//!
//! ```
//! use chorus_wakeword::{builtin, Detector};
//!
//! let mut detector = Detector::from_builtin(&builtin()[0]).unwrap();
//! let silence = [0i16; 1600];
//! assert!(detector.process(&silence).is_empty());
//! ```

mod fft;
mod flatbuf;
pub mod frontend;
mod interp;
mod manifest;

use std::collections::VecDeque;
use std::fmt;

use frontend::{Frontend, CHANNELS};
use interp::{Kind, Model};
pub use manifest::Manifest;

/// The step from a frontend feature to the value the models were trained on:
/// the frontend's 16-bit output times 10/256.
const FEATURE_SCALE: f64 = 0.039_062_5;

/// Why a model or a manifest was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The model file is not a well-formed TFLite FlatBuffer.
    Malformed(&'static str),
    /// The model is well-formed but uses something the interpreter does not run.
    Unsupported(String),
    /// The manifest is not a microWakeWord manifest this crate can honour.
    Manifest(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Malformed(what) => write!(f, "malformed model: {what}"),
            Error::Unsupported(what) => write!(f, "unsupported model: {what}"),
            Error::Manifest(what) => write!(f, "manifest: {what}"),
        }
    }
}

impl std::error::Error for Error {}

/// A model compiled into the crate from `third_party/wakeword/`.
#[derive(Clone, Copy, Debug)]
pub struct Builtin {
    /// The file stem, as the licence list names it (`okay_nabu`).
    pub name: &'static str,
    /// The TFLite file.
    pub model: &'static [u8],
    /// The manifest beside it.
    pub manifest: &'static str,
}

/// The vendored models. Every one is in the licence list; the conventions
/// check fails the build's gate when a file here is not.
pub fn builtin() -> &'static [Builtin] {
    const MODELS: &[Builtin] = &[Builtin {
        name: "okay_nabu",
        model: include_bytes!("../../../third_party/wakeword/okay_nabu.tflite"),
        manifest: include_str!("../../../third_party/wakeword/okay_nabu.json"),
    }];
    MODELS
}

/// One detection.
#[derive(Clone, Debug, PartialEq)]
pub struct Detection {
    /// The phrase that was heard, from the model's manifest.
    pub phrase: String,
    /// How many samples had been fed, since the detector was made or reset,
    /// when the phrase was recognised (the end of the window that tipped it).
    pub at_sample: u64,
    /// The mean probability over the sliding window, above the cutoff.
    pub probability: f32,
}

/// A streaming detector for one model.
pub struct Detector {
    manifest: Manifest,
    frontend: Frontend,
    model: Model,
    /// Feature frames per inference (the model's input height; 3 for the vendored model).
    stride: usize,
    input_scale: f64,
    input_zero: f64,
    input_range: (f64, f64),
    output_scale: f32,
    output_zero: i32,
    output_kind: Kind,
    /// Frames gathered towards the next inference.
    frames: usize,
    probabilities: VecDeque<f32>,
    /// False from a detection until the window's mean is back at or below the cutoff.
    armed: bool,
    samples: u64,
}

impl Detector {
    /// A detector for a model file and its manifest.
    pub fn new(model: &[u8], manifest: Manifest) -> Result<Self, Error> {
        let model = Model::load(model)?;
        let (input, output) = (model.input(), model.output());
        // The input is `stride` frames of 40 features, in any shape that says so.
        let elements: usize = input.shape.iter().product();
        if input.shape.last() != Some(&CHANNELS)
            || elements == 0
            || !elements.is_multiple_of(CHANNELS)
        {
            return Err(Error::Unsupported(
                "a model whose input is not frames of 40 features".into(),
            ));
        }
        if output.shape.iter().product::<usize>() != 1 {
            return Err(Error::Unsupported(
                "a model whose output is not one probability".into(),
            ));
        }
        let input_range = if input.kind == Kind::U8 {
            (0.0, 255.0)
        } else {
            (-128.0, 127.0)
        };
        Ok(Self {
            frontend: Frontend::new(),
            stride: elements / CHANNELS,
            input_scale: f64::from(input.scale),
            input_zero: f64::from(input.zero_point),
            input_range,
            output_scale: output.scale,
            output_zero: output.zero_point,
            output_kind: output.kind,
            frames: 0,
            probabilities: VecDeque::with_capacity(manifest.sliding_window_size),
            armed: true,
            samples: 0,
            model,
            manifest,
        })
    }

    /// A detector for one of the vendored models.
    pub fn from_builtin(builtin: &Builtin) -> Result<Self, Error> {
        Self::new(builtin.model, Manifest::from_json(builtin.manifest)?)
    }

    /// The phrase this detector listens for.
    pub fn phrase(&self) -> &str {
        &self.manifest.phrase
    }

    /// Forgets everything heard so far: buffered audio, the noise estimate,
    /// the model's streaming state and the sample count.
    pub fn reset(&mut self) {
        self.frontend.reset();
        self.model.reset();
        self.frames = 0;
        self.probabilities.clear();
        self.armed = true;
        self.samples = 0;
    }

    /// Feeds 16 kHz mono samples, in blocks of any length, and returns the
    /// detections they completed (none, nearly always).
    ///
    /// A detection is reported when the sliding window's mean probability
    /// rises above the manifest's cutoff, and not again until the mean has
    /// fallen back to the cutoff or below: the model's output stays high for
    /// some hundreds of milliseconds after a phrase, and that is one
    /// utterance, reported once.
    pub fn process(&mut self, mut samples: &[i16]) -> Vec<Detection> {
        let mut detections = Vec::new();
        while !samples.is_empty() {
            let (taken, frame) = self.frontend.process(samples);
            samples = &samples[taken..];
            self.samples += taken as u64;
            let Some(frame) = frame else { continue };
            if let Some(probability) = self.frame(&frame) {
                detections.push(Detection {
                    phrase: self.manifest.phrase.clone(),
                    at_sample: self.samples,
                    probability,
                });
            }
        }
        detections
    }

    /// Feeds one feature frame; runs the model when it has a stride of them.
    /// Returns the window's mean probability when it exceeds the cutoff.
    fn frame(&mut self, features: &[u16; CHANNELS]) -> Option<f32> {
        self.raw_frame(features)?;
        if self.probabilities.len() < self.manifest.sliding_window_size {
            return None;
        }
        let mean = self.probabilities.iter().sum::<f32>() / self.probabilities.len() as f32;
        if mean <= self.manifest.probability_cutoff {
            self.armed = true;
            return None;
        }
        // Above the cutoff: reported once, on the way up.
        std::mem::take(&mut self.armed).then_some(mean)
    }

    /// Feeds one feature frame and returns the model's raw output when this
    /// frame completed a stride and the model ran. Exposed for the reference
    /// test, which compares it with upstream's output value for value.
    #[doc(hidden)]
    pub fn raw_frame(&mut self, features: &[u16; CHANNELS]) -> Option<u8> {
        let (scale, zero, (lo, hi)) = (self.input_scale, self.input_zero, self.input_range);
        let row = &mut self.model.input_mut()[self.frames * CHANNELS..][..CHANNELS];
        for (q, f) in row.iter_mut().zip(features) {
            // Onto the input tensor's grid, ties to even as the reference runner rounds, and
            // saturated at the ends of the 8-bit range.
            let v = (f64::from(*f) * FEATURE_SCALE / scale + zero)
                .round_ties_even()
                .clamp(lo, hi);
            *q = v as i32 as i8;
        }
        self.frames += 1;
        if self.frames < self.stride {
            return None;
        }
        self.frames = 0;
        let raw = self.model.invoke()[0];
        let value = if self.output_kind == Kind::U8 {
            i32::from(raw as u8)
        } else {
            i32::from(raw)
        };
        if self.probabilities.len() == self.manifest.sliding_window_size {
            self.probabilities.pop_front();
        }
        self.probabilities
            .push_back((value - self.output_zero) as f32 * self.output_scale);
        Some(raw as u8)
    }
}
