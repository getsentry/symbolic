use std::mem;

use crate::{SymCacheError, SymCacheErrorKind};

/// The magic file preamble as individual bytes.
pub const SYMCACHE_MAGIC: [u8; 4] = *b"SYMC";

/// The start of a SymCache file.
#[repr(C, packed)]
#[derive(Default, Copy, Clone, Debug, PartialEq, Eq)]
pub struct Preamble {
    /// Magic bytes, see `SYMCACHE_MAGIC`.
    pub magic: [u8; 4],
    /// Version of the SymCache file format.
    pub version: u32,
}

impl Preamble {
    pub(crate) fn parse(buf: &[u8]) -> Result<Self, SymCacheError> {
        let preamble_size = mem::size_of::<Self>();

        if buf.len() < preamble_size {
            return Err(SymCacheErrorKind::BadFileHeader.into());
        }
        // SAFETY: we checked that the buffer is well aligned and large enough to fit a `Preamble`.
        let preamble = unsafe { &*(buf.as_ptr() as *const Self) };
        if preamble.magic != SYMCACHE_MAGIC {
            return Err(SymCacheErrorKind::BadFileMagic.into());
        }

        Ok(*preamble)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_preamble(preamble: &[Preamble]) -> Vec<u8> {
        let pointer = preamble.as_ptr() as *const u8;
        let len = mem::size_of_val(&preamble);
        // SAFETY: both pointer and len are derived directly from data/T and are valid.
        let bytes = unsafe { std::slice::from_raw_parts(pointer, len) };
        let mut buf = Vec::new();
        buf.write_all(bytes).unwrap();
        buf
    }

    #[test]
    fn correct_preamble() {
        let preamble = Preamble {
            magic: SYMCACHE_MAGIC,
            version: 1729,
        };
        let buf = write_preamble(&[preamble]);
        assert_eq!(Preamble::parse(&buf).unwrap(), preamble);
    }

    #[test]
    fn invalid_magic() {
        let preamble = Preamble {
            magic: *b"ABCD",
            version: 1729,
        };

        let buf = write_preamble(&[preamble]);
        assert_eq!(
            Preamble::parse(&buf).unwrap_err().kind(),
            SymCacheErrorKind::BadFileMagic,
        )
    }
}
