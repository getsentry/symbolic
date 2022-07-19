use std::collections::{BTreeMap, HashMap};
use std::io::Write;

use indexmap::IndexSet;
use symbolic_common::Language;
use zerocopy::AsBytes;

use super::{raw, CacheError};
use crate::PortablePdb;

/// The PortablePdbCache Converter.
///
/// This can extract data from a [`PortablePdb`] struct and
/// serialize it to disk via its [`serialize`](PortablePdbCacheConverter::serialize) method.
#[derive(Debug, Default)]
pub struct PortablePdbCacheConverter {
    /// A byte sequence uniquely representing the debugging metadata blob content.
    pdb_id: [u8; 20],
    /// The set of all [`raw::File`]s that have been added to this `Converter`.
    files: IndexSet<raw::File>,
    /// The concatenation of all strings that have been added to this `Converter`.
    string_bytes: Vec<u8>,
    /// A map from [`String`]s that have been added to this `Converter` to their offsets in the `string_bytes` field.
    strings: HashMap<String, u32>,
    /// A map from [`raw::Range`]s to the [`raw::SourceLocation`]s they correspond to.
    ///
    /// Only the starting address of a range is saved, the end address is given implicitly
    /// by the start address of the next range.
    ranges: BTreeMap<raw::Range, raw::SourceLocation>,
}

impl PortablePdbCacheConverter {
    /// Creates a new Converter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Processes a Portable PDB file, inserting its sequence point information into this converter.
    pub fn process_portable_pdb(&mut self, portable_pdb: &PortablePdb) -> Result<(), CacheError> {
        if let Some(id) = portable_pdb.pdb_id() {
            self.set_pdb_id(id);
        }

        for (function, sequence_points) in portable_pdb.get_all_sequence_points().enumerate() {
            let func_idx = (function + 1) as u32;
            let sequence_points = sequence_points?;

            for sp in sequence_points.iter() {
                let range = raw::Range {
                    func_idx,
                    il_offset: sp.il_offset,
                };

                let doc = portable_pdb.get_document(sp.document_id as usize)?;
                let file_idx = self.insert_file(&doc.name, doc.lang);
                let source_location = raw::SourceLocation {
                    line: if sp.is_hidden() { 0 } else { sp.start_line },
                    file_idx,
                };

                self.ranges.insert(range, source_location);
            }
        }

        Ok(())
    }

    /// Serialize the converted data.
    ///
    /// This writes the PortablePdbCache binary format into the given [`Write`].
    pub fn serialize<W: Write>(self, writer: &mut W) -> std::io::Result<()> {
        let mut writer = WriteWrapper::new(writer);

        let num_ranges = self.ranges.len() as u32;
        let num_files = self.files.len() as u32;
        let string_bytes = self.string_bytes.len() as u32;

        let header = raw::Header {
            magic: raw::PPDBCACHE_MAGIC,
            version: super::PPDBCACHE_VERSION,

            pdb_id: self.pdb_id,

            num_files,
            num_ranges,
            string_bytes,
            _reserved: [0; 16],
        };

        writer.write(header.as_bytes())?;
        writer.align()?;

        for file in self.files.into_iter() {
            writer.write(file.as_bytes())?;
        }
        writer.align()?;

        for sl in self.ranges.values() {
            writer.write(sl.as_bytes())?;
        }
        writer.align()?;

        for r in self.ranges.keys() {
            writer.write(r.as_bytes())?;
        }
        writer.align()?;

        writer.write(&self.string_bytes)?;

        Ok(())
    }

    fn set_pdb_id(&mut self, id: [u8; 20]) {
        self.pdb_id = id;
    }

    fn insert_file(&mut self, name: &str, lang: Language) -> u32 {
        let name_offset = self.insert_string(name);
        let file = raw::File {
            name_offset,
            lang: lang as u32,
        };

        self.files.insert_full(file).0 as u32
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

    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let len = data.len();
        self.writer.write_all(data)?;
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
