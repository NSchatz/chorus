//! A small integer interpreter for the streaming TFLite models microWakeWord
//! trains (ADR 0000).
//!
//! It runs the thirteen operators those models use and nothing else: the
//! resource-variable operators that hold the streaming state (`CALL_ONCE`,
//! `VAR_HANDLE`, `READ_VARIABLE`, `ASSIGN_VARIABLE`), the shape operators
//! (`RESHAPE`, `CONCATENATION`, `STRIDED_SLICE`, `SPLIT_V`) and the int8
//! kernels (`CONV_2D`, `DEPTHWISE_CONV_2D`, `FULLY_CONNECTED`, `LOGISTIC`,
//! `QUANTIZE`). A model that asks for anything else is refused when it is
//! loaded, with the operator's number in the error.
//!
//! The arithmetic is TensorFlow Lite's quantization specification
//! (<https://ai.google.dev/edge/litert/models/quantization_spec>): int8
//! activations, int32 biases, per-channel filter scales, and a fixed-point
//! multiplier per output channel applied with a saturating rounding doubling
//! high multiply and a rounding right shift. Everything is checked when the
//! model is loaded (shapes, types, constants, variable sizes), so running it
//! allocates nothing and cannot fail.

use crate::flatbuf::Table;
use crate::Error;

const OP_CONCATENATION: i32 = 2;
const OP_CONV_2D: i32 = 3;
const OP_DEPTHWISE_CONV_2D: i32 = 4;
const OP_FULLY_CONNECTED: i32 = 9;
const OP_LOGISTIC: i32 = 14;
const OP_RESHAPE: i32 = 22;
const OP_STRIDED_SLICE: i32 = 45;
const OP_SPLIT_V: i32 = 102;
const OP_QUANTIZE: i32 = 114;
const OP_CALL_ONCE: i32 = 129;
const OP_VAR_HANDLE: i32 = 142;
const OP_READ_VARIABLE: i32 = 143;
const OP_ASSIGN_VARIABLE: i32 = 144;

/// The largest tensor a model may declare, in elements. The vendored model's
/// largest is 6400; the bound keeps a hostile file from asking for gigabytes.
const MAX_ELEMENTS: usize = 1 << 20;

/// What a tensor holds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    /// Signed 8-bit quantized values.
    I8,
    /// Unsigned 8-bit quantized values (stored as their bit pattern).
    U8,
    /// 32-bit integers: biases and shape constants.
    I32,
    /// A handle to a resource variable.
    Resource,
}

struct Tensor {
    shape: Vec<usize>,
    kind: Kind,
    scale: Vec<f32>,
    zero_point: Vec<i64>,
    quant_dim: usize,
    /// The values of an [`Kind::I8`] or [`Kind::U8`] tensor.
    data: Vec<i8>,
    /// The values of a constant [`Kind::I32`] tensor.
    ints: Vec<i32>,
    constant: bool,
    /// The variable a [`Kind::Resource`] tensor names, once `VAR_HANDLE` ran.
    var: Option<usize>,
}

impl Tensor {
    fn elements(&self) -> usize {
        self.shape.iter().product()
    }

    /// The single scale and zero point of a per-tensor quantized tensor.
    fn affine(&self) -> Result<(f32, i32), Error> {
        if self.scale.len() != 1 || self.zero_point.len() != 1 {
            return Err(Error::Unsupported(
                "a tensor without per-tensor quantization where one is needed".into(),
            ));
        }
        let zp = i32::try_from(self.zero_point[0])
            .map_err(|_| Error::Malformed("zero point out of range"))?;
        Ok((self.scale[0], zp))
    }

    fn range(&self) -> (i32, i32) {
        if self.kind == Kind::U8 {
            (0, 255)
        } else {
            (-128, 127)
        }
    }
}

/// Reads element `i` of a quantized tensor as an integer.
#[inline]
fn value(kind: Kind, raw: i8) -> i32 {
    if kind == Kind::U8 {
        i32::from(raw as u8)
    } else {
        i32::from(raw)
    }
}

struct Conv {
    input: usize,
    filter: usize,
    bias: Option<usize>,
    output: usize,
    stride: (usize, usize),
    dilation: (usize, usize),
    pad: (usize, usize),
    /// `None` for `CONV_2D`, the depth multiplier for `DEPTHWISE_CONV_2D`.
    depthwise: Option<usize>,
    input_offset: i32,
    output_offset: i32,
    multiplier: Vec<i32>,
    shift: Vec<i32>,
    act: (i32, i32),
}

struct FullyConnected {
    input: usize,
    filter: usize,
    bias: Option<usize>,
    output: usize,
    input_offset: i32,
    filter_offset: i32,
    output_offset: i32,
    multiplier: i32,
    shift: i32,
    act: (i32, i32),
}

enum Step {
    Conv(Conv),
    FullyConnected(FullyConnected),
    Copy {
        input: usize,
        output: usize,
    },
    Concat {
        inputs: Vec<usize>,
        output: usize,
        axis: usize,
    },
    Slice {
        input: usize,
        output: usize,
        begin: Vec<usize>,
    },
    Split {
        input: usize,
        outputs: Vec<usize>,
        axis: usize,
    },
    Read {
        var: usize,
        output: usize,
    },
    Assign {
        var: usize,
        input: usize,
    },
    Lookup {
        input: usize,
        output: usize,
        table: Box<[i8; 256]>,
    },
    Requantize {
        input: usize,
        output: usize,
        input_zero: i32,
        output_zero: i32,
        multiplier: i32,
        shift: i32,
    },
}

struct Graph {
    tensors: Vec<Tensor>,
    steps: Vec<Step>,
    inputs: Vec<usize>,
    outputs: Vec<usize>,
}

/// A loaded model: its graphs, its variables and the order its initialisers run in.
pub(crate) struct Model {
    graphs: Vec<Graph>,
    /// The subgraphs `CALL_ONCE` names, in call order: they set the variables' first values.
    init: Vec<usize>,
    vars: Vec<Vec<i8>>,
}

/// Decomposes a positive finite `x` into a fraction in `[0.5, 1)` and a power of two.
fn frexp(x: f64) -> (f64, i32) {
    if x == 0.0 || !x.is_finite() {
        return (x, 0);
    }
    let bits = x.to_bits();
    let exp = ((bits >> 52) & 0x7ff) as i32;
    if exp == 0 {
        // Subnormal: scale it into the normal range first.
        let (f, e) = frexp(x * (1u64 << 54) as f64);
        return (f, e - 54);
    }
    let frac = f64::from_bits((bits & !(0x7ffu64 << 52)) | (1022u64 << 52));
    (frac, exp - 1022)
}

/// The fixed-point form of a real multiplier: `real = multiplier / 2^31 * 2^shift`.
fn quantize_multiplier(real: f64) -> Result<(i32, i32), Error> {
    if !real.is_finite() || real < 0.0 {
        return Err(Error::Malformed(
            "a quantization scale that is not a positive number",
        ));
    }
    if real == 0.0 {
        return Ok((0, 0));
    }
    let (q, mut shift) = frexp(real);
    let mut fixed = (q * (1i64 << 31) as f64).round() as i64;
    if fixed == 1i64 << 31 {
        fixed /= 2;
        shift += 1;
    }
    if shift < -31 {
        return Ok((0, 0));
    }
    if shift > 30 {
        return Ok((i32::MAX, 30));
    }
    Ok((fixed as i32, shift))
}

#[inline]
fn saturating_rounding_doubling_high_mul(a: i32, b: i32) -> i32 {
    if a == i32::MIN && b == i32::MIN {
        return i32::MAX;
    }
    let ab = i64::from(a) * i64::from(b);
    let nudge: i64 = if ab >= 0 { 1 << 30 } else { 1 - (1 << 30) };
    // Truncating division, as the specification's reference does.
    ((ab + nudge) / (1i64 << 31)) as i32
}

#[inline]
fn rounding_divide_by_pot(x: i32, exponent: i32) -> i32 {
    let mask = ((1i64 << exponent) - 1) as i32;
    let remainder = x & mask;
    let threshold = (mask >> 1) + i32::from(x < 0);
    (x >> exponent) + i32::from(remainder > threshold)
}

#[inline]
fn multiply_by_quantized_multiplier(x: i32, multiplier: i32, shift: i32) -> i32 {
    let left = shift.max(0);
    let right = (-shift).max(0);
    rounding_divide_by_pot(
        saturating_rounding_doubling_high_mul(x.wrapping_mul(1 << left), multiplier),
        right,
    )
}

fn activation_range(code: i8, out: &Tensor) -> Result<(i32, i32), Error> {
    let (lo, hi) = out.range();
    let (scale, zero) = out.affine()?;
    let quantize = |v: f32| zero.saturating_add((v / scale).round() as i32);
    match code {
        0 => Ok((lo, hi)),
        1 => Ok((lo.max(quantize(0.0)), hi)),
        3 => Ok((lo.max(quantize(0.0)), hi.min(quantize(6.0)))),
        other => Err(Error::Unsupported(format!("fused activation {other}"))),
    }
}

/// Output size and leading padding of one spatial dimension.
fn conv_dim(
    input: usize,
    kernel: usize,
    stride: usize,
    dilation: usize,
    same: bool,
) -> Result<(usize, usize), Error> {
    if stride == 0 || dilation == 0 || kernel == 0 {
        return Err(Error::Malformed("a zero stride, dilation or kernel size"));
    }
    let effective = (kernel - 1) * dilation + 1;
    if same {
        let out = input.div_ceil(stride);
        let needed = (out.saturating_sub(1)) * stride + effective;
        Ok((out, needed.saturating_sub(input) / 2))
    } else {
        if input < effective {
            return Err(Error::Malformed("a kernel larger than its input"));
        }
        Ok(((input - effective) / stride + 1, 0))
    }
}

fn nhwc(t: &Tensor) -> Result<[usize; 4], Error> {
    <[usize; 4]>::try_from(t.shape.as_slice())
        .map_err(|_| Error::Unsupported("a convolution over a tensor that is not 4-D".into()))
}

fn axis_of(axis: i32, rank: usize) -> Result<usize, Error> {
    let a = if axis < 0 { axis + rank as i32 } else { axis };
    usize::try_from(a)
        .ok()
        .filter(|a| *a < rank)
        .ok_or(Error::Malformed("an axis outside the tensor's rank"))
}

struct Loader<'a> {
    opcodes: Vec<i32>,
    subgraphs: Vec<Table<'a>>,
    buffers: Vec<Table<'a>>,
    graphs: Vec<Option<Graph>>,
    init: Vec<usize>,
    var_names: Vec<(Vec<u8>, Vec<u8>)>,
    var_len: Vec<Option<usize>>,
}

impl<'a> Loader<'a> {
    fn tensor(&self, t: Table<'a>) -> Result<Tensor, Error> {
        let shape = t
            .i32s(0)?
            .into_iter()
            .map(|d| {
                usize::try_from(d)
                    .map_err(|_| Error::Unsupported("a tensor with a dynamic dimension".into()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut elements = 1usize;
        for d in &shape {
            elements = elements
                .checked_mul(*d)
                .filter(|e| *e <= MAX_ELEMENTS)
                .ok_or(Error::Malformed("a tensor larger than the limit"))?;
        }
        let kind = match t.i8(1, 0)? {
            9 => Kind::I8,
            3 => Kind::U8,
            2 => Kind::I32,
            13 => Kind::Resource,
            other => return Err(Error::Unsupported(format!("tensor type {other}"))),
        };
        let buffer = t.u32(2, 0)? as usize;
        let buffer = self.buffers.get(buffer).ok_or(Error::Malformed(
            "a tensor names a buffer that does not exist",
        ))?;
        if buffer.u32(1, 0)? != 0 || buffer.u32(2, 0)? != 0 {
            // 64-bit fields, but a non-zero low half is enough to know the data lives outside the table.
            return Err(Error::Unsupported(
                "a buffer stored outside the FlatBuffer".into(),
            ));
        }
        let raw = buffer.bytes(0)?;
        let (scale, zero_point, quant_dim) = match t.table(4)? {
            Some(q) => (
                q.f32s(2)?,
                q.i64s(3)?,
                usize::try_from(q.i32(6, 0)?)
                    .map_err(|_| Error::Malformed("negative quantized dimension"))?,
            ),
            None => (Vec::new(), Vec::new(), 0),
        };
        if scale.len() != zero_point.len() || scale.iter().any(|s| !s.is_finite() || *s <= 0.0) {
            return Err(Error::Malformed(
                "quantization parameters that do not pair up or are not positive",
            ));
        }
        let mut out = Tensor {
            shape,
            kind,
            scale,
            zero_point,
            quant_dim,
            data: Vec::new(),
            ints: Vec::new(),
            constant: !raw.is_empty(),
            var: None,
        };
        match kind {
            Kind::I8 | Kind::U8 => {
                if raw.is_empty() {
                    out.data = vec![0; elements];
                } else if raw.len() == elements {
                    out.data = raw.iter().map(|b| *b as i8).collect();
                } else {
                    return Err(Error::Malformed(
                        "a constant whose size does not match its shape",
                    ));
                }
            }
            Kind::I32 => {
                if raw.len() != elements * 4 {
                    return Err(Error::Unsupported(
                        "an int32 tensor that is not a constant".into(),
                    ));
                }
                out.ints = raw
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|c| i32::from_le_bytes(*c))
                    .collect();
            }
            Kind::Resource => {}
        }
        Ok(out)
    }

    fn graph(&mut self, index: usize) -> Result<(), Error> {
        if self
            .graphs
            .get(index)
            .ok_or(Error::Malformed("a subgraph index past the end"))?
            .is_some()
        {
            return Ok(());
        }
        let sub = self.subgraphs[index];
        let tensors = sub
            .tables(0)?
            .into_iter()
            .map(|t| self.tensor(t))
            .collect::<Result<Vec<_>, _>>()?;
        let ids = |v: Vec<i32>, n: usize| -> Result<Vec<usize>, Error> {
            v.into_iter()
                .map(|i| {
                    usize::try_from(i)
                        .ok()
                        .filter(|i| *i < n)
                        .ok_or(Error::Malformed("a tensor index past the end"))
                })
                .collect()
        };
        let n = tensors.len();
        let mut g = Graph {
            inputs: ids(sub.i32s(1)?, n)?,
            outputs: ids(sub.i32s(2)?, n)?,
            tensors,
            steps: Vec::new(),
        };
        for op in sub.tables(3)? {
            let code = *self
                .opcodes
                .get(op.u32(0, 0)? as usize)
                .ok_or(Error::Malformed(
                    "an operator names a code that does not exist",
                ))?;
            // An omitted optional input is written as -1.
            let raw_inputs = op.i32s(1)?;
            let optional = |i: usize| -> Result<Option<usize>, Error> {
                match raw_inputs.get(i) {
                    None | Some(-1) => Ok(None),
                    Some(v) => usize::try_from(*v)
                        .ok()
                        .filter(|v| *v < n)
                        .map(Some)
                        .ok_or(Error::Malformed("a tensor index past the end")),
                }
            };
            let input = |i: usize| {
                optional(i)?.ok_or(Error::Malformed("an operator lacks a required input"))
            };
            let outputs = ids(op.i32s(2)?, n)?;
            let output = |i: usize| {
                outputs
                    .get(i)
                    .copied()
                    .ok_or(Error::Malformed("an operator lacks a required output"))
            };
            for o in &outputs {
                if g.tensors[*o].constant
                    || raw_inputs.iter().any(|i| usize::try_from(*i) == Ok(*o))
                {
                    return Err(Error::Malformed(
                        "an operator writes a constant or one of its own inputs",
                    ));
                }
            }
            let options = op.table(4)?;
            let opt = || options.ok_or(Error::Malformed("an operator lacks its options"));
            match code {
                OP_CALL_ONCE => {
                    let target = usize::try_from(opt()?.i32(0, 0)?)
                        .map_err(|_| Error::Malformed("negative subgraph index"))?;
                    // Only the main graph may call an initialiser, and never itself: no cycle can form.
                    if index != 0
                        || target == 0
                        || target >= self.subgraphs.len()
                        || self.init.contains(&target)
                    {
                        return Err(Error::Malformed("CALL_ONCE outside the main graph, or naming it, a missing subgraph or one already called"));
                    }
                    self.init.push(target);
                    self.graph(target)?;
                }
                OP_VAR_HANDLE => {
                    let o = opt()?;
                    let name = (o.bytes(0)?.to_vec(), o.bytes(1)?.to_vec());
                    let id = match self.var_names.iter().position(|v| *v == name) {
                        Some(id) => id,
                        None => {
                            self.var_names.push(name);
                            self.var_len.push(None);
                            self.var_names.len() - 1
                        }
                    };
                    let t = &mut g.tensors[output(0)?];
                    if t.kind != Kind::Resource {
                        return Err(Error::Malformed(
                            "VAR_HANDLE writes a tensor that is not a resource",
                        ));
                    }
                    t.var = Some(id);
                }
                OP_READ_VARIABLE => {
                    let var = g.tensors[input(0)?]
                        .var
                        .ok_or(Error::Malformed("READ_VARIABLE before its VAR_HANDLE"))?;
                    let out = output(0)?;
                    let t = &g.tensors[out];
                    if !matches!(t.kind, Kind::I8 | Kind::U8)
                        || self.var_len[var] != Some(t.elements())
                    {
                        return Err(Error::Malformed(
                            "READ_VARIABLE of a variable that was not assigned that size",
                        ));
                    }
                    g.steps.push(Step::Read { var, output: out });
                }
                OP_ASSIGN_VARIABLE => {
                    let var = g.tensors[input(0)?]
                        .var
                        .ok_or(Error::Malformed("ASSIGN_VARIABLE before its VAR_HANDLE"))?;
                    let src = input(1)?;
                    let t = &g.tensors[src];
                    if !matches!(t.kind, Kind::I8 | Kind::U8)
                        || self.var_len[var].is_some_and(|l| l != t.elements())
                    {
                        return Err(Error::Malformed(
                            "ASSIGN_VARIABLE changes a variable's size or type",
                        ));
                    }
                    self.var_len[var] = Some(t.elements());
                    g.steps.push(Step::Assign { var, input: src });
                }
                OP_RESHAPE => {
                    let (i, o) = (input(0)?, output(0)?);
                    same_values(&g.tensors[i], &g.tensors[o])?;
                    if g.tensors[i].elements() != g.tensors[o].elements() {
                        return Err(Error::Malformed("RESHAPE changes the element count"));
                    }
                    g.steps.push(Step::Copy {
                        input: i,
                        output: o,
                    });
                }
                OP_CONCATENATION => {
                    let o = opt()?;
                    if o.i8(1, 0)? != 0 {
                        return Err(Error::Unsupported(
                            "CONCATENATION with a fused activation".into(),
                        ));
                    }
                    let out = output(0)?;
                    let rank = g.tensors[out].shape.len();
                    let axis = axis_of(o.i32(0, 0)?, rank)?;
                    let inputs = (0..raw_inputs.len())
                        .map(&input)
                        .collect::<Result<Vec<_>, _>>()?;
                    let mut along = 0usize;
                    for i in &inputs {
                        let (t, to) = (&g.tensors[*i], &g.tensors[out]);
                        same_values(t, to)?;
                        if t.shape.len() != rank
                            || (0..rank).any(|d| d != axis && t.shape[d] != to.shape[d])
                        {
                            return Err(Error::Malformed(
                                "CONCATENATION of shapes that do not line up",
                            ));
                        }
                        along += t.shape[axis];
                    }
                    if along != g.tensors[out].shape[axis] {
                        return Err(Error::Malformed(
                            "CONCATENATION output size is not the sum of its inputs",
                        ));
                    }
                    g.steps.push(Step::Concat {
                        inputs,
                        output: out,
                        axis,
                    });
                }
                OP_STRIDED_SLICE => {
                    let o = opt()?;
                    let (begin_mask, end_mask) = (o.i32(0, 0)?, o.i32(1, 0)?);
                    if o.i32(2, 0)? != 0 || o.i32(3, 0)? != 0 || o.i32(4, 0)? != 0 || o.bool(5)? {
                        return Err(Error::Unsupported(
                            "STRIDED_SLICE with an ellipsis, new-axis, shrink or offset option"
                                .into(),
                        ));
                    }
                    let (i, out) = (input(0)?, output(0)?);
                    same_values(&g.tensors[i], &g.tensors[out])?;
                    let shape = &g.tensors[i].shape;
                    let rank = shape.len();
                    let (b, e, s) = (
                        &g.tensors[input(1)?].ints,
                        &g.tensors[input(2)?].ints,
                        &g.tensors[input(3)?].ints,
                    );
                    if b.len() != rank
                        || e.len() != rank
                        || s.len() != rank
                        || g.tensors[out].shape.len() != rank
                        || rank > 31
                    {
                        return Err(Error::Malformed(
                            "STRIDED_SLICE bounds that do not match the tensor's rank",
                        ));
                    }
                    let mut begin = Vec::with_capacity(rank);
                    for d in 0..rank {
                        if s[d] != 1 {
                            return Err(Error::Unsupported(
                                "STRIDED_SLICE with a stride other than 1".into(),
                            ));
                        }
                        let dim = shape[d] as i64;
                        let clamp = |v: i32| {
                            (if v < 0 {
                                i64::from(v) + dim
                            } else {
                                i64::from(v)
                            })
                            .clamp(0, dim)
                        };
                        let lo = if begin_mask & (1 << d) != 0 {
                            0
                        } else {
                            clamp(b[d])
                        };
                        let hi = if end_mask & (1 << d) != 0 {
                            dim
                        } else {
                            clamp(e[d])
                        };
                        if hi - lo != g.tensors[out].shape[d] as i64 {
                            return Err(Error::Malformed(
                                "STRIDED_SLICE output shape does not match its bounds",
                            ));
                        }
                        begin.push(lo as usize);
                    }
                    g.steps.push(Step::Slice {
                        input: i,
                        output: out,
                        begin,
                    });
                }
                OP_SPLIT_V => {
                    let i = input(0)?;
                    let rank = g.tensors[i].shape.len();
                    let axis = axis_of(
                        *g.tensors[input(2)?]
                            .ints
                            .first()
                            .ok_or(Error::Malformed("SPLIT_V without an axis"))?,
                        rank,
                    )?;
                    let mut along = 0usize;
                    for out in &outputs {
                        let (t, ti) = (&g.tensors[*out], &g.tensors[i]);
                        same_values(ti, t)?;
                        if t.shape.len() != rank
                            || (0..rank).any(|d| d != axis && t.shape[d] != ti.shape[d])
                        {
                            return Err(Error::Malformed(
                                "SPLIT_V into shapes that do not line up",
                            ));
                        }
                        along += t.shape[axis];
                    }
                    if along != g.tensors[i].shape[axis] {
                        return Err(Error::Malformed(
                            "SPLIT_V outputs do not add up to the input",
                        ));
                    }
                    g.steps.push(Step::Split {
                        input: i,
                        outputs: outputs.clone(),
                        axis,
                    });
                }
                OP_CONV_2D | OP_DEPTHWISE_CONV_2D => {
                    let o = opt()?;
                    let depthwise = code == OP_DEPTHWISE_CONV_2D;
                    // DepthwiseConv2DOptions has depth_multiplier at field 3, which moves the rest down one.
                    let k = usize::from(depthwise);
                    let same = match o.i8(0, 0)? {
                        0 => true,
                        1 => false,
                        _ => return Err(Error::Malformed("an unknown padding")),
                    };
                    let positive = |v: i32| {
                        usize::try_from(v)
                            .map_err(|_| Error::Malformed("a negative stride or dilation"))
                    };
                    let stride = (positive(o.i32(2, 0)?)?, positive(o.i32(1, 0)?)?);
                    let dilation = (positive(o.i32(5 + k, 1)?)?, positive(o.i32(4 + k, 1)?)?);
                    let (i, f, bias, out) = (input(0)?, input(1)?, optional(2)?, output(0)?);
                    let (ti, tf, to) = (&g.tensors[i], &g.tensors[f], &g.tensors[out]);
                    if ti.kind != Kind::I8
                        || tf.kind != Kind::I8
                        || to.kind != Kind::I8
                        || !tf.constant
                    {
                        return Err(Error::Unsupported(
                            "a convolution that is not int8 with constant weights".into(),
                        ));
                    }
                    let ([batch, ih, iw, ic], [f0, kh, kw, f3], [ob, oh, ow, oc]) =
                        (nhwc(ti)?, nhwc(tf)?, nhwc(to)?);
                    let (eh, ph) = conv_dim(ih, kh, stride.0, dilation.0, same)?;
                    let (ew, pw) = conv_dim(iw, kw, stride.1, dilation.1, same)?;
                    let (channel_dim, multiplier_of) = if depthwise {
                        let m = positive(o.i32(3, 0)?)?;
                        if f0 != 1 || m == 0 || f3 != ic * m || oc != f3 {
                            return Err(Error::Malformed(
                                "DEPTHWISE_CONV_2D filter does not match its input and output",
                            ));
                        }
                        (3, Some(m))
                    } else {
                        if f0 != oc || f3 != ic {
                            return Err(Error::Malformed(
                                "CONV_2D filter does not match its input and output",
                            ));
                        }
                        (0, None)
                    };
                    if batch != 1 || ob != 1 || (oh, ow) != (eh, ew) {
                        return Err(Error::Malformed(
                            "a convolution whose output shape does not follow from its input",
                        ));
                    }
                    let (in_scale, in_zero) = ti.affine()?;
                    let (out_scale, out_zero) = to.affine()?;
                    if tf.zero_point.iter().any(|z| *z != 0)
                        || !(tf.scale.len() == oc && tf.quant_dim == channel_dim
                            || tf.scale.len() == 1)
                    {
                        return Err(Error::Unsupported(
                            "convolution weights that are not symmetric per-channel or per-tensor"
                                .into(),
                        ));
                    }
                    let mut multiplier = Vec::with_capacity(oc);
                    let mut shift = Vec::with_capacity(oc);
                    for c in 0..oc {
                        let fs = tf.scale[if tf.scale.len() == 1 { 0 } else { c }];
                        let (m, s) = quantize_multiplier(
                            f64::from(in_scale) * f64::from(fs) / f64::from(out_scale),
                        )?;
                        multiplier.push(m);
                        shift.push(s);
                    }
                    check_bias(&g.tensors, bias, oc)?;
                    let act = activation_range(o.i8(3 + k, 0)?, to)?;
                    g.steps.push(Step::Conv(Conv {
                        input: i,
                        filter: f,
                        bias,
                        output: out,
                        stride,
                        dilation,
                        pad: (ph, pw),
                        depthwise: multiplier_of,
                        input_offset: -in_zero,
                        output_offset: out_zero,
                        multiplier,
                        shift,
                        act,
                    }));
                }
                OP_FULLY_CONNECTED => {
                    let o = opt()?;
                    if o.i8(1, 0)? != 0 {
                        return Err(Error::Unsupported(
                            "FULLY_CONNECTED with shuffled weights".into(),
                        ));
                    }
                    let (i, f, bias, out) = (input(0)?, input(1)?, optional(2)?, output(0)?);
                    let (ti, tf, to) = (&g.tensors[i], &g.tensors[f], &g.tensors[out]);
                    if ti.kind != Kind::I8
                        || tf.kind != Kind::I8
                        || to.kind != Kind::I8
                        || !tf.constant
                    {
                        return Err(Error::Unsupported(
                            "FULLY_CONNECTED that is not int8 with constant weights".into(),
                        ));
                    }
                    let [units, width] = <[usize; 2]>::try_from(tf.shape.as_slice())
                        .map_err(|_| Error::Malformed("FULLY_CONNECTED weights are not 2-D"))?;
                    if ti.elements() != width || to.elements() != units {
                        return Err(Error::Unsupported(
                            "FULLY_CONNECTED over more than one row".into(),
                        ));
                    }
                    let (in_scale, in_zero) = ti.affine()?;
                    let (f_scale, f_zero) = tf.affine()?;
                    let (out_scale, out_zero) = to.affine()?;
                    let (multiplier, shift) = quantize_multiplier(
                        f64::from(in_scale) * f64::from(f_scale) / f64::from(out_scale),
                    )?;
                    check_bias(&g.tensors, bias, units)?;
                    let act = activation_range(o.i8(0, 0)?, to)?;
                    g.steps.push(Step::FullyConnected(FullyConnected {
                        input: i,
                        filter: f,
                        bias,
                        output: out,
                        input_offset: -in_zero,
                        filter_offset: -f_zero,
                        output_offset: out_zero,
                        multiplier,
                        shift,
                        act,
                    }));
                }
                OP_LOGISTIC => {
                    let (i, out) = (input(0)?, output(0)?);
                    let (ti, to) = (&g.tensors[i], &g.tensors[out]);
                    if ti.kind != Kind::I8 || to.kind != Kind::I8 || ti.elements() != to.elements()
                    {
                        return Err(Error::Unsupported(
                            "LOGISTIC that is not int8 to int8".into(),
                        ));
                    }
                    let (in_scale, in_zero) = ti.affine()?;
                    let (out_scale, out_zero) = to.affine()?;
                    // The table TensorFlow Lite's kernel builds: the real function on each of the
                    // 256 inputs, rounded onto the output's grid.
                    let mut table = Box::new([0i8; 256]);
                    let inverse = 1.0 / out_scale;
                    for v in -128i32..=127 {
                        let x = in_scale * (v - in_zero) as f32;
                        let y = 1.0 / (1.0 + (-x).exp());
                        let q = (y * inverse).round() + out_zero as f32;
                        table[(v as i8 as u8) as usize] = q.clamp(-128.0, 127.0) as i8;
                    }
                    g.steps.push(Step::Lookup {
                        input: i,
                        output: out,
                        table,
                    });
                }
                OP_QUANTIZE => {
                    let (i, out) = (input(0)?, output(0)?);
                    let (ti, to) = (&g.tensors[i], &g.tensors[out]);
                    if !matches!(ti.kind, Kind::I8 | Kind::U8)
                        || !matches!(to.kind, Kind::I8 | Kind::U8)
                        || ti.elements() != to.elements()
                    {
                        return Err(Error::Unsupported(
                            "QUANTIZE that is not between 8-bit tensors".into(),
                        ));
                    }
                    let (in_scale, input_zero) = ti.affine()?;
                    let (out_scale, output_zero) = to.affine()?;
                    let (multiplier, shift) =
                        quantize_multiplier(f64::from(in_scale) / f64::from(out_scale))?;
                    g.steps.push(Step::Requantize {
                        input: i,
                        output: out,
                        input_zero,
                        output_zero,
                        multiplier,
                        shift,
                    });
                }
                other => return Err(Error::Unsupported(format!("TFLite operator {other}"))),
            }
        }
        self.graphs[index] = Some(g);
        Ok(())
    }
}

/// Two tensors an operator copies between must hold the same kind of value on the same grid.
fn same_values(a: &Tensor, b: &Tensor) -> Result<(), Error> {
    if a.kind != b.kind
        || !matches!(a.kind, Kind::I8 | Kind::U8)
        || a.scale != b.scale
        || a.zero_point != b.zero_point
    {
        return Err(Error::Unsupported(
            "a shape operator between tensors of different type or quantization".into(),
        ));
    }
    Ok(())
}

fn check_bias(tensors: &[Tensor], bias: Option<usize>, channels: usize) -> Result<(), Error> {
    match bias {
        Some(b) if tensors[b].kind != Kind::I32 || tensors[b].ints.len() != channels => Err(
            Error::Malformed("a bias that is not one int32 per output channel"),
        ),
        _ => Ok(()),
    }
}

impl Graph {
    fn run(&mut self, vars: &mut [Vec<i8>]) {
        let Graph { tensors, steps, .. } = self;
        for step in steps.iter() {
            match step {
                Step::Read { var, output } => tensors[*output].data.copy_from_slice(&vars[*var]),
                Step::Assign { var, input } => {
                    let src = &tensors[*input].data;
                    let dst = &mut vars[*var];
                    dst.resize(src.len(), 0);
                    dst.copy_from_slice(src);
                }
                Step::Copy { input, output } => {
                    let mut out = std::mem::take(&mut tensors[*output].data);
                    out.copy_from_slice(&tensors[*input].data);
                    tensors[*output].data = out;
                }
                Step::Concat {
                    inputs,
                    output,
                    axis,
                } => {
                    let mut out = std::mem::take(&mut tensors[*output].data);
                    let outer: usize = tensors[*output].shape[..*axis].iter().product();
                    let inner: usize = tensors[*output].shape[*axis + 1..].iter().product();
                    let mut at = 0;
                    for o in 0..outer {
                        for i in inputs {
                            let n = tensors[*i].shape[*axis] * inner;
                            out[at..at + n].copy_from_slice(&tensors[*i].data[o * n..(o + 1) * n]);
                            at += n;
                        }
                    }
                    tensors[*output].data = out;
                }
                Step::Split {
                    input,
                    outputs,
                    axis,
                } => {
                    let outer: usize = tensors[*input].shape[..*axis].iter().product();
                    let inner: usize = tensors[*input].shape[*axis + 1..].iter().product();
                    let mut at = 0;
                    for o in 0..outer {
                        for out in outputs {
                            let n = tensors[*out].shape[*axis] * inner;
                            let mut data = std::mem::take(&mut tensors[*out].data);
                            data[o * n..(o + 1) * n]
                                .copy_from_slice(&tensors[*input].data[at..at + n]);
                            tensors[*out].data = data;
                            at += n;
                        }
                    }
                }
                Step::Slice {
                    input,
                    output,
                    begin,
                } => {
                    let mut out = std::mem::take(&mut tensors[*output].data);
                    let (src, dst) = (&tensors[*input], &tensors[*output]);
                    let rank = begin.len();
                    if rank == 0 {
                        out.copy_from_slice(&src.data);
                    } else {
                        // Copies one run along the last dimension per index of the others.
                        let run = dst.shape[rank - 1];
                        let rows = out.len().checked_div(run).unwrap_or(0);
                        for row in 0..rows {
                            let (mut rest, mut offset, mut stride) =
                                (row, begin[rank - 1], src.shape[rank - 1]);
                            for d in (0..rank - 1).rev() {
                                offset += (begin[d] + rest % dst.shape[d]) * stride;
                                rest /= dst.shape[d];
                                stride *= src.shape[d];
                            }
                            out[row * run..(row + 1) * run]
                                .copy_from_slice(&src.data[offset..offset + run]);
                        }
                    }
                    tensors[*output].data = out;
                }
                Step::Lookup {
                    input,
                    output,
                    table,
                } => {
                    let mut out = std::mem::take(&mut tensors[*output].data);
                    for (o, i) in out.iter_mut().zip(&tensors[*input].data) {
                        *o = table[(*i as u8) as usize];
                    }
                    tensors[*output].data = out;
                }
                Step::Requantize {
                    input,
                    output,
                    input_zero,
                    output_zero,
                    multiplier,
                    shift,
                } => {
                    let mut out = std::mem::take(&mut tensors[*output].data);
                    let (lo, hi) = tensors[*output].range();
                    let kind = tensors[*input].kind;
                    for (o, i) in out.iter_mut().zip(&tensors[*input].data) {
                        let v = multiply_by_quantized_multiplier(
                            value(kind, *i) - input_zero,
                            *multiplier,
                            *shift,
                        ) + output_zero;
                        // The low eight bits are the value for both an int8 and a uint8 output.
                        *o = v.clamp(lo, hi) as i8;
                    }
                    tensors[*output].data = out;
                }
                Step::FullyConnected(fc) => {
                    let mut out = std::mem::take(&mut tensors[fc.output].data);
                    let (input, filter) = (&tensors[fc.input].data, &tensors[fc.filter].data);
                    let width = input.len();
                    for (unit, o) in out.iter_mut().enumerate() {
                        let mut acc = fc.bias.map_or(0, |b| tensors[b].ints[unit]);
                        for (x, w) in input.iter().zip(&filter[unit * width..(unit + 1) * width]) {
                            acc = acc.wrapping_add(
                                (i32::from(*x) + fc.input_offset)
                                    * (i32::from(*w) + fc.filter_offset),
                            );
                        }
                        let v = multiply_by_quantized_multiplier(acc, fc.multiplier, fc.shift)
                            .saturating_add(fc.output_offset);
                        *o = v.clamp(fc.act.0, fc.act.1) as i8;
                    }
                    tensors[fc.output].data = out;
                }
                Step::Conv(c) => {
                    let mut out = std::mem::take(&mut tensors[c.output].data);
                    let (ti, tf, to) = (&tensors[c.input], &tensors[c.filter], &tensors[c.output]);
                    let (ih, iw, ic) = (ti.shape[1], ti.shape[2], ti.shape[3]);
                    let (kh, kw, fc) = (tf.shape[1], tf.shape[2], tf.shape[3]);
                    let (oh, ow, oc) = (to.shape[1], to.shape[2], to.shape[3]);
                    for oy in 0..oh {
                        for ox in 0..ow {
                            for ch in 0..oc {
                                let mut acc = c.bias.map_or(0, |b| tensors[b].ints[ch]);
                                for ky in 0..kh {
                                    // A tap in the padding contributes nothing: there the input is its zero point.
                                    let Some(iy) = (oy * c.stride.0 + ky * c.dilation.0)
                                        .checked_sub(c.pad.0)
                                        .filter(|y| *y < ih)
                                    else {
                                        continue;
                                    };
                                    for kx in 0..kw {
                                        let Some(ix) = (ox * c.stride.1 + kx * c.dilation.1)
                                            .checked_sub(c.pad.1)
                                            .filter(|x| *x < iw)
                                        else {
                                            continue;
                                        };
                                        let at = (iy * iw + ix) * ic;
                                        match c.depthwise {
                                            Some(m) => {
                                                let x = i32::from(ti.data[at + ch / m])
                                                    + c.input_offset;
                                                acc = acc.wrapping_add(
                                                    x * i32::from(
                                                        tf.data[(ky * kw + kx) * fc + ch],
                                                    ),
                                                );
                                            }
                                            None => {
                                                let w = &tf.data[((ch * kh + ky) * kw + kx) * fc..]
                                                    [..ic];
                                                for (x, w) in ti.data[at..at + ic].iter().zip(w) {
                                                    acc = acc.wrapping_add(
                                                        (i32::from(*x) + c.input_offset)
                                                            * i32::from(*w),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                                let v = multiply_by_quantized_multiplier(
                                    acc,
                                    c.multiplier[ch],
                                    c.shift[ch],
                                )
                                .saturating_add(c.output_offset);
                                out[(oy * ow + ox) * oc + ch] = v.clamp(c.act.0, c.act.1) as i8;
                            }
                        }
                    }
                    tensors[c.output].data = out;
                }
            }
        }
    }
}

/// A tensor's shape and the affine grid its 8-bit values sit on.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Port {
    pub(crate) shape: Vec<usize>,
    pub(crate) kind: Kind,
    pub(crate) scale: f32,
    pub(crate) zero_point: i32,
}

impl Model {
    /// Parses and checks a TFLite file, then sets the variables to their first values.
    pub(crate) fn load(file: &[u8]) -> Result<Self, Error> {
        let root = Table::root(file)?;
        let mut opcodes = Vec::new();
        for code in root.tables(1)? {
            // The 8-bit field is the code for operators below 127; newer ones use the 32-bit field.
            opcodes.push(i32::from(code.i8(0, 0)?).max(code.i32(3, 0)?));
        }
        let subgraphs = root.tables(2)?;
        if subgraphs.is_empty() {
            return Err(Error::Malformed("a model without a subgraph"));
        }
        let mut loader = Loader {
            opcodes,
            buffers: root.tables(4)?,
            graphs: subgraphs.iter().map(|_| None).collect(),
            subgraphs,
            init: Vec::new(),
            var_names: Vec::new(),
            var_len: Vec::new(),
        };
        loader.graph(0)?;
        let Loader {
            graphs,
            init,
            var_len,
            ..
        } = loader;
        // A subgraph nothing called is never run; an empty graph stands in for it.
        let graphs = graphs
            .into_iter()
            .map(|g| {
                g.unwrap_or(Graph {
                    tensors: Vec::new(),
                    steps: Vec::new(),
                    inputs: Vec::new(),
                    outputs: Vec::new(),
                })
            })
            .collect::<Vec<_>>();
        let main = &graphs[0];
        if main.inputs.len() != 1 || main.outputs.len() != 1 {
            return Err(Error::Unsupported(
                "a model without exactly one input and one output".into(),
            ));
        }
        for t in [main.inputs[0], main.outputs[0]] {
            let t = &main.tensors[t];
            if !matches!(t.kind, Kind::I8 | Kind::U8) || t.constant {
                return Err(Error::Unsupported(
                    "a model whose input or output is not an 8-bit tensor".into(),
                ));
            }
            t.affine()?;
        }
        let mut model = Self {
            graphs,
            init,
            vars: var_len
                .into_iter()
                .map(|l| vec![0; l.unwrap_or(0)])
                .collect(),
        };
        model.reset();
        Ok(model)
    }

    /// Puts every variable back to its first value, as when the model was loaded.
    pub(crate) fn reset(&mut self) {
        for v in &mut self.vars {
            v.fill(0);
        }
        for i in 0..self.init.len() {
            let g = self.init[i];
            self.graphs[g].run(&mut self.vars);
        }
    }

    fn port(&self, tensor: usize) -> Port {
        let t = &self.graphs[0].tensors[tensor];
        Port {
            shape: t.shape.clone(),
            kind: t.kind,
            scale: t.scale[0],
            zero_point: t.zero_point[0] as i32,
        }
    }

    pub(crate) fn input(&self) -> Port {
        self.port(self.graphs[0].inputs[0])
    }

    pub(crate) fn output(&self) -> Port {
        self.port(self.graphs[0].outputs[0])
    }

    /// The input tensor's values, to be filled before [`Model::invoke`].
    pub(crate) fn input_mut(&mut self) -> &mut [i8] {
        let t = self.graphs[0].inputs[0];
        &mut self.graphs[0].tensors[t].data
    }

    /// Runs the main graph once and returns the output tensor's raw values.
    pub(crate) fn invoke(&mut self) -> &[i8] {
        self.graphs[0].run(&mut self.vars);
        &self.graphs[0].tensors[self.graphs[0].outputs[0]].data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frexp_matches_its_definition() {
        for x in [
            1.0,
            0.5,
            0.75,
            3.0,
            1e-9,
            123456.789,
            f64::MIN_POSITIVE / 8.0,
        ] {
            let (f, e) = frexp(x);
            assert!((0.5..1.0).contains(&f), "{x}: {f}");
            // In two steps, so a subnormal's exponent does not underflow on the way.
            assert_eq!(f * 2f64.powi(e / 2) * 2f64.powi(e - e / 2), x);
        }
    }

    #[test]
    fn the_multiplier_rounds_as_the_specification_says() {
        // 1.0 is 2^30 / 2^31 * 2^1.
        assert_eq!(quantize_multiplier(1.0).unwrap(), (1 << 30, 1));
        assert_eq!(multiply_by_quantized_multiplier(77, 1 << 30, 1), 77);
        // A half rounds up, towards positive infinity.
        let (m, s) = quantize_multiplier(0.5).unwrap();
        assert_eq!(multiply_by_quantized_multiplier(3, m, s), 2);
        assert_eq!(multiply_by_quantized_multiplier(-3, m, s), -1);
        assert_eq!(
            saturating_rounding_doubling_high_mul(i32::MIN, i32::MIN),
            i32::MAX
        );
        assert_eq!(rounding_divide_by_pot(5, 1), 3);
        assert_eq!(rounding_divide_by_pot(-5, 1), -3);
    }
}
