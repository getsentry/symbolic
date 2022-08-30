use std::collections::BTreeMap;
use std::io::Write;

use indexmap::IndexSet;
use symbolic_common::Language;
use watto::{Pod, StringTable};

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
    /// A map from [`raw::Range`]s to the [`raw::SourceLocation`]s they correspond to.
    ///
    /// Only the starting address of a range is saved, the end address is given implicitly
    /// by the start address of the next range.
    ranges: BTreeMap<raw::Range, raw::SourceLocation>,
    string_table: StringTable,
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
        let mut writer = watto::Writer::new(writer);

        let num_ranges = self.ranges.len() as u32;
        let num_files = self.files.len() as u32;

        let header = raw::Header {
            magic: raw::PPDBCACHE_MAGIC,
            version: super::PPDBCACHE_VERSION,

            pdb_id: self.pdb_id,

            num_files,
            num_ranges,
            string_bytes: self.string_table.len() as u32,
            _reserved: [0; 16],
        };

        writer.write_all(header.as_bytes())?;
        writer.align_to(8)?;

        for file in self.files.into_iter() {
            writer.write_all(file.as_bytes())?;
        }
        writer.align_to(8)?;

        for sl in self.ranges.values() {
            writer.write_all(sl.as_bytes())?;
        }
        writer.align_to(8)?;

        for r in self.ranges.keys() {
            writer.write_all(r.as_bytes())?;
        }
        writer.align_to(8)?;

        writer.write_all(self.string_table.as_bytes())?;

        Ok(())
    }

    fn set_pdb_id(&mut self, id: [u8; 20]) {
        self.pdb_id = id;
    }

    fn insert_file(&mut self, name: &str, lang: Language) -> u32 {
        let name_offset = self.string_table.insert(name) as u32;
        let file = raw::File {
            name_offset,
            lang: lang as u32,
        };

        self.files.insert_full(file).0 as u32
    }
}
