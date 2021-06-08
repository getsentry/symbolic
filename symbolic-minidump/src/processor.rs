//! Minidump processing facilities.
//!
//! This crate exposes rust bindings to the Breakpad processor for minidumps. The root type is
//! [`ProcessState`], which contains the high-level API to open a Minidump and extract most of the
//! information that is stored inside.
//!
//! For more information on the internals of the Breakpad processor, refer to the [official docs].
//!
//! [official docs]: https://chromium.googlesource.com/breakpad/breakpad/+/master/docs/processor_design.md
//! [`ProcessState`]: struct.ProcessState.html

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CStr, CString};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::Range;
use std::os::raw::{c_char, c_void};
use std::str::FromStr;
use std::{fmt, ptr, slice, str};

use lazy_static::lazy_static;
use regex::Regex;

use symbolic_common::{Arch, ByteView, CpuFamily, DebugId, ParseDebugIdError, Uuid};
use symbolic_debuginfo::breakpad::{
    BreakpadStackCfiDeltaRecords, BreakpadStackCfiRecords, BreakpadStackWinRecord,
    BreakpadStackWinRecordType, BreakpadStackWinRecords,
};
use symbolic_symcache::SymCache;
use symbolic_unwind::evaluator::{Constant, Identifier};
use symbolic_unwind::{MemoryRegion, RuntimeEndian};

use crate::cfi::CfiCache;
use crate::utils;

type DwarfUnwindRulesMap<'a> = BTreeMap<&'a CodeModuleId, DwarfUnwindRules<'a>>;
type WinUnwindRulesMap<'a> = BTreeMap<&'a CodeModuleId, WinUnwindRules<'a>>;
type SymCacheMap<'a> = BTreeMap<&'a CodeModuleId, SymCache<'a>>;
type Evaluator<'a, A> = symbolic_unwind::evaluator::Evaluator<'a, A, RuntimeEndian>;

lazy_static! {
    static ref LINUX_BUILD_RE: Regex =
        Regex::new(r"^Linux ([^ ]+) (.*) \w+(?: GNU/Linux)?$").unwrap();
}

extern "C" {
    fn code_module_base_address(module: *const CodeModule) -> u64;
    fn code_module_size(module: *const CodeModule) -> u64;
    fn code_module_code_file(module: *const CodeModule) -> *mut c_char;
    fn code_module_code_identifier(module: *const CodeModule) -> *mut c_char;
    fn code_module_debug_file(module: *const CodeModule) -> *mut c_char;
    fn code_module_debug_identifier(module: *const CodeModule) -> *mut c_char;
    fn code_modules_delete(state: *mut *const CodeModule);

    fn stack_frame_return_address(frame: *const StackFrame) -> u64;
    fn stack_frame_instruction(frame: *const StackFrame) -> u64;
    fn stack_frame_module(frame: *const StackFrame) -> *const CodeModule;
    fn stack_frame_trust(frame: *const StackFrame) -> FrameTrust;
    fn stack_frame_registers(
        frame: *const StackFrame,
        family: u32,
        size_out: *mut usize,
    ) -> *mut IRegVal;
    fn regval_delete(state: *mut IRegVal);

    fn call_stack_thread_id(stack: *const CallStack) -> u32;
    fn call_stack_frames(stack: *const CallStack, size_out: *mut usize)
        -> *const *const StackFrame;

    fn system_info_os_name(info: *const SystemInfo) -> *mut c_char;
    fn system_info_os_version(info: *const SystemInfo) -> *mut c_char;
    fn system_info_cpu_family(info: *const SystemInfo) -> *mut c_char;
    fn system_info_cpu_info(info: *const SystemInfo) -> *mut c_char;
    fn system_info_cpu_count(info: *const SystemInfo) -> u32;

    fn process_minidump_breakpad(
        buffer: *const c_char,
        buffer_size: usize,
        symbols: *const SymbolEntry,
        symbol_count: usize,
        result: *mut ProcessResult,
    ) -> *mut IProcessState;
    fn process_minidump_symbolic(
        buffer: *const c_char,
        buffer_size: usize,
        resolver: *mut c_void,
        result: *mut ProcessResult,
    ) -> *mut IProcessState;
    fn process_state_delete(state: *mut IProcessState);
    fn process_state_threads(
        state: *const IProcessState,
        size_out: *mut usize,
    ) -> *const *const CallStack;
    fn process_state_requesting_thread(state: *const IProcessState) -> i32;
    fn process_state_timestamp(state: *const IProcessState) -> u64;
    fn process_state_crashed(state: *const IProcessState) -> bool;
    fn process_state_crash_address(state: *const IProcessState) -> u64;
    fn process_state_crash_reason(state: *const IProcessState) -> *mut c_char;
    fn process_state_assertion(state: *const IProcessState) -> *mut c_char;
    fn process_state_system_info(state: *const IProcessState) -> *mut SystemInfo;
    fn process_state_modules(
        state: *const IProcessState,
        size_out: *mut usize,
    ) -> *mut *const CodeModule;
}

/// Auxiliary iterator that yields pairs of addresses and rule strings
/// of [`BreakpadStackCfiDeltaRecord`](symbolic_debuginfo::breakpad::BreakpadStackCfiDeltaRecord)s.
#[derive(Clone, Debug)]
struct DeltaRules<'a> {
    inner: BreakpadStackCfiDeltaRecords<'a>,
}

impl<'a> Iterator for DeltaRules<'a> {
    type Item = (u64, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .find_map(Result::ok)
            .map(|record| (record.address, record.rules))
    }
}

/// A structure containing a set of disjoint ranges with attached contents.
#[derive(Clone, Debug)]
pub struct RangeMap<A, E> {
    inner: Vec<(Range<A>, E)>,
}

impl<A: Ord + Copy, E> RangeMap<A, E> {
    /// Insert a range into the map.
    ///
    /// The range must be disjoint from all ranges that are already present.
    /// Returns true if the insertion was successful.
    pub fn insert(&mut self, range: Range<A>, contents: E) -> bool {
        if let Some(i) = self.free_slot(&range) {
            self.inner.insert(i, (range, contents));
            true
        } else {
            false
        }
    }

    /// Returns the position in the inner vector where the given range could be inserted, if that is possible.
    fn free_slot(&self, range: &Range<A>) -> Option<usize> {
        let index = match self.inner.binary_search_by_key(&range.end, |r| r.0.end) {
            Ok(_) => return None,
            Err(index) => index,
        };

        if index > 0 {
            let before = &self.inner[index - 1];
            if before.0.end > range.start {
                return None;
            }
        }

        match self.inner.get(index) {
            Some(after) if after.0.start < range.end => None,
            _ => Some(index),
        }
    }

    /// Retrieves the range covering the given address and the associated contents.
    pub fn get(&self, address: A) -> Option<&(Range<A>, E)> {
        let entry = match self
            .inner
            .binary_search_by_key(&address, |range| range.0.end)
        {
            // This means inner(index).end == address => address might be covered by the next one
            Ok(index) => self.inner.get(index + 1)?,
            // This means that inner(index).end > address => this could be the one
            Err(index) => self.inner.get(index)?,
        };

        (entry.0.start <= address).then(|| entry)
    }

    /// Retrieves the range covering the given address, allowing mutation.
    pub fn get_mut(&mut self, address: A) -> Option<&mut (Range<A>, E)> {
        let entry = match self
            .inner
            .binary_search_by_key(&address, |range| range.0.end)
        {
            // This means inner(index).end == address => address might be covered by the next one
            Ok(index) => self.inner.get_mut(index + 1)?,
            // This means that inner(index).end > address => this could be the one
            Err(index) => self.inner.get_mut(index)?,
        };

        (entry.0.start <= address).then(|| entry)
    }

    /// Retrieves the contents associated with the given address.
    pub fn get_contents(&self, address: A) -> Option<&E> {
        self.get(address).map(|(_, contents)| contents)
    }

    /// Retrieves the contents associated with the given address, allowing mutation.
    pub fn get_contents_mut(&mut self, address: A) -> Option<&mut E> {
        self.get_mut(address).map(|(_, contents)| contents)
    }

    /// Returns true if the given address is covered by some range in the map.
    pub fn contains(&self, address: A) -> bool {
        self.get(address).is_some()
    }
}

impl<A, E> Default for RangeMap<A, E> {
    fn default() -> Self {
        Self { inner: Vec::new() }
    }
}

type NestedRangeMapEntry<A, E> = (Range<A>, E, Box<NestedRangeMap<A, E>>);

/// A structure representing a tree of disjoint ranges with associated contents.
#[derive(Debug)]
pub struct NestedRangeMap<A, E> {
    inner: Vec<NestedRangeMapEntry<A, E>>,
}

impl<A: Ord + Copy + fmt::Debug, E> NestedRangeMap<A, E> {
    fn from_vec_unchecked(inner: Vec<NestedRangeMapEntry<A, E>>) -> Self {
        Self { inner }
    }

    /// Insert a range into the map.

    /// The insertion is valid if the new range does not
    /// overlap nontrivially with any existing ranges
    /// and is not equal to an existing range.
    /// Returns true if the insertion was successful.
    pub fn insert(&mut self, range: Range<A>, contents: E) -> bool {
        if self.inner.is_empty() {
            self.inner
                .push((range, contents, Box::new(NestedRangeMap::default())));
            return true;
        }

        let start_idx = self
            .inner
            .binary_search_by_key(&range.start, |entry| entry.0.start);

        let end_idx = self
            .inner
            .binary_search_by_key(&range.end, |entry| entry.0.end);

        match (start_idx, end_idx) {
            (Ok(i), Ok(j)) => {
                // Both the start and end of `range` line up with existing ranges
                match i.cmp(&j) {
                    Ordering::Equal => {
                        // [ range i)
                        // [ range  )
                        false
                    }
                    Ordering::Less => {
                        // [ range i ) … [range j )
                        // [        range         )
                        let inner_new = self.inner.drain(i..=j).collect();
                        let map_new = NestedRangeMap::from_vec_unchecked(inner_new);
                        self.inner.insert(i, (range, contents, Box::new(map_new)));
                        true
                    }
                    Ordering::Greater => {
                        // i > j should never happen.
                        false
                    }
                }
            }
            (Err(i), Err(j)) => {
                // Neither start nor end of `range` line up with existing ranges.
                if i <= j {
                    if let Some(before) = i.checked_sub(1).and_then(|k| self.inner.get(k)) {
                        if before.0.end > range.start {
                            // [ before )
                            //     [ range )
                            return false;
                        }
                    }

                    if let Some(after) = self.inner.get(j) {
                        if after.0.start < range.end {
                            //       [ after )
                            //  [ range )
                            return false;
                        }
                    }

                    //   [ range i ) … [ range j-1 )
                    // [           range             )
                    let inner_new = self.inner.drain(i..j).collect();
                    let map_new = NestedRangeMap::from_vec_unchecked(inner_new);
                    self.inner.insert(i, (range, contents, Box::new(map_new)));
                    true
                } else if i == j + 1 {
                    // [  range j  )
                    //   [ range )
                    self.inner[j].2.insert(range, contents)
                } else {
                    // i > j + 1, this should never happen.
                    false
                }
            }

            (Ok(i), Err(j)) => {
                // The start of `range` lines up with an existing range
                match i.cmp(&j) {
                    Ordering::Equal => {
                        // [  range i )
                        // [ range )
                        self.inner[i].2.insert(range, contents)
                    }
                    Ordering::Less => {
                        if let Some(after) = self.inner.get(j) {
                            if after.0.start < range.end {
                                //  [ range i )  …  [ after )
                                //  [        range       )
                                return false;
                            }
                        }

                        // [ range i ) … [ range j-1)
                        // [           range            )
                        let inner_new = self.inner.drain(i..j).collect();
                        let map_new = NestedRangeMap::from_vec_unchecked(inner_new);
                        self.inner.insert(i, (range, contents, Box::new(map_new)));
                        true
                    }
                    Ordering::Greater => {
                        // i > j, this should never happen.
                        false
                    }
                }
            }

            (Err(i), Ok(j)) => {
                // The end of `range` lines up with an existing range
                if i == j + 1 {
                    // [  range j  )
                    //   [  range  )
                    self.inner[j].2.insert(range, contents)
                } else if i <= j {
                    if let Some(before) = i.checked_sub(1).and_then(|k| self.inner.get(k)) {
                        if before.0.end > range.start {
                            // [ before ) … [ range j)
                            //     [      range      )
                            return false;
                        }
                    }

                    //   [ range i ) … [ range j )
                    // [          range          )
                    let inner_new = self.inner.drain(i..=j).collect();
                    let map_new = NestedRangeMap::from_vec_unchecked(inner_new);
                    self.inner.insert(i, (range, contents, Box::new(map_new)));
                    true
                } else {
                    // i > j + 1, this should never happen
                    false
                }
            }
        }
    }

    /// Retrieves the *most specific* contents associated with the given address, that is,
    /// those associated with the smallest range that covers the address.
    pub fn get_contents(&self, address: A) -> Option<&E> {
        let (range, entry, sub_map) = match self
            .inner
            .binary_search_by_key(&address, |range| range.0.end)
        {
            // This means inner(index).end == address => address might be covered by the next one
            Ok(index) => self.inner.get(index + 1)?,
            // This means that inner(index).end > address => this could be the one
            Err(index) => self.inner.get(index)?,
        };

        (range.start <= address).then(|| sub_map.get_contents(address).unwrap_or(entry))
    }

    /// Returns true if the given address is covered by some range in the map.
    pub fn contains(&self, address: A) -> bool {
        self.get_contents(address).is_some()
    }
}

impl<A, E> Default for NestedRangeMap<A, E> {
    fn default() -> Self {
        Self {
            inner: Vec::default(),
        }
    }
}

/// Struct containing Dwarf unwind information for a module.
struct DwarfUnwindRules<'a> {
    /// Unwind rules that have already been read and sorted.
    cache: RangeMap<u64, (&'a str, DeltaRules<'a>)>,

    /// An iterator over Breakpad stack records that have not yet been read.
    records_iter: BreakpadStackCfiRecords<'a>,
}

impl<'a> DwarfUnwindRules<'a> {
    /// Creates a new `DwarfUnwindRules` from the given records iterator.
    fn new(records_iter: BreakpadStackCfiRecords<'a>) -> Self {
        Self {
            cache: RangeMap::default(),
            records_iter,
        }
    }

    /// Retrieves the unwind rules associated with the given address.
    ///
    /// If there are no rules for the address in the cache,
    /// the inner iterator is consumed until the rules are found.
    /// All rules consumed on the way are added to the cache.
    fn get(&mut self, address: u64) -> Option<Vec<&'a str>> {
        let DwarfUnwindRules {
            cache,
            records_iter,
        } = self;
        if !cache.contains(address) {
            for cfi_record in records_iter.filter_map(Result::ok) {
                let start = cfi_record.start;
                let end = start + cfi_record.size;
                let deltas = DeltaRules {
                    inner: cfi_record.deltas().clone(),
                };
                cache.insert(start..end, (cfi_record.init_rules, deltas));
                if start <= address && address < end {
                    break;
                }
            }
        }

        let (init, deltas) = cache.get_contents_mut(address)?;
        let mut result = vec![*init];
        result.extend(
            deltas
                .clone()
                .take_while(|(a, _)| *a <= address)
                .map(|pair| pair.1),
        );

        Some(result)
    }
}

/// Struct containing windows unwind information for a module.
///
/// This maintains two separate caches for `STACK WIN` records
/// of types `FPO` and `FrameData`. If both exist for a given address,
/// the `FrameData` record is preferred.
struct WinUnwindRules<'a> {
    /// `FrameData` records that have already been read and sorted.
    cache_frame_data: NestedRangeMap<u32, BreakpadStackWinRecord<'a>>,

    /// Other stack win records that have already been read and sorted.
    cache_other: NestedRangeMap<u32, BreakpadStackWinRecord<'a>>,

    /// An iterator over Breakpad stack records that have not yet been read.
    records_iter: BreakpadStackWinRecords<'a>,
}

impl<'a> WinUnwindRules<'a> {
    /// Creates a new `WinUnwindRules` from the given records iterator.
    fn new(records_iter: BreakpadStackWinRecords<'a>) -> Self {
        Self {
            cache_frame_data: NestedRangeMap::default(),
            cache_other: NestedRangeMap::default(),
            records_iter,
        }
    }

    /// Retrieves the STACK WIN record associated with the given address, preferring `FrameData` records over
    /// `FPO` records.
    ///
    /// If there are no records for the address in either cache,
    /// the inner iterator is consumed until a record is found.
    /// All records consumed on the way are added to the respective cache.
    fn get(&mut self, address: u32) -> Option<&BreakpadStackWinRecord<'a>> {
        let WinUnwindRules {
            cache_frame_data,
            cache_other,
            records_iter,
        } = self;
        if !cache_frame_data.contains(address) && !cache_other.contains(address) {
            for win_record in records_iter.filter_map(Result::ok) {
                let start = win_record.code_start;
                let end = start + win_record.code_size;
                match win_record.ty {
                    BreakpadStackWinRecordType::FrameData => {
                        cache_frame_data.insert(start..end, win_record);
                    }
                    _ => {
                        cache_other.insert(start..end, win_record);
                    }
                }
            }
        }

        cache_frame_data
            .get_contents(address)
            .or_else(move || cache_other.get_contents(address))
    }
}

/// Struct that bundles the information an evaluator needs to compute caller registers from callee registers,
/// i.e., a vector of rules and the endianness with which to interpret memory.
#[derive(Debug)]
struct CfiFrameInfo<'a> {
    endian: RuntimeEndian,
    rules: Vec<&'a str>,
}

/// Struct that bundles the information retrieved by the `FillSourceLineInfo` method
/// on a Breakpad `SourceLineResolver` for a given stack frame, i.e., "this is line
/// `source_line` in file `source_file`, belonging to function `function_name` with
/// base address `function_base`".
#[derive(Clone, Copy, Debug)]
struct SourceLineInfo<'a> {
    /// The name of the function the line belongs to.
    function_name: &'a str,

    /// The base address of the function the line belongs to.
    function_base: u64,

    /// The name of the source file containing the line.
    source_file_name: &'a str,

    /// The line's number in its source file.
    source_line: u64,
}

/// A Rust implementation of Breakpad's [`SourceLineResolverInterface`](https://github.com/google/breakpad/blob/main/src/google_breakpad/processor/source_line_resolver_interface.h).
/// The only methods we really need are `HasModule`, `FindCFIFrameInfo`,
/// `FindWindowsFrameInfo`, and `FillSourceLineInfo`.
struct SymbolicSourceLineResolver<'a> {
    /// The endianness to use when evaluating memory contents.
    endian: RuntimeEndian,

    /// A map containing Dwarf CFI information for modules.
    unwind_dwarf: DwarfUnwindRulesMap<'a>,

    /// A map containing Windows CFI information for modules.
    unwind_win: WinUnwindRulesMap<'a>,

    symcaches: SymCacheMap<'a>,
}

impl<'a> SymbolicSourceLineResolver<'a> {
    /// Create a new SourceLineResolver from the given frame information.
    fn new(frame_infos: Option<&'a FrameInfoMap<'a>>) -> Self {
        if let Some(frame_infos) = frame_infos {
            let mut modules_cfi = DwarfUnwindRulesMap::default();
            let mut modules_win = WinUnwindRulesMap::default();
            for (id, cache) in frame_infos.iter() {
                modules_cfi.insert(
                    id,
                    DwarfUnwindRules::new(BreakpadStackCfiRecords::new(cache.as_slice())),
                );

                modules_win.insert(
                    id,
                    WinUnwindRules::new(BreakpadStackWinRecords::new(cache.as_slice())),
                );
            }

            Self {
                endian: RuntimeEndian::Little,
                unwind_dwarf: modules_cfi,
                unwind_win: modules_win,
                symcaches: SymCacheMap::default(),
            }
        } else {
            Self {
                endian: RuntimeEndian::Little,
                unwind_dwarf: DwarfUnwindRulesMap::new(),
                unwind_win: WinUnwindRulesMap::new(),
                symcaches: SymCacheMap::default(),
            }
        }
    }

    /// Returns true if the module with the given debug identifier has been loaded.
    fn has_module(&self, debug_id: &CodeModuleId) -> bool {
        self.unwind_dwarf.contains_key(debug_id)
    }

    /// Finds CFI information for a given module and address.
    ///
    /// "CFI information" here means a [`CfiFrameInfo`] object containing the rules that allow
    /// recovery of the caller's registers givent the callee's registers.
    fn find_cfi_frame_info(&mut self, module: &CodeModuleId, address: u64) -> CfiFrameInfo {
        let rules = self
            .unwind_dwarf
            .get_mut(module)
            .and_then(|unwind_rules| unwind_rules.get(address))
            .unwrap_or_default();

        CfiFrameInfo {
            endian: self.endian,
            rules,
        }
    }
    /// Finds CFI information for a given module and address.
    ///
    /// "CFI information" here means a [`STACK WIN` record](symbolic_debuginfo::breakpad::BreakpadStackWinRecord).
    fn find_windows_frame_info(
        &mut self,
        module: &CodeModuleId,
        address: u32,
    ) -> Option<&BreakpadStackWinRecord> {
        self.unwind_win
            .get_mut(module)
            .and_then(|win_records| win_records.get(address))
    }

    /// Retrieves information for the line at the given address in the given module.
    fn fill_source_line_info<'b>(
        &'b self,
        module: &'b CodeModuleId,
        address: u64,
    ) -> Option<SourceLineInfo<'a>> {
        self.symcaches.get(module).and_then(|cache| {
            let line_info = cache.lookup(address).ok()?.find_map(Result::ok)?;
            let function_name = line_info.symbol();
            let source_file_name = line_info.filename();

            Some(SourceLineInfo {
                function_name,
                function_base: line_info.function_address(),
                source_file_name,
                source_line: line_info.line() as u64,
            })
        })
    }
}

#[no_mangle]
unsafe extern "C" fn resolver_set_endian(resolver: *mut c_void, is_big_endian: bool) {
    let resolver = &mut *(resolver as *mut SymbolicSourceLineResolver);
    resolver.endian = if is_big_endian {
        RuntimeEndian::Big
    } else {
        RuntimeEndian::Little
    };
}

#[no_mangle]
unsafe extern "C" fn resolver_has_module(resolver: *mut c_void, module: *const c_char) -> bool {
    if module.is_null() {
        return false;
    }

    let resolver = &mut *(resolver as *mut SymbolicSourceLineResolver);
    let module = match CStr::from_ptr(module).to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };

    let module: CodeModuleId = match module.parse() {
        Ok(id) => id,
        Err(_) => return false,
    };

    resolver.has_module(&module)
}

#[no_mangle]
unsafe extern "C" fn resolver_fill_source_line_info(
    resolver: *mut c_void,
    module: *const c_char,
    address: u64,
    function_name_out: *mut *const c_char,
    function_name_len_out: *mut usize,
    function_base_out: *mut u64,
    source_file_name_out: *mut *const c_char,
    source_file_name_len_out: *mut usize,
    source_line_out: *mut u64,
) {
    let resolver = &mut *(resolver as *mut SymbolicSourceLineResolver);
    let module = match CStr::from_ptr(module).to_str() {
        Ok(s) => s,
        Err(_) => return,
    };

    let module: CodeModuleId = match module.parse() {
        Ok(id) => id,
        Err(_) => return,
    };

    if let Some(source_line_info) = resolver.fill_source_line_info(&module, address) {
        *function_name_out = source_line_info.function_name.as_ptr() as *const i8;
        *function_name_len_out = source_line_info.function_name.len();
        *source_file_name_out = source_line_info.source_file_name.as_ptr() as *const i8;
        *source_file_name_len_out = source_line_info.source_file_name.len();
        *function_base_out = source_line_info.function_base;
        *source_line_out = source_line_info.source_line;
    }
}

#[no_mangle]
unsafe extern "C" fn resolver_find_cfi_frame_info(
    resolver: *mut c_void,
    module: *const c_char,
    address: u64,
) -> *mut c_void {
    let resolver = &mut *(resolver as *mut SymbolicSourceLineResolver);
    let module = match CStr::from_ptr(module).to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };

    let module: CodeModuleId = match module.parse() {
        Ok(id) => id,
        Err(_) => return ptr::null_mut(),
    };

    let cfi_frame_info = resolver.find_cfi_frame_info(&module, address);
    Box::into_raw(Box::new(cfi_frame_info)) as *mut c_void
}

#[no_mangle]
unsafe extern "C" fn resolver_find_windows_frame_info(
    resolver: *mut c_void,
    module: *const c_char,
    address: u32,
    type_out: &mut i64,
    prolog_size_out: &mut u32,
    epilog_size_out: &mut u32,
    parameter_size_out: &mut u32,
    saved_register_size_out: &mut u32,
    local_size_out: &mut u32,
    max_stack_size_out: &mut u32,
    allocates_base_pointer_out: &mut bool,
    program_string_out: &mut *const c_char,
    program_string_len_out: &mut usize,
) -> bool {
    let resolver = &mut *(resolver as *mut SymbolicSourceLineResolver);
    let module = match CStr::from_ptr(module).to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };

    let module: CodeModuleId = match module.parse() {
        Ok(id) => id,
        Err(_) => return false,
    };

    if let Some(record) = resolver.find_windows_frame_info(&module, address) {
        *type_out = record.ty as i64;

        *prolog_size_out = record.prolog_size as u32;
        *epilog_size_out = record.epilog_size as u32;
        *parameter_size_out = record.params_size;
        *saved_register_size_out = record.saved_regs_size as u32;
        *local_size_out = record.locals_size;
        *max_stack_size_out = record.max_stack_size;
        *allocates_base_pointer_out = record.uses_base_pointer;

        if let Some(ps) = record.program_string {
            *program_string_out = ps.as_ptr() as *const i8;
            *program_string_len_out = ps.len();
        }

        true
    } else {
        false
    }
}

#[no_mangle]
unsafe extern "C" fn find_caller_regs_32(
    cfi_frame_info: *const c_void,
    memory_base: u64,
    memory_len: usize,
    memory_bytes: *const u8,
    registers: *const IRegVal,
    registers_len: usize,
    size_out: *mut usize,
) -> *mut IRegVal {
    let cfi_frame_info = &*(cfi_frame_info as *const CfiFrameInfo<'_>);
    let mut evaluator = Box::new(Evaluator::new(cfi_frame_info.endian));

    for rules_string in cfi_frame_info.rules.iter() {
        if evaluator.add_cfi_rules_string(rules_string).is_err() {
            return std::ptr::null_mut();
        }
    }

    let memory = MemoryRegion {
        base_addr: memory_base,
        contents: std::slice::from_raw_parts(memory_bytes, memory_len),
    };

    *evaluator = evaluator.memory(memory);

    let mut variables = BTreeMap::new();
    let mut constants = BTreeMap::new();

    let registers = std::slice::from_raw_parts(registers, registers_len);
    for IRegVal { name, value, size } in registers {
        let value = match size {
            4 => *value as u32,
            _ => continue,
        };

        let name = match CStr::from_ptr(*name).to_str() {
            Ok(name) => name,
            Err(_) => continue,
        };

        if let Ok(r) = name.parse() {
            variables.insert(r, value);
        } else if let Ok(r) = name.parse() {
            constants.insert(r, value);
        }
    }

    *evaluator = evaluator.constants(constants).variables(variables);

    let caller_registers = evaluator.evaluate_cfi_rules().unwrap_or_default();
    if caller_registers.contains_key(&Identifier::Const(Constant::cfa()))
        && caller_registers.contains_key(&Identifier::Const(Constant::ra()))
    {
        let mut result = Vec::new();
        for (register, value) in caller_registers.into_iter() {
            let name = match CString::new(register.to_string()) {
                Ok(name) => name,
                Err(_) => continue,
            };

            result.push(IRegVal {
                name: name.into_raw() as *const c_char,
                value: value as u64,
                size: 4,
            });
        }

        result.shrink_to_fit();
        let len = result.len();
        let ptr = result.as_mut_ptr();

        if !size_out.is_null() {
            *size_out = len;
        }

        std::mem::forget(result);

        ptr
    } else {
        std::ptr::null_mut() as *mut _
    }
}

#[no_mangle]
unsafe extern "C" fn find_caller_regs_64(
    cfi_frame_info: *const c_void,
    memory_base: u64,
    memory_len: usize,
    memory_bytes: *const u8,
    registers: *const IRegVal,
    registers_len: usize,
    size_out: *mut usize,
) -> *mut IRegVal {
    let cfi_frame_info = &*(cfi_frame_info as *const CfiFrameInfo<'_>);
    let mut evaluator = Box::new(Evaluator::new(cfi_frame_info.endian));

    for rules_string in cfi_frame_info.rules.iter() {
        if evaluator.add_cfi_rules_string(rules_string).is_err() {
            return std::ptr::null_mut();
        }
    }

    let memory = MemoryRegion {
        base_addr: memory_base,
        contents: std::slice::from_raw_parts(memory_bytes, memory_len),
    };

    *evaluator = evaluator.memory(memory);

    let mut variables = BTreeMap::new();
    let mut constants = BTreeMap::new();

    let registers = std::slice::from_raw_parts(registers, registers_len);
    for IRegVal { name, value, size } in registers {
        let value = match size {
            8 => *value,
            _ => continue,
        };

        let name = match CStr::from_ptr(*name).to_str() {
            Ok(name) => name,
            Err(_) => continue,
        };

        if let Ok(r) = name.parse() {
            variables.insert(r, value);
        } else if let Ok(r) = name.parse() {
            constants.insert(r, value);
        }
    }

    *evaluator = evaluator.constants(constants).variables(variables);

    let caller_registers = evaluator.evaluate_cfi_rules().unwrap_or_default();
    if caller_registers.contains_key(&Identifier::Const(Constant::cfa()))
        && caller_registers.contains_key(&Identifier::Const(Constant::ra()))
    {
        let mut result = Vec::new();
        for (register, value) in caller_registers.into_iter() {
            let name = match CString::new(register.to_string()) {
                Ok(name) => name,
                Err(_) => continue,
            };

            result.push(IRegVal {
                name: name.into_raw() as *const c_char,
                value,
                size: 8,
            });
        }

        result.shrink_to_fit();
        let len = result.len();
        let ptr = result.as_mut_ptr();

        if !size_out.is_null() {
            *size_out = len;
        }

        std::mem::forget(result);

        ptr
    } else {
        std::ptr::null_mut() as *mut _
    }
}

#[no_mangle]
unsafe extern "C" fn cfi_frame_info_free(cfi_frame_info: *mut c_void) {
    std::mem::drop(Box::from_raw(cfi_frame_info as *mut CfiFrameInfo));
}

#[no_mangle]
unsafe extern "C" fn regvals_free(reg_vals: *mut IRegVal, size: usize) {
    let values = Vec::from_raw_parts(reg_vals, size, size);
    for value in values {
        std::mem::drop(CString::from_raw(value.name as *mut c_char));
    }
}

/// An error returned when parsing an invalid [`CodeModuleId`](struct.CodeModuleId.html).
pub type ParseCodeModuleIdError = ParseDebugIdError;

/// Breakpad code module IDs.
///
/// # Example
///
/// ```rust
/// use std::str::FromStr;
/// use symbolic_minidump::processor::CodeModuleId;
/// # use symbolic_minidump::processor::ParseCodeModuleIdError;
///
/// # fn main() -> Result<(), ParseCodeModuleIdError> {
/// let id = CodeModuleId::from_str("DFB8E43AF2423D73A453AEB6A777EF75a")?;
/// assert_eq!(
///     "DFB8E43AF2423D73A453AEB6A777EF75a".to_string(),
///     id.to_string()
/// );
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Copy)]
pub struct CodeModuleId {
    inner: DebugId,
}

impl CodeModuleId {
    /// Constructs a `CodeModuleId` from its `uuid` and `age` parts.
    pub fn from_parts(uuid: Uuid, age: u32) -> CodeModuleId {
        CodeModuleId {
            inner: DebugId::from_parts(uuid, age),
        }
    }

    /// Returns the UUID part of the code module id.
    pub fn uuid(&self) -> Uuid {
        self.inner.uuid()
    }

    /// Returns the appendix part of the code module id.
    ///
    /// On Windows, this is an incrementing counter to identify the build.
    /// On all other platforms, this value will always be zero.
    pub fn age(&self) -> u32 {
        self.inner.appendix()
    }

    /// Converts this code module id into a debug identifier.
    pub fn as_object_id(&self) -> DebugId {
        self.inner
    }
}

impl From<DebugId> for CodeModuleId {
    fn from(inner: DebugId) -> Self {
        CodeModuleId { inner }
    }
}

impl From<CodeModuleId> for DebugId {
    fn from(source: CodeModuleId) -> Self {
        source.inner
    }
}

impl fmt::Display for CodeModuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.breakpad().fmt(f)
    }
}

impl str::FromStr for CodeModuleId {
    type Err = ParseCodeModuleIdError;

    fn from_str(string: &str) -> Result<CodeModuleId, ParseCodeModuleIdError> {
        Ok(CodeModuleId {
            inner: DebugId::from_breakpad(string)?,
        })
    }
}

#[cfg(feature = "serde")]
impl ::serde::ser::Serialize for CodeModuleId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::ser::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(feature = "serde")]
impl<'de> ::serde::de::Deserialize<'de> for CodeModuleId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: ::serde::de::Deserializer<'de>,
    {
        <::std::borrow::Cow<str>>::deserialize(deserializer)?
            .parse()
            .map_err(::serde::de::Error::custom)
    }
}

/// Carries information about a code module loaded into the process during the
/// crash. The `debug_identifier` uniquely identifies this module.
#[repr(C)]
pub struct CodeModule(c_void);

impl CodeModule {
    /// Returns the unique identifier of this `CodeModule`, which corresponds to the identifier
    /// returned by [`debug_identifier`](struct.CodeModuleId#method.debug_identifier).
    pub fn id(&self) -> Option<CodeModuleId> {
        match self.debug_identifier().as_str() {
            "" => None,
            id => CodeModuleId::from_str(id).ok(),
        }
    }

    /// Returns the base address of this code module as it was loaded by the
    /// process. (uint64_t)-1 on error.
    pub fn base_address(&self) -> u64 {
        unsafe { code_module_base_address(self) }
    }

    /// The size of the code module. 0 on error.
    pub fn size(&self) -> u64 {
        unsafe { code_module_size(self) }
    }

    /// Returns the path or file name that the code module was loaded from.
    pub fn code_file(&self) -> String {
        unsafe {
            let ptr = code_module_code_file(self);
            utils::ptr_to_string(ptr)
        }
    }

    /// An identifying string used to discriminate between multiple versions and builds of the same
    /// code module.
    ///
    /// The contents of this identifier are implementation defined. GCC generally uses a 40
    /// character (20 byte) SHA1 checksum of the code. On Windows, this is the program timestamp and
    /// version number. On macOS, this value is empty.
    pub fn code_identifier(&self) -> String {
        let id = unsafe {
            let ptr = code_module_code_identifier(self);
            utils::ptr_to_string(ptr)
        };

        // For platforms that do not have explicit code identifiers, the breakpad processor returns
        // a hardcoded "id". Since this is only a placeholder, return an empty string instead.
        if id == "id" {
            String::new()
        } else {
            id
        }
    }

    /// Returns the filename containing debugging information of this code module.
    ///
    /// If debugging information is stored in a file separate from the code module itself (as is the
    /// case when .pdb or .dSYM files are used), this will be different from `code_file`.  If
    /// debugging information is stored in the code module itself (possibly prior to stripping),
    /// this will be the same as code_file.
    pub fn debug_file(&self) -> String {
        unsafe {
            let ptr = code_module_debug_file(self);
            utils::ptr_to_string(ptr)
        }
    }

    /// Returns a string identifying the specific version and build of the associated debug file.
    ///
    /// This may be the same as `code_identifier` when the `debug_file` and `code_file` are
    /// identical or when the same identifier is used to identify distinct debug and code files.
    ///
    /// It usually comprises the library's UUID and an age field. On Windows, the age field is a
    /// generation counter, on all other platforms it is mostly zero.
    pub fn debug_identifier(&self) -> String {
        let id = unsafe {
            let ptr = code_module_debug_identifier(self);
            utils::ptr_to_string(ptr)
        };

        // The breakpad processor sometimes returns only zeros when it cannot determine a debug
        // identifier, for example from mapped fonts or shared memory regions. Since this is
        // clearly a garbage value, return an empty string instead.
        if id == "000000000000000000000000000000000" {
            String::new()
        } else {
            id
        }
    }
}

impl Eq for CodeModule {}

impl PartialEq for CodeModule {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Hash for CodeModule {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id().hash(state)
    }
}

impl Ord for CodeModule {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id().cmp(&other.id())
    }
}

impl PartialOrd for CodeModule {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Debug for CodeModule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CodeModule")
            .field("id", &self.id())
            .field("base_address", &self.base_address())
            .field("size", &self.size())
            .field("code_file", &self.code_file())
            .field("code_identifier", &self.code_identifier())
            .field("debug_file", &self.debug_file())
            .field("debug_identifier", &self.debug_identifier())
            .finish()
    }
}

/// Indicates how well the instruction pointer derived during
/// stack walking is trusted. Since the stack walker can resort to
/// stack scanning, it can wind up with dubious frames.
///
/// In rough order of "trust metric".
#[allow(clippy::upper_case_acronyms)]
#[repr(u32)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum FrameTrust {
    /// Unknown trust.
    None,

    /// Scanned the stack, found this (lowest precision).
    Scan,

    /// Found while scanning stack using call frame info.
    CFIScan,

    /// Derived from frame pointer.
    FP,

    /// Derived from call frame info.
    CFI,

    /// Explicitly provided by some external stack walker.
    Prewalked,

    /// Given as instruction pointer in a context (highest precision).
    Context,
}

impl fmt::Display for FrameTrust {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let string = match *self {
            FrameTrust::None => "none",
            FrameTrust::Scan => "stack scanning",
            FrameTrust::CFIScan => "call frame info with scanning",
            FrameTrust::FP => "previous frame's frame pointer",
            FrameTrust::CFI => "call frame info",
            FrameTrust::Prewalked => "recovered by external stack walker",
            FrameTrust::Context => "given as instruction pointer in context",
        };

        write!(f, "{}", string)
    }
}

/// Error when converting a string to [`FrameTrust`].
///
/// [`FrameTrust`]: enum.FrameTrust.html
#[derive(Debug)]
pub struct ParseFrameTrustError;

impl fmt::Display for ParseFrameTrustError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to parse frame trust")
    }
}

impl FromStr for FrameTrust {
    type Err = ParseFrameTrustError;

    fn from_str(string: &str) -> Result<FrameTrust, Self::Err> {
        Ok(match string {
            "none" => FrameTrust::None,
            "scan" => FrameTrust::Scan,
            "cfiscan" => FrameTrust::CFIScan,
            "fp" => FrameTrust::FP,
            "cfi" => FrameTrust::CFI,
            "prewalked" => FrameTrust::Prewalked,
            "context" => FrameTrust::Context,
            _ => return Err(ParseFrameTrustError),
        })
    }
}

impl std::error::Error for ParseFrameTrustError {}

impl Default for FrameTrust {
    fn default() -> FrameTrust {
        FrameTrust::None
    }
}

#[cfg(feature = "serde")]
impl ::serde::ser::Serialize for FrameTrust {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::ser::Serializer,
    {
        serializer.serialize_str(match *self {
            FrameTrust::None => "none",
            FrameTrust::Scan => "scan",
            FrameTrust::CFIScan => "cfiscan",
            FrameTrust::FP => "fp",
            FrameTrust::CFI => "cfi",
            FrameTrust::Prewalked => "prewalked",
            FrameTrust::Context => "context",
        })
    }
}

#[cfg(feature = "serde")]
impl<'de> ::serde::de::Deserialize<'de> for FrameTrust {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: ::serde::de::Deserializer<'de>,
    {
        <::std::borrow::Cow<str>>::deserialize(deserializer)?
            .parse()
            .map_err(::serde::de::Error::custom)
    }
}

/// Helper for register values.
#[repr(C)]
struct IRegVal {
    name: *const c_char,
    value: u64,
    size: u8,
}

/// Value of a stack frame register.
#[derive(Clone, Copy, Debug)]
pub enum RegVal {
    /// 32-bit register value.
    U32(u32),
    /// 64-bit register value.
    U64(u64),
}

impl fmt::Display for RegVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            RegVal::U32(u) => write!(f, "{:#010x}", u),
            RegVal::U64(u) => write!(f, "{:#018x}", u),
        }
    }
}

/// Contains information from the memorydump, especially the frame's instruction
/// pointer. Also references an optional `CodeModule` that contains the
/// instruction of this stack frame.
#[repr(C)]
pub struct StackFrame(c_void);

impl StackFrame {
    /// Returns the program counter location as an absolute virtual address.
    ///
    /// - For the innermost called frame in a stack, this will be an exact
    ///   program counter or instruction pointer value.
    ///
    /// - For all other frames, this address is within the instruction that
    ///   caused execution to branch to this frame's callee (although it may
    ///   not point to the exact beginning of that instruction). This ensures
    ///   that, when we look up the source code location for this frame, we
    ///   get the source location of the call, not of the point at which
    ///   control will resume when the call returns, which may be on the next
    ///   line. (If the compiler knows the callee never returns, it may even
    ///   place the call instruction at the very end of the caller's machine
    ///   code, such that the "return address" (which will never be used)
    ///   immediately after the call instruction is in an entirely different
    ///   function, perhaps even from a different source file.)
    ///
    /// On some architectures, the return address as saved on the stack or in
    /// a register is fine for looking up the point of the call. On others, it
    /// requires adjustment. ReturnAddress returns the address as saved by the
    /// machine.
    ///
    /// Use `trust` to obtain how trustworthy this instruction is.
    pub fn instruction(&self) -> u64 {
        unsafe { stack_frame_instruction(self) }
    }

    /// Return the actual return address, as saved on the stack or in a
    /// register. See the comments for `StackFrame::instruction' for
    /// details.
    pub fn return_address(&self, arch: Arch) -> u64 {
        let address = unsafe { stack_frame_return_address(self) };

        // The return address reported for ARM* frames is actually the
        // instruction with heuristics from Breakpad applied already.
        // To resolve the original return address value, compensate
        // by adding the offsets applied in `StackwalkerARM::GetCallerFrame`
        // and `StackwalkerARM64::GetCallerFrame`.
        match arch.cpu_family() {
            CpuFamily::Arm32 => address + 2,
            CpuFamily::Arm64 => address + 4,
            _ => address,
        }
    }

    /// Returns the `CodeModule` that contains this frame's instruction.
    pub fn module(&self) -> Option<&CodeModule> {
        unsafe { stack_frame_module(self).as_ref() }
    }

    /// Returns how well the instruction pointer is trusted.
    pub fn trust(&self) -> FrameTrust {
        unsafe { stack_frame_trust(self) }
    }

    /// Returns a mapping of registers to their known values, if any.
    pub fn registers(&self, arch: Arch) -> BTreeMap<&'static str, RegVal> {
        unsafe {
            let mut size = 0;
            let values = stack_frame_registers(self, arch.cpu_family() as u32, &mut size);
            let map = slice::from_raw_parts(values, size)
                .iter()
                .filter_map(|v| {
                    Some((
                        CStr::from_ptr(v.name).to_str().unwrap(),
                        match v.size {
                            4 => RegVal::U32(v.value as u32),
                            8 => RegVal::U64(v.value),
                            _ => return None,
                        },
                    ))
                })
                .collect();

            regval_delete(values);
            map
        }
    }
}

impl fmt::Debug for StackFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StackFrame")
            .field("return_address", &self.return_address(Arch::Unknown))
            .field("instruction", &self.instruction())
            .field("trust", &self.trust())
            .field("module", &self.module())
            .finish()
    }
}

/// Represents a thread of the `ProcessState` which holds a list of [`StackFrame`]s.
///
/// [`StackFrame`]: struct.StackFrame.html
#[repr(C)]
pub struct CallStack(c_void);

impl CallStack {
    /// Returns the thread identifier of this callstack.
    pub fn thread_id(&self) -> u32 {
        unsafe { call_stack_thread_id(self) }
    }

    /// Returns the list of `StackFrame`s in the call stack.
    pub fn frames(&self) -> &[&StackFrame] {
        unsafe {
            let mut size = 0;
            let data = call_stack_frames(self, &mut size);
            slice::from_raw_parts(data as *const &StackFrame, size)
        }
    }
}

impl fmt::Debug for CallStack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallStack")
            .field("thread_id", &self.thread_id())
            .field("frames", &self.frames())
            .finish()
    }
}

/// Information about the CPU and OS on which a minidump was generated.
#[repr(C)]
pub struct SystemInfo(c_void);

impl SystemInfo {
    /// A string identifying the operating system, such as "Windows NT", "Mac OS X", or "Linux".
    ///
    /// If the information is present in the dump but its value is unknown, this field will contain
    /// a numeric value.  If the information is not present in the dump, this field will be empty.
    pub fn os_name(&self) -> String {
        unsafe {
            let ptr = system_info_os_name(self);
            utils::ptr_to_string(ptr)
        }
    }

    /// Strings identifying the version and build number of the operating system.
    ///
    /// If the dump does not contain either information, the component will be empty. Tries to parse
    /// the version number from the build if it is not apparent from the version string.
    pub fn os_parts(&self) -> (String, String) {
        let string = unsafe {
            let ptr = system_info_os_version(self);
            utils::ptr_to_string(ptr)
        };

        let mut parts = string.splitn(2, ' ');
        let version = parts.next().unwrap_or("0.0.0");
        let build = parts.next().unwrap_or("");

        if version == "0.0.0" {
            // Try to parse the Linux build string. Breakpad and Crashpad run
            // `uname -srvmo` to generate it. This roughtly resembles:
            // "Linux [version] [build...] [arch] Linux/GNU"
            if let Some(captures) = LINUX_BUILD_RE.captures(build) {
                let version = captures.get(1).unwrap(); // uname -r portion
                let build = captures.get(2).unwrap(); // uname -v portion
                return (version.as_str().into(), build.as_str().into());
            }
        }

        (version.into(), build.into())
    }

    /// A string identifying the version of the operating system.
    ///
    /// The version will be formatted as three-component semantic version, such as "5.1.2600" or
    /// "10.4.8".  If the dump does not contain this information, this field will contain "0.0.0".
    pub fn os_version(&self) -> String {
        self.os_parts().0
    }

    /// A string identifying the build of the operating system.
    ///
    /// This build version is platform dependent, such as "Service Pack 2" or "8L2127".  If the dump
    /// does not contain this information, this field will be empty.
    pub fn os_build(&self) -> String {
        self.os_parts().1
    }

    /// A string identifying the basic CPU family, such as "x86" or "ppc".
    ///
    /// If this information is present in the dump but its value is unknown,
    /// this field will contain a numeric value.  If the information is not
    /// present in the dump, this field will be empty.
    pub fn cpu_family(&self) -> String {
        unsafe {
            let ptr = system_info_cpu_family(self);
            utils::ptr_to_string(ptr)
        }
    }

    /// The architecture of the CPU parsed from `ProcessState::cpu_family`.
    ///
    /// If this information is present in the dump but its value is unknown
    /// or if the value is missing, this field will contain `Arch::Unknown`.
    pub fn cpu_arch(&self) -> Arch {
        self.cpu_family().parse().unwrap_or_default()
    }

    /// A string further identifying the specific CPU.
    ///
    /// This information depends on the CPU vendor, such as "GenuineIntel level 6 model 13 stepping
    /// 8". If the information is not present in the dump, or additional identifying information is
    /// not defined for the CPU family, this field will be empty.
    pub fn cpu_info(&self) -> String {
        unsafe {
            let ptr = system_info_cpu_info(self);
            utils::ptr_to_string(ptr)
        }
    }

    /// The number of processors in the system.
    ///
    /// Will be greater than one for multi-core systems.
    pub fn cpu_count(&self) -> u32 {
        unsafe { system_info_cpu_count(self) }
    }
}

impl fmt::Debug for SystemInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SystemInfo")
            .field("os_name", &self.os_name())
            .field("os_version", &self.os_version())
            .field("cpu_family", &self.cpu_family())
            .field("cpu_info", &self.cpu_info())
            .field("cpu_count", &self.cpu_count())
            .finish()
    }
}

/// Result of processing a Minidump or Microdump file.
///
/// Usually included in `ProcessError` when the file cannot be processed.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ProcessResult {
    /// The dump was processed successfully.
    Ok,

    /// The minidump file was not found or the buffer was empty.
    MinidumpNotFound,

    /// The minidump file had no header.
    NoMinidumpHeader,

    /// The minidump file has no thread list.
    NoThreadList,

    /// There was an error getting one thread's data from the dump.
    InvalidThreadIndex,

    /// There was an error getting a thread id from the thread's data.
    InvalidThreadId,

    /// There was more than one requesting thread.
    DuplicateRequestingThreads,

    /// The dump processing was interrupted (not fatal).
    SymbolSupplierInterrupted,
}

impl ProcessResult {
    /// Indicates whether the process state is usable.
    ///
    /// Depending on the result, the process state might only contain partial information. For a
    /// full minidump, check for `ProcessResult::Ok` instead.
    pub fn is_usable(self) -> bool {
        matches!(self, ProcessResult::Ok | ProcessResult::NoThreadList)
    }
}

impl fmt::Display for ProcessResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let formatted = match *self {
            ProcessResult::Ok => "dump processed successfully",
            ProcessResult::MinidumpNotFound => "file could not be opened",
            ProcessResult::NoMinidumpHeader => "minidump header missing",
            ProcessResult::NoThreadList => "minidump has no thread list",
            ProcessResult::InvalidThreadIndex => "could not get thread data",
            ProcessResult::InvalidThreadId => "could not get a thread by id",
            ProcessResult::DuplicateRequestingThreads => "multiple requesting threads",
            ProcessResult::SymbolSupplierInterrupted => "processing was interrupted (not fatal)",
        };

        write!(f, "{}", formatted)
    }
}

/// An error generated when trying to process a minidump.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ProcessMinidumpError(ProcessResult);

impl ProcessMinidumpError {
    /// Returns the kind of this error.
    pub fn kind(&self) -> ProcessResult {
        self.0
    }
}

impl fmt::Display for ProcessMinidumpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "minidump processing failed: {}", self.0)
    }
}

impl std::error::Error for ProcessMinidumpError {}

/// Internal type used to transfer Breakpad symbols over FFI.
#[repr(C)]
struct SymbolEntry {
    debug_identifier: *const c_char,
    symbol_size: usize,
    symbol_data: *const u8,
}

/// Container for call frame information (CFI) of [`CodeModule`]s.
///
/// This information is required by the stackwalker in case framepointers are
/// missing in the raw stacktraces. Frame information is given as plain ASCII
/// text as specified in the Breakpad symbol file specification.
///
/// [`CodeModule`]: struct.CodeModule.html
pub type FrameInfoMap<'a> = BTreeMap<CodeModuleId, CfiCache<'a>>;

type IProcessState = c_void;

/// Snapshot of the state of a processes during its crash. The object can be
/// obtained by processing Minidump or Microdump files.
pub struct ProcessState<'a> {
    internal: *mut IProcessState,
    _ty: PhantomData<ByteView<'a>>,
}

impl<'a> ProcessState<'a> {
    /// Processes a minidump supplied via raw binary data.
    ///
    /// Returns a `ProcessState` that contains information about the crashed
    /// process. The parameter `frame_infos` expects a map of Breakpad symbols
    /// containing STACK CFI and STACK WIN records to allow stackwalking with
    /// omitted frame pointers.
    pub fn from_minidump(
        buffer: &ByteView<'a>,
        frame_infos: Option<&FrameInfoMap<'_>>,
    ) -> Result<ProcessState<'a>, ProcessMinidumpError> {
        let cfi_count = frame_infos.map_or(0, BTreeMap::len);
        let mut result: ProcessResult = ProcessResult::Ok;

        // Keep a reference to all CStrings to extend their lifetime.
        let cfi_vec: Vec<_> = frame_infos.map_or(Vec::new(), |s| {
            s.iter()
                .map(|(k, v)| {
                    (
                        CString::new(k.to_string()),
                        v.as_slice().len(),
                        v.as_slice().as_ptr(),
                    )
                })
                .collect()
        });

        // Keep a reference to all symbol entries to extend their lifetime.
        let cfi_entries: Vec<_> = cfi_vec
            .iter()
            .map(|&(ref id, size, data)| SymbolEntry {
                debug_identifier: id.as_ref().map(|i| i.as_ptr()).unwrap_or(ptr::null()),
                symbol_size: size,
                symbol_data: data,
            })
            .collect();

        let internal = unsafe {
            process_minidump_breakpad(
                buffer.as_ptr() as *const c_char,
                buffer.len(),
                cfi_entries.as_ptr(),
                cfi_count,
                &mut result,
            )
        };

        if result.is_usable() && !internal.is_null() {
            Ok(ProcessState {
                internal,
                _ty: PhantomData,
            })
        } else {
            unsafe { process_state_delete(internal) };
            Err(ProcessMinidumpError(result))
        }
    }

    /// Processes a minidump supplied via raw binary data.
    ///
    /// Returns a `ProcessState` that contains information about the crashed
    /// process. The parameter `frame_infos` expects a map of Breakpad symbols
    /// containing STACK CFI and STACK WIN records to allow stackwalking with
    /// omitted frame pointers.
    pub fn from_minidump_new(
        buffer: &ByteView<'a>,
        frame_infos: Option<&FrameInfoMap<'_>>,
    ) -> Result<ProcessState<'a>, ProcessMinidumpError> {
        let mut result: ProcessResult = ProcessResult::Ok;

        let mut resolver = SymbolicSourceLineResolver::new(frame_infos);
        let internal = unsafe {
            process_minidump_symbolic(
                buffer.as_ptr() as *const c_char,
                buffer.len(),
                (&mut resolver) as *mut _ as *mut c_void,
                &mut result,
            )
        };

        if result.is_usable() && !internal.is_null() {
            Ok(ProcessState {
                internal,
                _ty: PhantomData,
            })
        } else {
            unsafe { process_state_delete(internal) };
            Err(ProcessMinidumpError(result))
        }
    }

    /// The index of the thread that requested a dump be written in the threads vector.
    ///
    /// If a dump was produced as a result of a crash, this will point to the thread that crashed.
    /// If the dump was produced as by user code without crashing, and the dump contains extended
    /// Breakpad information, this will point to the thread that requested the dump. If the dump was
    /// not produced as a result of an exception and no extended Breakpad information is present,
    /// this field will be set to -1, indicating that the dump thread is not available.
    pub fn requesting_thread(&self) -> i32 {
        unsafe { process_state_requesting_thread(self.internal) }
    }

    /// The time-date stamp of the minidump.
    pub fn timestamp(&self) -> u64 {
        unsafe { process_state_timestamp(self.internal) }
    }

    /// True if the process crashed, false if the dump was produced outside
    /// of an exception handler.
    pub fn crashed(&self) -> bool {
        unsafe { process_state_crashed(self.internal) }
    }

    /// If the process crashed, and if crash_reason implicates memory, the memory address that
    /// caused the crash.
    ///
    /// For data access errors, this will be the data address that caused the fault.  For code
    /// errors, this will be the address of the instruction that caused the fault.
    pub fn crash_address(&self) -> u64 {
        unsafe { process_state_crash_address(self.internal) }
    }

    /// If the process crashed, the type of crash.
    ///
    /// OS- and possibly CPU-specific.  For example, "EXCEPTION_ACCESS_VIOLATION" (Windows),
    /// "EXC_BAD_ACCESS / KERN_INVALID_ADDRESS" (Mac OS X), "SIGSEGV" (other Unix).
    pub fn crash_reason(&self) -> String {
        unsafe {
            let ptr = process_state_crash_reason(self.internal);
            utils::ptr_to_string(ptr)
        }
    }

    /// If there was an assertion that was hit, a textual representation
    /// of that assertion, possibly including the file and line at which
    /// it occurred.
    pub fn assertion(&self) -> String {
        unsafe {
            let ptr = process_state_assertion(self.internal);
            utils::ptr_to_string(ptr)
        }
    }

    /// Returns OS and CPU information.
    pub fn system_info(&self) -> &SystemInfo {
        unsafe { process_state_system_info(self.internal).as_ref().unwrap() }
    }

    /// Returns a list of `CallStack`s in the minidump.
    pub fn threads(&self) -> &[&CallStack] {
        unsafe {
            let mut size = 0;
            let data = process_state_threads(self.internal, &mut size);
            slice::from_raw_parts(data as *const &CallStack, size)
        }
    }

    /// Returns the full list of loaded `CodeModule`s.
    pub fn modules(&self) -> Vec<&CodeModule> {
        unsafe {
            let mut size = 0;
            let data = process_state_modules(self.internal, &mut size);
            let vec = slice::from_raw_parts(data as *mut &CodeModule, size).to_vec();
            code_modules_delete(data);
            vec
        }
    }

    /// Returns a list of all `CodeModule`s referenced in one of the `CallStack`s.
    pub fn referenced_modules(&self) -> BTreeSet<&CodeModule> {
        self.threads()
            .iter()
            .flat_map(|stack| stack.frames().iter())
            .filter_map(|frame| frame.module())
            .collect()
    }
}

impl<'a> Drop for ProcessState<'a> {
    fn drop(&mut self) {
        unsafe { process_state_delete(self.internal) };
    }
}

impl<'a> fmt::Debug for ProcessState<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProcessState")
            .field("requesting_thread", &self.requesting_thread())
            .field("timestamp", &self.timestamp())
            .field("crash_address", &self.crash_address())
            .field("crash_reason", &self.crash_reason())
            .field("assertion", &self.assertion())
            .field("system_info", &self.system_info())
            .field("threads", &self.threads())
            .field("modules", &self.modules())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Creates a vector of nested subranges of `range` by recursively halving `range`
    /// and shuffling the end result.
    fn arb_nested_ranges(range: Range<u32>) -> impl Strategy<Value = Vec<Range<u32>>> {
        fn go(range: Range<u32>, acc: &mut Vec<Range<u32>>) {
            let mid = (range.end - range.start) / 2;
            if mid > range.start + 1 {
                go(range.start..mid, acc);
            }
            if range.start > mid + 1 {
                go(mid..range.end, acc);
            }

            acc.push(range);
        }

        let mut ranges = Vec::new();
        go(range, &mut ranges);

        Just(ranges).prop_shuffle()
    }

    /// Checks that a `NestedRangeMap` is actually properly nested.
    fn check<A: Ord, E>(map: NestedRangeMap<A, E>, range: Option<Range<A>>) {
        for (r, _, sub_map) in map.inner {
            if let Some(ref range) = range {
                assert!(range.contains(&r.start) && range.contains(&r.end));
            }
            check(*sub_map, Some(r));
        }
    }

    #[test]
    fn nested_range_map_simple() {
        let mut map = NestedRangeMap::default();

        assert!(map.insert(0u8..10, "Outer"));
        assert!(!map.insert(5..15, "Overlapping"));
        assert!(map.insert(1..5, "Middle 1"));
        assert!(map.insert(2..4, "Inner 1"));
        assert!(map.insert(5..8, "Middle 2"));
        assert!(!map.insert(3..8, "Overlapping"));
        assert!(map.insert(6..8, "Inner 2"));
        assert!(map.insert(0..9, "Middle 3"));

        //  0    1    2    3    4    5    6    7    8    9    10
        //          [Inner 1 ]          [Inner 2 ]
        //     [   Middle 1       ][   Middle 2  ]
        // [                  Middle 3                ]
        // [                    Outer                      ]
        assert_eq!(map.get_contents(0).unwrap(), &"Middle 3");
        assert_eq!(map.get_contents(1).unwrap(), &"Middle 1");
        assert_eq!(map.get_contents(2).unwrap(), &"Inner 1");
        assert_eq!(map.get_contents(3).unwrap(), &"Inner 1");
        assert_eq!(map.get_contents(4).unwrap(), &"Middle 1");
        assert_eq!(map.get_contents(5).unwrap(), &"Middle 2");
        assert_eq!(map.get_contents(6).unwrap(), &"Inner 2");
        assert_eq!(map.get_contents(7).unwrap(), &"Inner 2");
        assert_eq!(map.get_contents(8).unwrap(), &"Middle 3");
        assert_eq!(map.get_contents(9).unwrap(), &"Outer");
        assert_eq!(map.get_contents(10), None);
    }

    proptest! {
        #[test]
        fn proptest_nested_range_map(ranges in arb_nested_ranges(0..100)) {
            let mut map = NestedRangeMap::default();

            for range in ranges.into_iter() {
                assert!(map.insert(range, ()));
            }

            check(map, None);
        }
    }
}
