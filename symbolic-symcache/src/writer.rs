//! Defines the [SymCache Converter](`SymCacheConverter`).

#[cfg(feature = "il2cpp")]
use std::borrow::Cow;
use std::collections::btree_map;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;

use indexmap::IndexSet;
use symbolic_common::{Arch, DebugId};
use symbolic_debuginfo::{DebugSession, Function, LineInfo, ObjectLike, Symbol};

#[cfg(feature = "il2cpp")]
use symbolic_common::Language;
#[cfg(feature = "il2cpp")]
use symbolic_il2cpp::usym::{UsymSourceRecord, UsymSymbols};

use super::{raw, transform};
use crate::{Error, ErrorKind};

/// The SymCache Converter.
///
/// This can convert data in various source formats to an intermediate representation, which can
/// then be serialized to disk via its [`serialize`](SymCacheConverter::serialize) method.
#[derive(Debug, Default)]
pub struct SymCacheConverter<'a> {
    /// Debug identifier of the object file.
    debug_id: DebugId,
    /// CPU architecture of the object file.
    arch: Arch,

    /// A list of transformers that are used to transform each function / source location.
    transformers: transform::Transformers<'a>,

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

impl<'a> SymCacheConverter<'a> {
    /// Creates a new Converter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a new [`transform::Transformer`] to this [`SymCacheConverter`].
    ///
    /// Every [`transform::Function`] and [`transform::SourceLocation`] will be passed through
    /// this transformer before it is written to the SymCache.
    pub fn add_transformer<T>(&mut self, t: T)
    where
        T: transform::Transformer + 'a,
    {
        self.transformers.0.push(Box::new(t));
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
    fn insert_string(
        string_bytes: &mut Vec<u8>,
        strings: &mut HashMap<String, u32>,
        s: &str,
    ) -> u32 {
        if s.is_empty() {
            return u32::MAX;
        }
        if let Some(&offset) = strings.get(s) {
            return offset;
        }
        let string_offset = string_bytes.len() as u32;
        let string_len = s.len() as u32;
        string_bytes.extend(string_len.to_ne_bytes());
        string_bytes.extend(s.bytes());
        // we should have written exactly `string_len + 4` bytes
        debug_assert_eq!(
            string_bytes.len(),
            string_offset as usize + string_len as usize + std::mem::size_of::<u32>(),
        );
        strings.insert(s.to_owned(), string_offset);
        string_offset
    }

    // Methods processing symbolic-debuginfo [`ObjectLike`] below:
    // Feel free to move these to a separate file.

    /// This processes the given [`ObjectLike`] object, collecting all its functions and line
    /// information into the converter.
    #[tracing::instrument(skip_all, fields(object.debug_id = %object.debug_id().breakpad()))]
    pub fn process_object<'d, 'o, O>(&mut self, object: &'o O) -> Result<(), Error>
    where
        O: ObjectLike<'d, 'o>,
        O::Error: std::error::Error + Send + Sync + 'static,
    {
        let session = object
            .debug_session()
            .map_err(|e| Error::new(ErrorKind::BadDebugFile, e))?;

        self.set_arch(object.arch());
        self.set_debug_id(object.debug_id());

        for function in session.functions() {
            let function = function.map_err(|e| Error::new(ErrorKind::BadDebugFile, e))?;

            self.process_symbolic_function(&function, &[]);
        }

        for symbol in object.symbols() {
            self.process_symbolic_symbol(&symbol);
        }

        Ok(())
    }

    /// Processes an individual [`Function`], adding its line information to the converter.
    pub fn process_symbolic_function(
        &mut self,
        function: &Function<'_>,
        parent_line_records: &[LineInfo],
    ) {
        // skip over empty functions or functions whose address is too large to fit in a u32
        if function.size == 0 || function.address > u32::MAX as u64 {
            return;
        }

        let comp_dir = std::str::from_utf8(function.compilation_dir).ok();

        let entry_pc = if function.inline {
            u32::MAX
        } else {
            function.address as u32
        };

        let function_idx = {
            let language = function.name.language();
            let mut function = transform::Function {
                name: function.name.as_str().into(),
                comp_dir: comp_dir.map(Into::into),
            };
            for transformer in &mut self.transformers.0 {
                function = transformer.transform_function(function);
            }

            let string_bytes = &mut self.string_bytes;
            let strings = &mut self.strings;
            let name_offset = Self::insert_string(string_bytes, strings, &function.name);

            let lang = language as u32;
            let (fun_idx, _) = self.functions.insert_full(raw::Function {
                name_offset,
                _comp_dir_offset: u32::MAX,
                entry_pc,
                lang,
            });
            fun_idx as u32
        };

        let mut parent_line_records = parent_line_records.iter().peekable();
        let mut parent_line = parent_line_records.next();

        for line in &function.lines {
            let mut location = transform::SourceLocation {
                file: transform::File {
                    name: line.file.name_str(),
                    directory: Some(line.file.dir_str()),
                    comp_dir: comp_dir.map(Into::into),
                },
                line: line.line as u32,
            };
            for transformer in &mut self.transformers.0 {
                location = transformer.transform_source_location(location);
            }

            let string_bytes = &mut self.string_bytes;
            let strings = &mut self.strings;
            let name_offset = Self::insert_string(string_bytes, strings, &location.file.name);
            let directory_offset = location
                .file
                .directory
                .map_or(u32::MAX, |d| Self::insert_string(string_bytes, strings, &d));
            let comp_dir_offset = location.file.comp_dir.map_or(u32::MAX, |cd| {
                Self::insert_string(string_bytes, strings, &cd)
            });

            let (file_idx, _) = self.files.insert_full(raw::File {
                name_offset,
                directory_offset,
                comp_dir_offset,
            });

            let source_location = raw::SourceLocation {
                file_idx: file_idx as u32,
                line: location.line,
                function_idx,
                inlined_into_idx: u32::MAX,
            };

            if function.inline {
                while let Some(next_parent) = parent_line_records.peek() {
                    if next_parent.address <= line.address {
                        parent_line = parent_line_records.next();
                    } else {
                        break;
                    }
                }

                // if `parent_line` is nonempty, it is the last parent line record that starts at or before `line`
                let parent_line = match parent_line {
                    Some(parent_line)
                        if parent_line
                            .size
                            .map_or(true, |size| parent_line.address + size > line.address) =>
                    {
                        parent_line
                    }
                    _ => {
                        tracing::warn!(
                            line.address,
                            line.size,
                            ?parent_line,
                            "parent function does not have a covering line record"
                        );
                        continue;
                    }
                };

                match self.ranges.get(&(parent_line.address as u32)) {
                    None => {
                        tracing::warn!(
                            line.address,
                            line.size,
                            parent_line.address,
                            parent_line.size,
                            "parent line record should have been inserted in a previous call"
                        );
                        self.ranges.insert(line.address as u32, source_location);
                    }

                    Some(caller_source_location) => {
                        let caller_source_location = caller_source_location.clone();

                        let mut callee_source_location = source_location;
                        let (inlined_into_idx, _) = self
                            .source_locations
                            .insert_full(caller_source_location.clone());

                        callee_source_location.inlined_into_idx = inlined_into_idx as u32;
                        self.ranges
                            .insert(line.address as u32, callee_source_location);

                        // if `line` ends before `parent_line`, we need to insert another range for the leftover piece.
                        if let Some(size) = line.size {
                            let line_end = line.address + size;
                            if let Some(parent_size) = parent_line.size {
                                let parent_end = parent_line.address + parent_size;

                                if line_end == parent_end {
                                    continue;
                                }
                            }

                            self.ranges.insert(line_end as u32, caller_source_location);
                        }
                    }
                }
            } else {
                match self.ranges.entry(line.address as u32) {
                    btree_map::Entry::Vacant(entry) => {
                        entry.insert(source_location);
                    }
                    btree_map::Entry::Occupied(mut entry) => {
                        // BUG:
                        // the abstraction yields multiple top-level functions for the same
                        // instruction addr
                        tracing::warn!(
                            line.address,
                            line.size,
                            "function is not inlined, but the range is already occupied"
                        );
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
            self.process_symbolic_function(inlinee, &function.lines);
        }

        let function_end = function.end_address() as u32;
        let last_addr = self.last_addr.get_or_insert(0);
        if function_end > *last_addr {
            *last_addr = function_end;
        }
    }

    /// Processes an individual [`Symbol`].
    pub fn process_symbolic_symbol(&mut self, symbol: &Symbol<'_>) {
        let name_idx = {
            let mut function = transform::Function {
                name: match symbol.name {
                    Some(ref name) => name.clone(),
                    None => return,
                },
                comp_dir: None,
            };
            for transformer in &mut self.transformers.0 {
                function = transformer.transform_function(function);
            }

            Self::insert_string(&mut self.string_bytes, &mut self.strings, &function.name)
        };

        match self.ranges.entry(symbol.address as u32) {
            btree_map::Entry::Vacant(entry) => {
                let function = raw::Function {
                    name_offset: name_idx,
                    _comp_dir_offset: u32::MAX,
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

    #[cfg(feature = "il2cpp")]
    /// Processes a set of [`UsymSymbols`], passing all mapped symbols into the converter.
    pub fn process_usym(&mut self, usym: &UsymSymbols) -> Result<(), Error> {
        // Assume records they are sorted by address; There's a test that guarantees this

        let debug_id = usym
            .id()
            .map_err(|e| Error::new(ErrorKind::HeaderTooSmall, e))?;
        self.set_debug_id(debug_id);

        let arch = usym.arch().unwrap_or_default();
        self.set_arch(arch);

        let mapped_records = usym.records().filter_map(|r| match r {
            UsymSourceRecord::Unmapped(_) => None,
            UsymSourceRecord::Mapped(r) => Some(r),
        });

        let mut curr_id: Option<(Cow<'_, str>, Cow<'_, str>)> = None;
        let mut function_idx = 0;
        for record in mapped_records {
            // like process_symbolic_function, skip functions whose address is too large to fit in a
            // u32
            if record.address > u32::MAX as u64 {
                continue;
            }
            let address = record.address as u32;

            // Records that belong to the same function will have the same identifier.
            // Symbols have GUID-like sections to them that might ensure they're unique across
            // files, but we'll just include the file name and paths to be very safe.
            let identifier = Some((record.native_file, record.native_symbol));
            if identifier != curr_id {
                function_idx = {
                    let mut function = transform::Function {
                        name: record.managed_symbol.clone(),
                        comp_dir: None,
                    };
                    for transformer in &mut self.transformers.0 {
                        function = transformer.transform_function(function);
                    }

                    let string_bytes = &mut self.string_bytes;
                    let strings = &mut self.strings;
                    let name_offset = Self::insert_string(string_bytes, strings, &function.name);

                    let (fun_idx, _) = self.functions.insert_full(raw::Function {
                        name_offset,
                        _comp_dir_offset: u32::MAX,
                        entry_pc: address,
                        lang: Language::CSharp as u32,
                    });
                    fun_idx
                };
            }

            let managed_dir = Some(record.managed_file_info.dir_str()).filter(|d| !d.is_empty());
            let mut location = transform::SourceLocation {
                file: transform::File {
                    name: record.managed_file_info.name_str(),
                    directory: managed_dir,
                    comp_dir: None,
                },
                line: record.managed_line,
            };
            for transformer in &mut self.transformers.0 {
                location = transformer.transform_source_location(location);
            }

            let string_bytes = &mut self.string_bytes;
            let strings = &mut self.strings;
            let name_offset = Self::insert_string(string_bytes, strings, &location.file.name);
            let directory_offset = location
                .file
                .directory
                .map_or(u32::MAX, |d| Self::insert_string(string_bytes, strings, &d));

            let (file_idx, _) = self.files.insert_full(raw::File {
                name_offset,
                directory_offset,
                comp_dir_offset: u32::MAX,
            });

            let source_location = raw::SourceLocation {
                file_idx: file_idx as u32,
                line: location.line,
                function_idx: function_idx as u32,
                inlined_into_idx: u32::MAX,
            };

            match self.ranges.entry(address) {
                btree_map::Entry::Vacant(entry) => {
                    entry.insert(source_location);
                }
                btree_map::Entry::Occupied(mut entry) => {
                    // TODO: This exists in native-only mappings, but we don't know yet if it's
                    // possible to generate these types of records in managed code.
                    // println!(
                    //     "Found what's probably an inlined source {}::{}:L{}",
                    //     record.managed_file_info.path_str(),
                    //     record.managed_symbol,
                    //     record.managed_line,
                    // );
                    entry.insert(source_location);
                }
            }
            curr_id = identifier;
        }

        Ok(())
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
            version: crate::SYMCACHE_VERSION,

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
