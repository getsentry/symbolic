//! Defines the [SymCache Converter](`SymCacheConverter`).

use std::collections::btree_map;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;

use indexmap::IndexSet;
use symbolic_common::{Arch, DebugId, Language};
use symbolic_debuginfo::{DebugSession, Function, ObjectLike, Symbol};

use super::raw;
use crate::{SymCacheError, SymCacheErrorKind};

/// The SymCache Converter.
///
/// This can convert data in various source formats to an intermediate representation, which can
/// then be serialized to disk via its [`serialize`](SymCacheConverter::serialize) method.
#[derive(Debug, Default)]
pub struct SymCacheConverter {
    /// Debug identifier of the object file.
    debug_id: DebugId,
    /// CPU architecture of the object file.
    arch: Arch,

    /// The concatenation of all strings that have been added to this `Converter`.
    string_bytes: Vec<u8>,
    /// A map from [`String`]s that have been added to this `Converter` to their offsets in the `string_bytes` field.
    strings: HashMap<String, u32>,
    /// The set of all [`raw::File`]s that have been added to this `Converter`.
    files: IndexSet<raw::File>,
    /// The set of all [`raw::Function`]s that have been added to this `Converter`.
    functions: IndexSet<raw::Function>,
    /// The set of all [`raw::SourceLocation`]s that have been added to this `Converter` and that
    /// aren't directly associated with a code range.
    source_locations: IndexSet<raw::SourceLocation>,
    /// A map from code ranges to the [`raw::SourceLocation`]s they correspond to.
    ///
    /// Only the starting address of a range is saved, the end address is given implicitly
    /// by the start address of the next range.
    ranges: BTreeMap<u32, raw::SourceLocation>,

    /// This is highest addr that we know is outside of a valid function.
    /// Functions have an explicit end, while Symbols implicitly extend to infinity.
    /// In case the highest addr belongs to a Symbol, this will be `None` and the SymCache
    /// also extends to infinite, otherwise this is the end of the highest function.
    last_addr: Option<u32>,
}

impl SymCacheConverter {
    /// Creates a new Converter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the CPU architecture of this SymCache.
    pub fn set_arch(&mut self, arch: Arch) {
        self.arch = arch;
    }

    /// Sets the debug identifier of this SymCache.
    pub fn set_debug_id(&mut self, debug_id: DebugId) {
        self.debug_id = debug_id;
    }

    /// Insert a string into this converter.
    ///
    /// If the string was already present, it is not added again. A newly added string
    /// is prefixed by its length as a `u32`. The returned `u32`
    /// is the offset into the `string_bytes` field where the string is saved.
    fn insert_string(&mut self, s: &str) -> u32 {
        if s.is_empty() {
            return u32::MAX;
        }
        if let Some(&offset) = self.strings.get(s) {
            return offset;
        }
        let string_offset = self.string_bytes.len() as u32;
        let string_len = s.len() as u32;
        self.string_bytes.extend(string_len.to_ne_bytes());
        self.string_bytes.extend(s.bytes());
        // we should have written exactly `string_len + 4` bytes
        debug_assert_eq!(
            self.string_bytes.len(),
            string_offset as usize + string_len as usize + std::mem::size_of::<u32>(),
        );
        self.strings.insert(s.to_owned(), string_offset);
        string_offset
    }

    /// Insert a file into this converter.
    ///
    /// If the file was already present, it is not added again. The returned `u32`
    /// is the file's index in insertion order.
    fn insert_file(
        &mut self,
        path_name: &str,
        directory: Option<&str>,
        comp_dir: Option<&str>,
    ) -> u32 {
        let path_name_offset = self.insert_string(path_name);
        let directory_offset = directory.map_or(u32::MAX, |d| self.insert_string(d));
        let comp_dir_offset = comp_dir.map_or(u32::MAX, |cd| self.insert_string(cd));

        let (file_idx, _) = self.files.insert_full(raw::File {
            path_name_offset,
            directory_offset,
            comp_dir_offset,
        });

        file_idx as u32
    }

    /// Insert a function into this converter.
    ///
    /// If the function was already present, it is not added again. The returned `u32`
    /// is the function's index in insertion order.
    fn insert_function(
        &mut self,
        name: &str,
        comp_dir: Option<&str>,
        entry_pc: u32,
        lang: Language,
    ) -> u32 {
        let name_offset = self.insert_string(name);
        let comp_dir_offset = comp_dir.map_or(u32::MAX, |comp_dir| self.insert_string(comp_dir));
        let lang = lang as u32;
        let (fun_idx, _) = self.functions.insert_full(raw::Function {
            name_offset,
            comp_dir_offset,
            entry_pc,
            lang,
        });
        fun_idx as u32
    }

    // Methods processing symbolic-debuginfo [`ObjectLike`] below:
    // Feel free to move these to a separate file.

    /// This processes the given [`ObjectLike`] object, collecting all its functions and line
    /// information into the converter.
    pub fn process_object<'d, 'o, O>(&mut self, object: &'o O) -> Result<(), SymCacheError>
    where
        O: ObjectLike<'d, 'o>,
        O::Error: std::error::Error + Send + Sync + 'static,
    {
        let session = object
            .debug_session()
            .map_err(|e| SymCacheError::new(SymCacheErrorKind::BadDebugFile, e))?;

        for function in session.functions() {
            let function =
                function.map_err(|e| SymCacheError::new(SymCacheErrorKind::BadDebugFile, e))?;

            self.process_symbolic_function(&function);
        }

        for symbol in object.symbols() {
            self.process_symbolic_symbol(&symbol);
        }

        Ok(())
    }

    pub fn process_symbolic_function(&mut self, function: &Function<'_>) {
        // skip over empty functions
        if function.size == 0 {
            return;
        }

        let comp_dir = std::str::from_utf8(function.compilation_dir).ok();

        let entry_pc = if function.inline {
            u32::MAX
        } else {
            function.address as u32
        };
        let function_idx = self.insert_function(
            function.name.as_str(),
            comp_dir,
            entry_pc,
            function.name.language(),
        );

        for line in &function.lines {
            let path_name = line.file.name_str();
            let file_idx = self.insert_file(&path_name, Some(&line.file.dir_str()), comp_dir);

            let source_location = raw::SourceLocation {
                file_idx,
                line: line.line as u32,
                function_idx,
                inlined_into_idx: u32::MAX,
            };

            match self.ranges.entry(line.address as u32) {
                btree_map::Entry::Vacant(entry) => {
                    if function.inline {
                        // BUG:
                        // the abstraction should have defined this line record inside the caller
                        // function already!
                    }
                    entry.insert(source_location);
                }
                btree_map::Entry::Occupied(mut entry) => {
                    if function.inline {
                        let caller_source_location = entry.get().clone();

                        let mut callee_source_location = source_location;
                        let (inlined_into_idx, _) =
                            self.source_locations.insert_full(caller_source_location);

                        callee_source_location.inlined_into_idx = inlined_into_idx as u32;
                        entry.insert(callee_source_location);
                    } else {
                        // BUG:
                        // the abstraction yields multiple top-level functions for the same
                        // instruction addr
                        entry.insert(source_location);
                    }
                }
            }
        }

        // add the bare minimum of information for the function if there isn't any.
        self.ranges.entry(entry_pc).or_insert(raw::SourceLocation {
            file_idx: u32::MAX,
            line: 0,
            function_idx,
            inlined_into_idx: u32::MAX,
        });

        for inlinee in &function.inlinees {
            self.process_symbolic_function(inlinee);
        }

        let function_end = function.end_address() as u32;
        let last_addr = self.last_addr.get_or_insert(0);
        if function_end > *last_addr {
            *last_addr = function_end;
        }
    }

    pub fn process_symbolic_symbol(&mut self, symbol: &Symbol<'_>) {
        let name = match symbol.name {
            Some(ref name) => name.as_ref(),
            None => return,
        };

        let name_idx = self.insert_string(name);

        match self.ranges.entry(symbol.address as u32) {
            btree_map::Entry::Vacant(entry) => {
                let function = raw::Function {
                    name_offset: name_idx,
                    comp_dir_offset: u32::MAX,
                    entry_pc: symbol.address as u32,
                    lang: u32::MAX,
                };
                let function_idx = self.functions.insert_full(function).0 as u32;

                entry.insert(raw::SourceLocation {
                    file_idx: u32::MAX,
                    line: 0,
                    function_idx,
                    inlined_into_idx: u32::MAX,
                });
            }
            btree_map::Entry::Occupied(entry) => {
                // ASSUMPTION:
                // the `functions` iterator has already filled in this addr via debug session.
                // we could trace the caller hierarchy up to the root, and assert that it is
                // indeed the same function, and maybe update its `entry_pc`, but we don’t do
                // that for now.
                let _function_idx = entry.get().function_idx as usize;
            }
        }

        let last_addr = self.last_addr.get_or_insert(0);
        if symbol.address as u32 >= *last_addr {
            self.last_addr = None;
        }
    }

    // Methods for serializing to a [`Write`] below:
    // Feel free to move these to a separate file.

    /// Serialize the converted data.
    ///
    /// This writes the SymCache binary format into the given [`Write`].
    pub fn serialize<W: Write>(mut self, writer: &mut W) -> std::io::Result<()> {
        let mut writer = WriteWrapper::new(writer);

        // Insert a trailing sentinel source location in case we have a definite end addr
        if let Some(last_addr) = self.last_addr {
            // TODO: to be extra safe, we might check that `last_addr` is indeed larger than
            // the largest range at some point.
            match self.ranges.entry(last_addr) {
                btree_map::Entry::Vacant(entry) => {
                    entry.insert(raw::NO_SOURCE_LOCATION);
                }
                btree_map::Entry::Occupied(_entry) => {
                    // BUG:
                    // the last addr should not map to an already defined range
                }
            }
        }

        let num_files = self.files.len() as u32;
        let num_functions = self.functions.len() as u32;
        let num_source_locations = (self.source_locations.len() + self.ranges.len()) as u32;
        let num_ranges = self.ranges.len() as u32;
        let string_bytes = self.string_bytes.len() as u32;

        let header = raw::Header {
            magic: raw::SYMCACHE_MAGIC,
            version: raw::SYMCACHE_VERSION,

            debug_id: self.debug_id,
            arch: self.arch,

            num_files,
            num_functions,
            num_source_locations,
            num_ranges,
            string_bytes,
            _reserved: [0; 16],
        };

        writer.write(&[header])?;
        writer.align()?;

        for f in self.files {
            writer.write(&[f])?;
        }
        writer.align()?;

        for f in self.functions {
            writer.write(&[f])?;
        }
        writer.align()?;

        for s in self.source_locations {
            writer.write(&[s])?;
        }
        for s in self.ranges.values() {
            writer.write(std::slice::from_ref(s))?;
        }
        writer.align()?;

        for r in self.ranges.keys() {
            writer.write(&[raw::Range(*r)])?;
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
        let len = raw::align_to_eight(self.position);
        self.write(&buf[0..len])
    }
}
