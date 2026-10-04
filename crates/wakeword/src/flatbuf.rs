//! A bounds-checked reader for the parts of the TFLite FlatBuffers schema the
//! interpreter needs.
//!
//! Written from the FlatBuffers binary format description
//! (<https://flatbuffers.dev/internals/>) and the field order of TensorFlow
//! Lite's `schema.fbs` (Apache-2.0). Every offset is checked against the
//! buffer, so a truncated or hostile file is an [`Error`], never a panic.

use crate::Error;

/// One table inside the buffer: where it starts and where its vtable is.
#[derive(Clone, Copy)]
pub(crate) struct Table<'a> {
    buf: &'a [u8],
    pos: usize,
    vtable: usize,
    vtable_len: usize,
}

fn bytes(buf: &[u8], at: usize, n: usize) -> Result<&[u8], Error> {
    let end = at
        .checked_add(n)
        .ok_or(Error::Malformed("offset overflow"))?;
    buf.get(at..end)
        .ok_or(Error::Malformed("offset past the end of the file"))
}

fn u16_at(buf: &[u8], at: usize) -> Result<u16, Error> {
    let b = bytes(buf, at, 2)?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(buf: &[u8], at: usize) -> Result<u32, Error> {
    let b = bytes(buf, at, 4)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn i32_at(buf: &[u8], at: usize) -> Result<i32, Error> {
    Ok(u32_at(buf, at)? as i32)
}

/// Follows the unsigned offset stored at `at`.
fn indirect(buf: &[u8], at: usize) -> Result<usize, Error> {
    let off = u32_at(buf, at)? as usize;
    at.checked_add(off)
        .ok_or(Error::Malformed("offset overflow"))
}

impl<'a> Table<'a> {
    /// The root table of a file with the `TFL3` identifier.
    pub(crate) fn root(buf: &'a [u8]) -> Result<Self, Error> {
        if bytes(buf, 4, 4)? != b"TFL3" {
            return Err(Error::Malformed("not a TFLite file (no TFL3 identifier)"));
        }
        Self::at(buf, indirect(buf, 0)?)
    }

    fn at(buf: &'a [u8], pos: usize) -> Result<Self, Error> {
        let back = i32_at(buf, pos)? as i64;
        let vtable = (pos as i64)
            .checked_sub(back)
            .filter(|v| *v >= 0)
            .ok_or(Error::Malformed("vtable offset"))? as usize;
        let vtable_len = u16_at(buf, vtable)? as usize;
        if vtable_len < 4 {
            return Err(Error::Malformed("vtable shorter than its header"));
        }
        bytes(buf, vtable, vtable_len)?;
        Ok(Self {
            buf,
            pos,
            vtable,
            vtable_len,
        })
    }

    /// The absolute position of field `id`, or `None` when the table omits it.
    fn field(&self, id: usize) -> Result<Option<usize>, Error> {
        let slot = 4 + 2 * id;
        if slot + 2 > self.vtable_len {
            return Ok(None);
        }
        let off = u16_at(self.buf, self.vtable + slot)? as usize;
        Ok(if off == 0 { None } else { Some(self.pos + off) })
    }

    pub(crate) fn u8(&self, id: usize, default: u8) -> Result<u8, Error> {
        Ok(match self.field(id)? {
            Some(at) => bytes(self.buf, at, 1)?[0],
            None => default,
        })
    }

    pub(crate) fn i8(&self, id: usize, default: i8) -> Result<i8, Error> {
        Ok(self.u8(id, default as u8)? as i8)
    }

    pub(crate) fn bool(&self, id: usize) -> Result<bool, Error> {
        Ok(self.u8(id, 0)? != 0)
    }

    pub(crate) fn i32(&self, id: usize, default: i32) -> Result<i32, Error> {
        Ok(match self.field(id)? {
            Some(at) => i32_at(self.buf, at)?,
            None => default,
        })
    }

    pub(crate) fn u32(&self, id: usize, default: u32) -> Result<u32, Error> {
        Ok(self.i32(id, default as i32)? as u32)
    }

    pub(crate) fn table(&self, id: usize) -> Result<Option<Table<'a>>, Error> {
        match self.field(id)? {
            Some(at) => Ok(Some(Table::at(self.buf, indirect(self.buf, at)?)?)),
            None => Ok(None),
        }
    }

    /// A vector field as (position of element 0, element count); an omitted
    /// vector is empty.
    fn vector(&self, id: usize, elem: usize) -> Result<(usize, usize), Error> {
        let Some(at) = self.field(id)? else {
            return Ok((0, 0));
        };
        let start = indirect(self.buf, at)?;
        let len = u32_at(self.buf, start)? as usize;
        let total = len
            .checked_mul(elem)
            .ok_or(Error::Malformed("vector length overflow"))?;
        bytes(self.buf, start + 4, total)?;
        Ok((start + 4, len))
    }

    pub(crate) fn bytes(&self, id: usize) -> Result<&'a [u8], Error> {
        let (start, len) = self.vector(id, 1)?;
        bytes(self.buf, start, len)
    }

    pub(crate) fn i32s(&self, id: usize) -> Result<Vec<i32>, Error> {
        let (start, len) = self.vector(id, 4)?;
        (0..len).map(|i| i32_at(self.buf, start + 4 * i)).collect()
    }

    pub(crate) fn f32s(&self, id: usize) -> Result<Vec<f32>, Error> {
        let (start, len) = self.vector(id, 4)?;
        (0..len)
            .map(|i| Ok(f32::from_bits(u32_at(self.buf, start + 4 * i)?)))
            .collect()
    }

    pub(crate) fn i64s(&self, id: usize) -> Result<Vec<i64>, Error> {
        let (start, len) = self.vector(id, 8)?;
        (0..len)
            .map(|i| {
                let b = bytes(self.buf, start + 8 * i, 8)?;
                let mut a = [0u8; 8];
                a.copy_from_slice(b);
                Ok(i64::from_le_bytes(a))
            })
            .collect()
    }

    pub(crate) fn tables(&self, id: usize) -> Result<Vec<Table<'a>>, Error> {
        let (start, len) = self.vector(id, 4)?;
        (0..len)
            .map(|i| Table::at(self.buf, indirect(self.buf, start + 4 * i)?))
            .collect()
    }
}
