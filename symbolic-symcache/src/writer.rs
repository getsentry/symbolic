//! Defines the [SymCache Converter](`SymCacheConverter`).

use std::collections::btree_map;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;

use indexmap::IndexSet;
use symbolic_common::{Arch, DebugId};
use symbolic_debuginfo::{DebugSession, Function, ObjectLike, Symbol};

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
    /// The set of [`raw::SourceLocation`]s used in this `Converter` that are only used as
    /// "call locations", i.e. which are only referred to from `inlined_into_idx`.
    call_locations: IndexSet<raw::SourceLocation>,
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

            self.process_symbolic_function(&function);
        }

        for symbol in object.symbols() {
            self.process_symbolic_symbol(&symbol);
        }

        Ok(())
    }

    /// Processes an individual [`Function`], adding its line information to the converter.
    pub fn process_symbolic_function(&mut self, function: &Function<'_>) {
        self.process_symbolic_function_recursive(function, &[(0x0, u32::MAX)]);
    }

    /// Processes an individual [`Function`], adding its line information to the converter.
    ///
    /// `call_locations` is a non-empty sorted list of `(address, call_location index)` pairs.
    fn process_symbolic_function_recursive(
        &mut self,
        function: &Function<'_>,
        call_locations: &[(u32, u32)],
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

        // We can divide the instructions in a function into two buckets:
        //  (1) Instructions which are part of an inlined function call, and
        //  (2) instructions which are *not* part of an inlined function call.
        //
        // Our incoming line records cover both (1) and (2) types of instructions.
        //
        // Let's call the address ranges of these instructions (1) inlinee ranges and (2) self ranges.
        //
        // We use the following strategy: For each function, only insert that function's "self ranges"
        // into `self.ranges`. Then recurse into the function's inlinees. Those will insert their
        // own "self ranges". Once the entire tree has been traversed, `self.ranges` will contain
        // entries from all levels.
        //
        // In order to compute this function's "self ranges", we first gather and sort its
        // "inlinee ranges". Later, when we iterate over this function's lines, we will compute the
        // "self ranges" from the gaps between the "inlinee ranges".

        let mut inlinee_ranges = Vec::new();
        for inlinee in &function.inlinees {
            for line in &inlinee.lines {
                let start = line.address as u32;
                let end = (line.address + line.size.unwrap_or(1)) as u32;
                inlinee_ranges.push(start..end);
            }
        }
        inlinee_ranges.sort_unstable_by_key(|range| range.start);

        // Walk three iterators. All of these are already sorted by address.
        let mut line_iter = function.lines.iter();
        let mut call_location_iter = call_locations.iter();
        let mut inline_iter = inlinee_ranges.into_iter();

        // call_locations is non-empty, so the first element always exists.
        let mut current_call_location = call_location_iter.next().unwrap();

        let mut next_call_location = call_location_iter.next();
        let mut next_line = line_iter.next();
        let mut next_inline = inline_iter.next();

        // This will be the list we pass to our inlinees as the call_locations argument.
        // This list is ordered by address by construction.
        let mut callee_call_locations = Vec::new();

        // Iterate over the line records.
        while let Some(line) = next_line.take() {
            let line_range_start = line.address as u32;
            let line_range_end = (line.address + line.size.unwrap_or(1)) as u32;

            // Find the call location for this line.
            while next_call_location.is_some() && next_call_location.unwrap().0 <= line_range_start
            {
                current_call_location = next_call_location.unwrap();
                next_call_location = call_location_iter.next();
            }
            let inlined_into_idx = current_call_location.1;

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
                inlined_into_idx,
            };

            // The current line can be a "self line", or a "call line", or even a mixture.
            //
            // Examples:
            //
            //  a) Just self line:
            //      Line:            |==============|
            //      Inlinee ranges:  (none)
            //
            //      Effect: insert_range
            //
            //  b) Just call line:
            //      Line:            |==============|
            //      Inlinee ranges:  |--------------|
            //
            //      Effect: make_call_location
            //
            //  c) Just call line, for multiple inlined calls:
            //      Line:            |==========================|
            //      Inlinee ranges:  |----------||--------------|
            //
            //      Effect: make_call_location, make_call_location
            //
            //  d) Call line and trailing self line:
            //      Line:            |==================|
            //      Inlinee ranges:  |-----------|
            //
            //      Effect: make_call_location, insert_range
            //
            //  e) Leading self line and also call line:
            //      Line:            |==================|
            //      Inlinee ranges:         |-----------|
            //
            //      Effect: insert_range, make_call_location
            //
            //  f) Interleaving
            //      Line:            |======================================|
            //      Inlinee ranges:         |-----------|    |-------|
            //
            //      Effect: insert_range, make_call_location, insert_range, make_call_location, insert_range
            //
            //  g) Bad debug info
            //      Line:            |=======|
            //      Inlinee ranges:  |-------------|
            //
            //      Effect: make_call_location

            let mut current_address = line_range_start;
            while current_address < line_range_end {
                // Emit our source location at current_address if current_address is not covered by an inlinee.
                if next_inline.is_none() || next_inline.as_ref().unwrap().start > current_address {
                    // "insert_range"
                    self.ranges.insert(current_address, source_location.clone());
                }

                // If there is an inlinee range covered by this line record, turn this line into that
                // call's "call line". Make a `call_location_idx` for it and store it in `callee_call_locations`.
                if next_inline.is_some() && next_inline.as_ref().unwrap().start < line_range_end {
                    let inline_range = next_inline.take().unwrap();

                    // "make_call_location"
                    let (call_location_idx, _) =
                        self.call_locations.insert_full(source_location.clone());
                    callee_call_locations.push((inline_range.start, call_location_idx as u32));

                    // Advance current_address to the end of this inlinee range.
                    current_address = inline_range.end;
                    next_inline = inline_iter.next();
                } else {
                    // No further inlinee ranges are overlapping with this line record. Advance to the
                    // end of the line record.
                    current_address = line_range_end;
                }
            }

            // Advance the line iterator.
            next_line = line_iter.next();

            // Skip any lines that start before current_address.
            // Such lines can exist if the debug information is faulty, or if the compiler created
            // multiple identical small "call line" records instead of one combined record
            // covering the entire inlinee range. We can't have different "call lines" for a single
            // inlinee range anyway, so it's fine to skip these.
            while next_line.is_some()
                && (next_line.as_ref().unwrap().address as u32) < current_address
            {
                next_line = line_iter.next();
            }
        }

        if !function.inline {
            // add the bare minimum of information for the function if there isn't any.
            self.ranges.entry(entry_pc).or_insert(raw::SourceLocation {
                file_idx: u32::MAX,
                line: 0,
                function_idx,
                inlined_into_idx: u32::MAX,
            });
        }

        // We've processed all address ranges which are *not* covered by inlinees.
        // Now it's time to recurse.
        // Process our inlinees.
        if !callee_call_locations.is_empty() {
            for inlinee in &function.inlinees {
                self.process_symbolic_function_recursive(inlinee, &callee_call_locations);
            }
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
        let num_source_locations = (self.call_locations.len() + self.ranges.len()) as u32;
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

        for s in self.call_locations {
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
