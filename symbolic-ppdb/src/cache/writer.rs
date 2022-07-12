use std::{
    collections::{BTreeMap, HashMap},
    io::Write,
};

use indexmap::IndexMap;

use super::{raw, CacheError};
use crate::PortablePdb;

#[derive(Debug, Default)]
pub struct PortablePdbCacheConverter {
    pdb_id: [u8; 20],
    string_bytes: Vec<u8>,
    strings: HashMap<String, u32>,
    pub(crate) ranges: IndexMap<raw::Range, raw::SourceLocation>,
}

impl PortablePdbCacheConverter {
    pub fn new() -> Self {
        Self::default()
    }

    fn set_pdb_id(&mut self, id: [u8; 20]) {
        self.pdb_id = id;
    }

    /// Insert a string into this converter.
    ///
    /// If the string was already present, it is not added again. A newly added string
    /// is prefixed by its length in LEB128 encoding. The returned `u32`
    /// is the offset into the `string_bytes` field where the string is saved.
    fn insert_string(&mut self, s: &str) -> u32 {
        let Self {
            ref mut strings,
            ref mut string_bytes,
            ..
        } = self;
        if s.is_empty() {
            return u32::MAX;
        }
        if let Some(&offset) = strings.get(s) {
            return offset;
        }
        let string_offset = string_bytes.len() as u32;
        let string_len = s.len() as u64;
        leb128::write::unsigned(string_bytes, string_len).unwrap();
        string_bytes.extend(s.bytes());

        strings.insert(s.to_owned(), string_offset);
        string_offset
    }

    pub fn process_portable_pdb(&mut self, portable_pdb: &PortablePdb) -> Result<(), CacheError> {
        if let Some(id) = portable_pdb.pdb_id() {
            self.set_pdb_id(id);
        }

        for (method, sequence_points) in portable_pdb.get_all_sequence_points().enumerate() {
            let method = method + 1;
            let sequence_points = sequence_points?;
            for sp in sequence_points.iter() {
                let range = raw::Range {
                    idx: method as u32,
                    il_offset: sp.il_offset,
                };

                let doc = portable_pdb.get_document(sp.document_id as usize)?;
                let file_name_idx = self.insert_string(&doc.name);
                let source_location = raw::SourceLocation {
                    line: if sp.is_hidden() { 0 } else { sp.start_line },
                    file_name_idx,
                    lang: doc.lang as u32,
                };

                self.ranges.insert(range, source_location);
            }
        }

        Ok(())
    }

    // Methods for serializing to a [`Write`] below:
    // Feel free to move these to a separate file.

    /// Serialize the converted data.
    ///
    /// This writes the SymCache binary format into the given [`Write`].
    pub fn serialize<W: Write>(self, writer: &mut W) -> std::io::Result<()> {
        let mut writer = WriteWrapper::new(writer);

        let num_ranges = self.ranges.len() as u32;
        let string_bytes = self.string_bytes.len() as u32;

        let header = raw::Header {
            magic: raw::PPDBCACHE_MAGIC,
            version: super::PPDBCACHE_VERSION,

            pdb_id: self.pdb_id,

            num_ranges,
            string_bytes,
            _reserved: [0; 16],
        };

        writer.write(&[header])?;
        writer.align()?;

        for sl in self.ranges.values().copied() {
            writer.write(&[sl])?;
        }
        writer.align()?;

        for r in self.ranges.keys().copied() {
            writer.write(&[r])?;
        }
        writer.align()?;

        writer.write(&self.string_bytes)?;

        Ok(())
    }
}

struct WriteWrapper<W> {
    writer: W,
    position: usize,
}

impl<W: Write> WriteWrapper<W> {
    fn new(writer: W) -> Self {
        Self {
            writer,
            position: 0,
        }
    }

    fn write<T>(&mut self, data: &[T]) -> std::io::Result<usize> {
        let pointer = data.as_ptr() as *const u8;
        let len = std::mem::size_of_val(data);
        // SAFETY: both pointer and len are derived directly from data/T and are valid.
        let buf = unsafe { std::slice::from_raw_parts(pointer, len) };
        self.writer.write_all(buf)?;
        self.position += len;
        Ok(len)
    }

    fn align(&mut self) -> std::io::Result<usize> {
        let buf = &[0u8; 7];
        let len = {
            let to_align = self.position;
            let remainder = to_align % 8;
            if remainder == 0 {
                remainder
            } else {
                8 - remainder
            }
        };
        self.write(&buf[0..len])
    }
}
