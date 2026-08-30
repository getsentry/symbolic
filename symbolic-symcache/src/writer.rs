//! Defines the [SymCache Converter](`SymCacheConverter`).

use std::collections::BTreeMap;
use std::collections::btree_map;
use std::io::Write;
use std::rc::Rc;

use indexmap::IndexSet;
use symbolic_common::{Arch, DebugId};
use symbolic_debuginfo::{
    self as di, DebugSession, FileFormat, Function, ObjectLike, Symbol, TypeRef,
};
use watto::{Pod, StringTable, Writer};

use crate::v9;
use crate::{Error, ErrorKind};
use crate::{raw, transform};

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

    /// A flag whether variable information from functions should be embedded into the symcache.
    collect_variables: bool,

    /// A list of transformers that are used to transform each function / source location.
    transformers: transform::Transformers<'a>,

    string_table: StringTable,
    /// The set of all [`v9::raw::File`]s that have been added to this `Converter`.
    files: IndexSet<v9::raw::File>,
    /// The set of all [`v9::raw::Function`]s that have been added to this `Converter`.
    functions: IndexSet<v9::raw::Function>,
    /// The set of [`v9::raw::SourceLocation`]s used in this `Converter` that are only used as
    /// "call locations", i.e. which are only referred to from `inlined_into_idx`.
    call_locations: IndexSet<v9::raw::SourceLocation>,
    /// A map from code ranges to the [`v9::raw::SourceLocation`]s they correspond to.
    ///
    /// Only the starting address of a range is saved, the end address is given implicitly
    /// by the start address of the next range.
    ranges: BTreeMap<u32, v9::raw::SourceLocation>,

    /// The set of variables tied to a function.
    ///
    /// This is keyed with the function id and contains a list of variables associated with that function,
    /// importantly the variables from inlined functions are attributed to their base function.
    function_variables: BTreeMap<u32, Vec<v9::raw::Variable>>,
    /// A list of all variable locations.
    ///
    /// Variable locations are stored in continous chunks per variable.
    variable_locations: Vec<v9::raw::VariableLocationInfo>,
    /// The set of all types referenced from variables.
    types: IndexSet<v9::raw::Type>,

    /// This is highest addr that we know is outside of a valid function.
    /// Functions have an explicit end, while Symbols implicitly extend to infinity.
    /// In case the highest addr belongs to a Symbol, this will be `None` and the SymCache
    /// also extends to infinite, otherwise this is the end of the highest function.
    last_addr: Option<u32>,
}

struct InProgressFunction<'a> {
    function: &'a Function<'a>,
    base_index: u32,
    depth: u16,
    call_locations: Rc<Vec<(u32, u32)>>,
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

    /// Sets whether the symcache should store function variable information.
    pub fn set_collect_variables(&mut self, variables: bool) {
        self.collect_variables = variables;
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

            let function = InProgressFunction {
                function: &function,
                base_index: 0,
                depth: 0,
                call_locations: Rc::new(vec![(0x0, u32::MAX)]),
            };
            let function_stack = vec![function];
            self.process_symbolic_functions(&session, function_stack);
        }

        for symbol in object.symbols() {
            self.process_symbolic_symbol(&symbol);
        }

        self.is_windows_object = false;

        Ok(())
    }

    /// Processes an individual [`Function`], adding its line information to the converter.
    pub fn process_symbolic_function(&mut self, function: &Function<'_>) {
        self.process_symbolic_function_recursive(
            &NoopTypeResolver,
            function,
            &[(0x0, u32::MAX)],
            0,
            0,
        );
    }

    /// Processes an individual [`Function`], adding its line information to the converter.
    ///
    /// `call_locations` is a non-empty sorted list of `(address, call_location index)` pairs.
    fn process_symbolic_function_recursive(
        &mut self,
        tr: &dyn TypeResolver,
        function: &Function<'_>,
        call_locations: &[(u32, u32)],
        base_idx: u32,
        fn_depth: u16,
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

            let function_name = if self.is_windows_object {
                undecorate_win_symbol(&function.name)
            } else {
                &function.name
            };

            let name_offset = self.string_table.insert(function_name) as u32;

            let lang = language as u32;
            let (fun_idx, _) = self.functions.insert_full(v9::raw::Function {
                name_offset,
                _comp_dir_offset: u32::MAX,
                entry_pc,
                lang,
            });
            fun_idx as u32
        };

        let base_idx = match fn_depth {
            // If the depth is zero, the current function is a 'base' function,
            // it has not been inlined into another function.
            0 => function_idx,
            // If the depth is non-zero, it means were are N levels deep in resolving
            // inlinees. So keep the existing base.
            _ => base_idx,
        };
        self.process_symbolic_variables(tr, function, base_idx, fn_depth);

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

        let string_table = &mut self.string_table;

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
                    srcsrv_name: line.file.srcsrv_name_str(),
                    srcsrv_dir: line.file.srcsrv_dir_str(),
                    srcsrv_revision: line.file.srcsrv_revision().map(|s| s.into()),
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
            let srcsrv_name_offset = location
                .file
                .srcsrv_name
                .map_or(u32::MAX, |r| string_table.insert(&r) as u32);
            let srcsrv_dir_offset = location
                .file
                .srcsrv_dir
                .map_or(u32::MAX, |r| string_table.insert(&r) as u32);
            let srcsrv_revision_offset = location
                .file
                .srcsrv_revision
                .map_or(u32::MAX, |r| string_table.insert(&r) as u32);

            let (file_idx, _) = self.files.insert_full(v9::raw::File {
                name_offset,
                directory_offset,
                comp_dir_offset,
                srcsrv_name_offset,
                srcsrv_dir_offset,
                srcsrv_revision_offset,
            });

            let source_location = v9::raw::SourceLocation {
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
            insert_source_location(&mut self.ranges, entry_pc, || v9::raw::SourceLocation {
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
                self.process_symbolic_function_recursive(
                    tr,
                    inlinee,
                    &callee_call_locations,
                    base_idx,
                    fn_depth + 1,
                );
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
            vacant_entry.insert(v9::raw::NO_SOURCE_LOCATION);
        }
    }

    /// Processes an individual [`Function`], adding its line information to the converter.
    ///
    /// `call_locations` is a non-empty sorted list of `(address, call_location index)` pairs.
    fn process_symbolic_functions(
        &mut self,
        tr: &dyn TypeResolver,
        mut function_stack: Vec<InProgressFunction<'_>>,
    ) {
        while let Some(in_progress_function) = function_stack.pop() {
            let function = in_progress_function.function;
            let base_idx = in_progress_function.base_index;
            let fn_depth = in_progress_function.depth;
            let call_locations = in_progress_function.call_locations;

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

                let name_offset = self.string_table.insert(function_name) as u32;

                let lang = language as u32;
                let (fun_idx, _) = self.functions.insert_full(v9::raw::Function {
                    name_offset,
                    _comp_dir_offset: u32::MAX,
                    entry_pc,
                    lang,
                });
                fun_idx as u32
            };

            let base_idx = match fn_depth {
                // If the depth is zero, the current function is a 'base' function,
                // it has not been inlined into another function.
                0 => function_idx,
                // If the depth is non-zero, it means were are N levels deep in resolving
                // inlinees. So keep the existing base.
                _ => base_idx,
            };
            self.process_symbolic_variables(tr, function, base_idx, fn_depth);

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

            let string_table = &mut self.string_table;

            // Iterate over the line records.
            while let Some(line) = next_line.take() {
                let (line_range_start, line_range_end) = line_boundaries(line.address, line.size);

                // Find the call location for this line.
                while next_call_location.is_some()
                    && next_call_location.unwrap().0 <= line_range_start
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
                        srcsrv_name: line.file.srcsrv_name_str(),
                        srcsrv_dir: line.file.srcsrv_dir_str(),
                        srcsrv_revision: line.file.srcsrv_revision().map(|s| s.into()),
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
                let srcsrv_name_offset = location
                    .file
                    .srcsrv_name
                    .map_or(u32::MAX, |r| string_table.insert(&r) as u32);
                let srcsrv_dir_offset = location
                    .file
                    .srcsrv_dir
                    .map_or(u32::MAX, |r| string_table.insert(&r) as u32);
                let srcsrv_revision_offset = location
                    .file
                    .srcsrv_revision
                    .map_or(u32::MAX, |r| string_table.insert(&r) as u32);

                let (file_idx, _) = self.files.insert_full(v9::raw::File {
                    name_offset,
                    directory_offset,
                    comp_dir_offset,
                    srcsrv_name_offset,
                    srcsrv_dir_offset,
                    srcsrv_revision_offset,
                });

                let source_location = v9::raw::SourceLocation {
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
                insert_source_location(&mut self.ranges, entry_pc, || v9::raw::SourceLocation {
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
                let callee_call_locations = Rc::new(callee_call_locations);
                for inlinee in function.inlinees.iter().rev() {
                    let function = InProgressFunction {
                        function: inlinee,
                        base_index: base_idx,
                        depth: fn_depth + 1,
                        call_locations: callee_call_locations.clone(),
                    };
                    function_stack.push(function);
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
                vacant_entry.insert(v9::raw::NO_SOURCE_LOCATION);
            }
        }
    }

    /// Collects all variables from a [`Function`].
    ///
    /// This takes the current `function`, which may have been inlined into an `outer` function.
    /// If the passed function is not inlined, then `base_idx` must point to the index of `function`
    /// and depth is `0`.
    fn process_symbolic_variables(
        &mut self,
        tr: &dyn TypeResolver,
        function: &Function<'_>,
        base_idx: u32,
        fn_depth: u16,
    ) {
        if !self.collect_variables || fn_depth > u8::MAX.into() {
            return;
        }

        for variable in &function.variables {
            let location_idx = self.variable_locations.len();
            for location in &variable.locations {
                // If the end address fits into a `u32` its components also fit.
                if location
                    .address
                    .checked_add(location.size)
                    .is_none_or(|v| v > u32::MAX as u64)
                {
                    continue;
                }

                let raw_loc: v9::raw::location::Enum = match location.location {
                    di::VariableLocation::Register { id } => v9::raw::RegisterLocation {
                        id,
                        _reserved: [0; _],
                    }
                    .into(),
                    di::VariableLocation::FrameOffset { offset } => v9::raw::FrameOffsetLocation {
                        offset: offset as i32,
                    }
                    .into(),
                };
                let (kind, data) = raw_loc.into_impl();

                self.variable_locations.push(v9::raw::VariableLocationInfo {
                    start: location.address as u32,
                    size: location.size as u32,
                    location: data,
                    kind,
                    _reserved: [0; _],
                });
            }

            let num_locations = self.variable_locations.len() - location_idx;
            if num_locations == 0 {
                // No locations, no need to keep the variable around.
                continue;
            }

            // Locations must already be sorted, this is an invariant on the `Function`.
            debug_assert!(self.variable_locations[location_idx..].is_sorted_by_key(|v| v.start));

            let type_idx = match &variable.ty {
                Some(ty) => self.process_symbolic_type(tr, ty, 0),
                None => u32::MAX,
            };

            let name_offset = self.string_table.insert(&variable.name) as u32;

            self.function_variables
                .entry(base_idx)
                .or_default()
                .push(v9::raw::Variable {
                    name_offset,
                    type_idx,
                    location_idx: location_idx as u32,
                    num_locations: num_locations as u32,
                    depth: fn_depth as u8,
                    kind: v9::convert::variable_kind_to_u8(variable.kind),
                    _reserved: [0; _],
                });
        }
    }

    fn process_symbolic_type(&mut self, tr: &dyn TypeResolver, ty: &TypeRef, depth: usize) -> u32 {
        // This really is just a preliminary limit. In the future we need to currently
        // be able to handle recursive types such as `Foo(Option<Box<Foo>>)`. For the moment
        // there is only support for primitive types and pointers which shouldn't be recursive.
        const MAX_DEPTH: usize = 5;

        if depth >= MAX_DEPTH {
            return u32::MAX;
        }

        let Some(ty) = tr.lookup_type(ty) else {
            return u32::MAX;
        };

        let ty = match ty {
            di::Type::Primitive(ty) => v9::raw::PrimitiveType {
                name_offset: ty
                    .name
                    .map_or(u32::MAX, |n| self.string_table.insert(&n) as u32),
                size: size(ty.size),
                encoding: v9::convert::primitive_type_encoding_to_u8(ty.encoding),
                _reserved: [0; _],
            }
            .into(),
            di::Type::Pointer(ty) => v9::raw::PointerType {
                pointee_idx: self.process_symbolic_type(tr, &ty.pointee, depth + 1),
                size: size(ty.size),
                _reserved: [0; _],
            }
            .into(),
        };

        self.types.insert_full(ty).0 as u32
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
            let function = v9::raw::Function {
                name_offset: name_idx,
                _comp_dir_offset: u32::MAX,
                entry_pc: symbol.address as u32,
                lang: u32::MAX,
            };
            let function_idx = self.functions.insert_full(function).0 as u32;

            v9::raw::SourceLocation {
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
                vacant_entry.insert(v9::raw::NO_SOURCE_LOCATION);
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
                    entry.insert(v9::raw::NO_SOURCE_LOCATION);
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

        // Write VersionInfo preamble
        let version_info = raw::VersionInfo {
            magic: crate::raw::SYMCACHE_MAGIC,
            version: crate::SYMCACHE_VERSION,
        };
        writer.write_all(version_info.as_bytes())?;

        // Write v9 Header
        let header = v9::raw::Header {
            debug_id: self.debug_id,
            arch: self.arch as u32,
            num_files,
            num_functions,
            num_source_locations,
            num_ranges,
            string_bytes: string_bytes.len() as u32,
            variable_header: match self.collect_variables {
                true => v9::raw::VARIABLE_HEADER_VERSION,
                false => 0,
            },
            _reserved: [0; 14],
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

        if !self.collect_variables {
            return Ok(());
        }

        let num_variables: u32 = self
            .function_variables
            .values()
            .map(|v| v.len() as u32)
            .sum();

        writer.align_to(8)?;
        writer.write_all(
            v9::raw::VariableHeader {
                num_variables,
                num_variable_locations: self.variable_locations.len() as u32,
                num_types: self.types.len() as u32,
            }
            .as_bytes(),
        )?;

        let function_variables = (0..header.num_functions).scan(0u32, |next_idx, function_idx| {
            let variables = self
                .function_variables
                .get_mut(&function_idx)
                .map(|v| v.as_mut_slice());

            let Some(variables) = variables.filter(|v| !v.is_empty()) else {
                return Some(v9::raw::NO_VARIABLES);
            };

            // Sort by depth, to allow a more efficient lookup/scan.
            variables.sort_unstable_by_key(|v| v.depth);

            let res = v9::raw::FunctionVariables {
                variable_idx: *next_idx,
                num_variables: variables.len() as u32,
            };
            *next_idx += res.num_variables;
            Some(res)
        });
        writer.align_to(8)?;
        for fv in function_variables {
            writer.write_all(fv.as_bytes())?;
        }

        writer.align_to(8)?;
        for vars in self.function_variables.values() {
            for v in vars {
                writer.write_all(v.as_bytes())?;
            }
        }

        writer.align_to(8)?;
        for l in &self.variable_locations {
            writer.write_all(l.as_bytes())?;
        }

        writer.align_to(8)?;
        for t in self.types {
            writer.write_all(t.as_bytes())?;
        }

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
    source_locations: &mut BTreeMap<K, v9::raw::SourceLocation>,
    key: K,
    val: F,
) where
    K: Ord,
    F: FnOnce() -> v9::raw::SourceLocation,
{
    if source_locations
        .get(&key)
        .is_none_or(|sl| *sl == v9::raw::NO_SOURCE_LOCATION)
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

fn size(s: di::TypeSize) -> u32 {
    match s {
        di::TypeSize::Bytes(bytes) => bytes as u32,
    }
}

/// A tiny helper which only exposes type information from a debug session.
///
/// This makes passing around a debug session much more convenient and also allows
/// code to deal with no debug session [`NoopTypeResolver`].
trait TypeResolver {
    fn lookup_type(&self, ty: &TypeRef) -> Option<di::Type<'_>>;
}

impl<S> TypeResolver for S
where
    S: for<'session> DebugSession<'session>,
{
    fn lookup_type(&self, ty: &TypeRef) -> Option<di::Type<'_>> {
        DebugSession::lookup_type(self, ty).ok().flatten()
    }
}

/// A [`TypeResolver`] which never resolves a type.
struct NoopTypeResolver;

impl TypeResolver for NoopTypeResolver {
    fn lookup_type(&self, _ty: &TypeRef) -> Option<di::Type<'_>> {
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
