//! Defines the [SymCache Converter](`SymCacheConverter`).

use std::collections::btree_map;
use std::collections::BTreeMap;
use std::io::Write;

use indexmap::IndexSet;
use symbolic_common::{Arch, DebugId};
use symbolic_debuginfo::{DebugSession, FileFormat, Function, ObjectLike, Symbol};
use watto::{Pod, StringTable, Writer};

use super::{raw, transform};
use crate::raw::NO_SOURCE_LOCATION;
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

    /// A flag that indicates that we are currently processing a Windows object, which
    /// will inform us if we should undecorate function names.
    is_windows_object: bool,

    /// A list of transformers that are used to transform each function / source location.
    transformers: transform::Transformers<'a>,

    string_table: StringTable,
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

        self.is_windows_object = matches!(object.file_format(), FileFormat::Pe | FileFormat::Pdb);

        for function in session.functions() {
            let function = function.map_err(|e| Error::new(ErrorKind::BadDebugFile, e))?;

            self.process_symbolic_function(&function);
        }

        for symbol in object.symbols() {
            self.process_symbolic_symbol(&symbol);
        }

        self.is_windows_object = false;

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
        let string_table = &mut self.string_table;
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

            let function_name = if self.is_windows_object {
                undecorate_win_symbol(&function.name)
            } else {
                &function.name
            };

            let name_offset = string_table.insert(function_name) as u32;

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
                let (start, end) = line_boundaries(line.address, line.size);
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
            let (line_range_start, line_range_end) = line_boundaries(line.address, line.size);

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

            let name_offset = string_table.insert(&location.file.name) as u32;
            let directory_offset = location
                .file
                .directory
                .map_or(u32::MAX, |d| string_table.insert(&d) as u32);
            let comp_dir_offset = location
                .file
                .comp_dir
                .map_or(u32::MAX, |cd| string_table.insert(&cd) as u32);

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
                if next_inline
                    .as_ref()
                    .is_none_or(|next| next.start > current_address)
                {
                    // "insert_range"
                    self.ranges.insert(current_address, source_location.clone());
                }

                // If there is an inlinee range covered by this line record, turn this line into that
                // call's "call line". Make a `call_location_idx` for it and store it in `callee_call_locations`.
                if let Some(inline_range) =
                    take_if(&mut next_inline, |next| next.start < line_range_end)
                {
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
            while next_line
                .as_ref()
                .is_some_and(|next| (next.address as u32) < current_address)
            {
                next_line = line_iter.next();
            }
        }

        if !function.inline {
            // add the bare minimum of information for the function if there isn't any.
            insert_source_location(&mut self.ranges, entry_pc, || raw::SourceLocation {
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

        // Insert an explicit "empty" mapping for the end of the function.
        // This is to ensure that addresses that fall "between" functions don't get
        // erroneously mapped to the previous function.
        //
        // We only do this if there is no previous mapping for the end address—we don't
        // want to overwrite valid mappings.
        //
        // If the next function starts right at this function's end, that's no trouble,
        // it will just overwrite this mapping with one of its ranges.
        if let btree_map::Entry::Vacant(vacant_entry) = self.ranges.entry(function_end) {
            vacant_entry.insert(NO_SOURCE_LOCATION);
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

            let function_name = if self.is_windows_object {
                undecorate_win_symbol(&function.name)
            } else {
                &function.name
            };

            self.string_table.insert(function_name) as u32
        };

        // Insert a source location for the symbol, overwriting `NO_SOURCE_LOCATION` sentinel
        // values but not actual source locations coming from e.g. functions.
        insert_source_location(&mut self.ranges, symbol.address as u32, || {
            let function = raw::Function {
                name_offset: name_idx,
                _comp_dir_offset: u32::MAX,
                entry_pc: symbol.address as u32,
                lang: u32::MAX,
            };
            let function_idx = self.functions.insert_full(function).0 as u32;

            raw::SourceLocation {
                file_idx: u32::MAX,
                line: 0,
                function_idx,
                inlined_into_idx: u32::MAX,
            }
        });

        let last_addr = self.last_addr.get_or_insert(0);
        if symbol.address as u32 >= *last_addr {
            self.last_addr = None;
        }

        // Insert an explicit "empty" mapping for the end of the symbol.
        // This is to ensure that addresses that fall "between" symbols don't get
        // erroneously mapped to the previous symbol.
        //
        // We only do this if there is no previous mapping for the end address—we don't
        // want to overwrite valid mappings.
        //
        // If the next symbol starts right at this symbols's end, that's no trouble,
        // it will just overwrite this mapping.
        if symbol.size > 0 {
            let end_address = (symbol.address + symbol.size) as u32;
            if let btree_map::Entry::Vacant(vacant_entry) = self.ranges.entry(end_address) {
                vacant_entry.insert(NO_SOURCE_LOCATION);
            }
        }
    }

    // Methods for serializing to a [`Write`] below:
    // Feel free to move these to a separate file.

    /// Serialize the converted data.
    ///
    /// This writes the SymCache binary format into the given [`Write`].
    pub fn serialize<W: Write>(mut self, writer: &mut W) -> std::io::Result<()> {
        let mut writer = Writer::new(writer);

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
        let string_bytes = self.string_table.into_bytes();

        let header = raw::Header {
            magic: raw::SYMCACHE_MAGIC,
            version: crate::SYMCACHE_VERSION,

            debug_id: self.debug_id,
            arch: self.arch,

            num_files,
            num_functions,
            num_source_locations,
            num_ranges,
            string_bytes: string_bytes.len() as u32,
            _reserved: [0; 16],
        };

        writer.write_all(header.as_bytes())?;
        writer.align_to(8)?;

        for f in self.files {
            writer.write_all(f.as_bytes())?;
        }
        writer.align_to(8)?;

        for f in self.functions {
            writer.write_all(f.as_bytes())?;
        }
        writer.align_to(8)?;

        for s in self.call_locations {
            writer.write_all(s.as_bytes())?;
        }
        for s in self.ranges.values() {
            writer.write_all(s.as_bytes())?;
        }
        writer.align_to(8)?;

        for r in self.ranges.keys() {
            writer.write_all(r.as_bytes())?;
        }
        writer.align_to(8)?;

        writer.write_all(&string_bytes)?;

        Ok(())
    }
}

/// Inserts a source location into a map, but only if there either isn't already
/// a value for the provided key or the value is the `NO_SOURCE_LOCATION` sentinel.
///
/// This is useful because a `NO_SOURCE_LOCATION` value may be inserted at an address to explicitly
/// mark the end of a function or symbol. If later there is a function, symbol, or range
/// starting at that same address, we want to evict that sentinel, but we wouldn't want to
/// evict source locations carrying actual information.
fn insert_source_location<K, F>(
    source_locations: &mut BTreeMap<K, raw::SourceLocation>,
    key: K,
    val: F,
) where
    K: Ord,
    F: FnOnce() -> raw::SourceLocation,
{
    if source_locations
        .get(&key)
        .is_none_or(|sl| *sl == NO_SOURCE_LOCATION)
    {
        source_locations.insert(key, val());
    }
}

/// Undecorates a Windows C-decorated symbol name.
///
/// The decoration rules are explained here:
/// <https://docs.microsoft.com/en-us/cpp/build/reference/decorated-names?view=vs-2019>
///
/// - __cdecl Leading underscore (_)
/// - __stdcall Leading underscore (_) and a trailing at sign (@) followed by the number of bytes in the parameter list in decimal
/// - __fastcall Leading and trailing at signs (@) followed by a decimal number representing the number of bytes in the parameter list
/// - __vectorcall Two trailing at signs (@@) followed by a decimal number of bytes in the parameter list
/// > In a 64-bit environment, C or extern "C" functions are only decorated when using the __vectorcall calling convention."
///
/// This code is adapted from `dump_syms`:
/// See <https://github.com/mozilla/dump_syms/blob/325cf2c61b2cacc55a7f1af74081b57237c7f9de/src/symbol.rs#L169-L216>
fn undecorate_win_symbol(name: &str) -> &str {
    if name.starts_with('?') || name.contains([':', '(', '<']) {
        return name;
    }

    // Parse __vectorcall.
    if let Some((name, param_size)) = name.rsplit_once("@@") {
        if param_size.parse::<u32>().is_ok() {
            return name;
        }
    }

    // Parse the other three.
    if !name.is_empty() {
        if let ("@" | "_", rest) = name.split_at(1) {
            if let Some((name, param_size)) = rest.rsplit_once('@') {
                if param_size.parse::<u32>().is_ok() {
                    // __stdcall or __fastcall
                    return name;
                }
            }
            if let Some(name) = name.strip_prefix('_') {
                // __cdecl
                return name;
            }
        }
    }

    name
}

/// Returns the start and end address for a line record, clamped to `u32`.
fn line_boundaries(address: u64, size: Option<u64>) -> (u32, u32) {
    let start = address.try_into().unwrap_or(u32::MAX);
    let end = start.saturating_add(size.unwrap_or(1).try_into().unwrap_or(u32::MAX));
    (start, end)
}

fn take_if<T>(opt: &mut Option<T>, predicate: impl FnOnce(&mut T) -> bool) -> Option<T> {
    if opt.as_mut().is_some_and(predicate) {
        opt.take()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests that computing a range with a large size naively
    /// results in an empty range, but using `line_boundaries`
    /// doesn't.
    #[test]
    fn test_large_range() {
        // Line record values from an actual example
        let address = 0x11d255;
        let size = 0xffee9d55;

        let naive_range = {
            let start = address as u32;
            let end = (address + size) as u32;
            start..end
        };

        assert!(naive_range.is_empty());

        let range = {
            let (start, end) = line_boundaries(address, Some(size));
            start..end
        };

        assert!(!range.is_empty());
    }
}
