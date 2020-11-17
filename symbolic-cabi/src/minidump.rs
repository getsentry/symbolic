use std::ffi::CStr;
use std::mem;
use std::os::raw::c_char;
use std::slice;
use std::str::FromStr;

use symbolic::common::{Arch, ByteView};
use symbolic::minidump::cfi::{CfiCache, CFICACHE_LATEST_VERSION};
use symbolic::minidump::processor::{
    CallStack, CodeModule, CodeModuleId, FrameInfoMap, ProcessState, RegVal, StackFrame, SystemInfo,
};

use crate::core::SymbolicStr;
use crate::debuginfo::SymbolicObject;
use crate::utils::ForeignObject;

/// Contains stack frame information (CFI) for an image.
pub struct SymbolicCfiCache;

impl ForeignObject for SymbolicCfiCache {
    type RustObject = CfiCache<'static>;
}

/// A map of stack frame infos for images.
pub struct SymbolicFrameInfoMap;

impl ForeignObject for SymbolicFrameInfoMap {
    type RustObject = FrameInfoMap<'static>;
}

/// Indicates how well the instruction pointer derived during stack walking is trusted.
#[repr(u32)]
pub enum SymbolicFrameTrust {
    None,
    Scan,
    CfiScan,
    Fp,
    Cfi,
    Prewalked,
    Context,
}

/// Carries information about a code module loaded into the process during the crash.
#[repr(C)]
pub struct SymbolicCodeModule {
    pub code_id: SymbolicStr,
    pub code_file: SymbolicStr,
    pub debug_id: SymbolicStr,
    pub debug_file: SymbolicStr,
    pub addr: u64,
    pub size: u64,
}

/// The CPU register value of a stack frame.
#[repr(C)]
pub struct SymbolicRegVal {
    pub name: SymbolicStr,
    pub value: SymbolicStr,
}

/// Contains the absolute instruction address and image information of a stack frame.
#[repr(C)]
pub struct SymbolicStackFrame {
    pub return_address: u64,
    pub instruction: u64,
    pub trust: SymbolicFrameTrust,
    pub module: SymbolicCodeModule,
    pub registers: *mut SymbolicRegVal,
    pub register_count: usize,
}

impl Drop for SymbolicStackFrame {
    fn drop(&mut self) {
        unsafe {
            Vec::from_raw_parts(self.registers, self.register_count, self.register_count);
        }
    }
}

/// Represents a thread of the process state which holds a list of stack frames.
#[repr(C)]
pub struct SymbolicCallStack {
    pub thread_id: u32,
    pub frames: *mut SymbolicStackFrame,
    pub frame_count: usize,
}

impl Drop for SymbolicCallStack {
    fn drop(&mut self) {
        unsafe {
            Vec::from_raw_parts(self.frames, self.frame_count, self.frame_count);
        }
    }
}

/// OS and CPU information in a minidump.
#[repr(C)]
pub struct SymbolicSystemInfo {
    pub os_name: SymbolicStr,
    pub os_version: SymbolicStr,
    pub os_build: SymbolicStr,
    pub cpu_family: SymbolicStr,
    pub cpu_info: SymbolicStr,
    pub cpu_count: u32,
}

/// State of a crashed process in a minidump.
#[repr(C)]
pub struct SymbolicProcessState {
    pub requesting_thread: i32,
    pub timestamp: u64,
    pub crashed: bool,
    pub crash_address: u64,
    pub crash_reason: SymbolicStr,
    pub assertion: SymbolicStr,
    pub system_info: SymbolicSystemInfo,
    pub threads: *mut SymbolicCallStack,
    pub thread_count: usize,
    pub modules: *mut SymbolicCodeModule,
    pub module_count: usize,
}

impl SymbolicProcessState {
    pub unsafe fn from_process_state(state: &ProcessState<'_>) -> Self {
        map_process_state(state)
    }
}

impl Drop for SymbolicProcessState {
    fn drop(&mut self) {
        unsafe {
            Vec::from_raw_parts(self.threads, self.thread_count, self.thread_count);
            Vec::from_raw_parts(self.modules, self.module_count, self.module_count);
        }
    }
}

/// Creates a packed array of mapped FFI elements from a slice.
unsafe fn map_slice<T, S, F>(items: &[T], mut mapper: F) -> (*mut S, usize)
where
    F: FnMut(&T) -> S,
{
    let mut vec = Vec::with_capacity(items.len());
    for item in items {
        vec.push(mapper(item));
    }

    let ptr = vec.as_ptr() as *mut S;
    let len = vec.len();

    mem::forget(vec);
    (ptr, len)
}

/// Creates a packed array of mapped FFI elements from an iterator.
unsafe fn map_iter<T, S, I, F>(items: I, mapper: F) -> (*mut S, usize)
where
    I: Iterator<Item = T>,
    F: Fn(T) -> S,
{
    let mut vec = Vec::with_capacity(items.size_hint().0);
    for item in items {
        vec.push(mapper(item));
    }

    vec.shrink_to_fit();
    let ptr = vec.as_ptr() as *mut S;
    let len = vec.len();

    mem::forget(vec);
    (ptr, len)
}

/// Maps a `CodeModule` to its FFI type.
unsafe fn map_code_module(module: &CodeModule) -> SymbolicCodeModule {
    SymbolicCodeModule {
        code_id: module.code_identifier().into(),
        code_file: module.code_file().into(),
        debug_id: module
            .id()
            .map(|id| id.to_string().into())
            .unwrap_or_default(),
        debug_file: module.debug_file().into(),
        addr: module.base_address(),
        size: module.size(),
    }
}

/// Maps a pair of register name and value to its FFI type.
unsafe fn map_regval(regval: (&str, RegVal)) -> SymbolicRegVal {
    SymbolicRegVal {
        name: regval.0.into(),
        value: regval.1.to_string().into(),
    }
}

/// Maps a `StackFrame` to its FFI type.
unsafe fn map_stack_frame(frame: &StackFrame, arch: Arch) -> SymbolicStackFrame {
    let empty_module = SymbolicCodeModule {
        code_id: "".into(),
        code_file: "".into(),
        debug_id: "".into(),
        debug_file: "".into(),
        addr: 0,
        size: 0,
    };

    let (registers, register_count) =
        map_iter(frame.registers(arch).into_iter(), |r| map_regval(r));

    SymbolicStackFrame {
        instruction: frame.instruction(),
        return_address: frame.return_address(arch),
        trust: mem::transmute(frame.trust()),
        module: frame.module().map_or(empty_module, |m| map_code_module(m)),
        registers,
        register_count,
    }
}

/// Maps a `CallStack` to its FFI type.
unsafe fn map_call_stack(stack: &CallStack, arch: Arch) -> SymbolicCallStack {
    let (frames, frame_count) = map_slice(stack.frames(), |f| map_stack_frame(f, arch));
    SymbolicCallStack {
        thread_id: stack.thread_id(),
        frames,
        frame_count,
    }
}

/// Maps a `SystemInfo` to its FFI type.
unsafe fn map_system_info(info: &SystemInfo) -> SymbolicSystemInfo {
    SymbolicSystemInfo {
        os_name: SymbolicStr::from_string(info.os_name()),
        os_version: SymbolicStr::from_string(info.os_version()),
        os_build: SymbolicStr::from_string(info.os_build()),
        cpu_family: SymbolicStr::from_string(info.cpu_family()),
        cpu_info: SymbolicStr::from_string(info.cpu_info()),
        cpu_count: info.cpu_count(),
    }
}

/// Maps a `ProcessState` to its FFI type.
unsafe fn map_process_state(state: &ProcessState<'_>) -> SymbolicProcessState {
    let arch = state.system_info().cpu_arch();
    let (threads, thread_count) = map_slice(state.threads(), |s| map_call_stack(s, arch));
    let (modules, module_count) = map_iter(state.modules().iter(), |m| map_code_module(m));

    SymbolicProcessState {
        requesting_thread: state.requesting_thread(),
        timestamp: state.timestamp(),
        crashed: state.crashed(),
        crash_address: state.crash_address(),
        crash_reason: SymbolicStr::from_string(state.crash_reason()),
        assertion: SymbolicStr::from_string(state.assertion()),
        system_info: map_system_info(state.system_info()),
        threads,
        thread_count,
        modules,
        module_count,
    }
}

ffi_fn! {
    /// Creates a new frame info map.
    unsafe fn symbolic_frame_info_map_new() -> Result<*mut SymbolicFrameInfoMap> {
        Ok(SymbolicFrameInfoMap::from_rust(FrameInfoMap::new()))
    }
}

ffi_fn! {
    /// Adds the CfiCache for a module specified by `debug_id`. Assumes ownership over the cache.
    unsafe fn symbolic_frame_info_map_add(
        frame_info_map: *mut SymbolicFrameInfoMap,
        debug_id: *const SymbolicStr,
        cfi_cache: *mut SymbolicCfiCache,
    ) -> Result<()> {
        let map = SymbolicFrameInfoMap::as_rust_mut(frame_info_map);
        let id = CodeModuleId::from_str((*debug_id).as_str())?;
        let cache = *SymbolicCfiCache::into_rust(cfi_cache);

        map.insert(id, cache);
        Ok(())
    }
}

ffi_fn! {
    /// Frees a frame info map object.
    unsafe fn symbolic_frame_info_map_free(frame_info_map: *mut SymbolicFrameInfoMap) {
        SymbolicFrameInfoMap::drop(frame_info_map);
    }
}

ffi_fn! {
    /// Processes a minidump with optional CFI information and returns the state
    /// of the process at the time of the crash.
    unsafe fn symbolic_process_minidump(
        path: *const c_char,
        frame_info_map: *const SymbolicFrameInfoMap,
    ) -> Result<*mut SymbolicProcessState> {
        let byteview = ByteView::open(CStr::from_ptr(path).to_str()?)?;
        let map = if frame_info_map.is_null() {
            None
        } else {
            Some(SymbolicFrameInfoMap::as_rust(frame_info_map))
        };

        let state = ProcessState::from_minidump(&byteview, map)?;
        let sstate = SymbolicProcessState::from_process_state(&state);
        Ok(Box::into_raw(Box::new(sstate)))
    }
}

ffi_fn! {
    /// Processes a minidump with optional CFI information and returns the state
    /// of the process at the time of the crash.
    unsafe fn symbolic_process_minidump_buffer(
        buffer: *const c_char,
        length: usize,
        frame_info_map: *const SymbolicFrameInfoMap,
    ) -> Result<*mut SymbolicProcessState> {
        let bytes = slice::from_raw_parts(buffer as *const u8, length);
        let byteview = ByteView::from_slice(bytes);
        let map = if frame_info_map.is_null() {
            None
        } else {
            Some(SymbolicFrameInfoMap::as_rust(frame_info_map))
        };

        let state = ProcessState::from_minidump(&byteview, map)?;
        let sstate = map_process_state(&state);
        Ok(Box::into_raw(Box::new(sstate)))
    }
}

ffi_fn! {
    /// Frees a process state object.
    unsafe fn symbolic_process_state_free(process_state: *mut SymbolicProcessState) {
        if !process_state.is_null() {
            Box::from_raw(process_state);
        }
    }
}

ffi_fn! {
    /// Extracts call frame information (CFI) from an Object.
    unsafe fn symbolic_cficache_from_object(
        object: *const SymbolicObject,
    ) -> Result<*mut SymbolicCfiCache> {
        let object = SymbolicObject::as_rust(object).get();
        let cache = CfiCache::from_object(object)?;
        Ok(SymbolicCfiCache::from_rust(cache))
    }
}

ffi_fn! {
    /// Loads a CFI cache from the given path.
    unsafe fn symbolic_cficache_open(path: *const c_char) -> Result<*mut SymbolicCfiCache> {
        let byteview = ByteView::open(CStr::from_ptr(path).to_str()?)?;
        let cache = CfiCache::from_bytes(byteview)?;
        Ok(SymbolicCfiCache::from_rust(cache))
    }
}

ffi_fn! {
    /// Returns the file format version of the CFI cache.
    unsafe fn symbolic_cficache_get_version(cache: *const SymbolicCfiCache) -> Result<u32> {
        Ok(SymbolicCfiCache::as_rust(cache).version())
    }
}

ffi_fn! {
    /// Returns a pointer to the raw buffer of the CFI cache.
    unsafe fn symbolic_cficache_get_bytes(cache: *const SymbolicCfiCache) -> Result<*const u8> {
        Ok(SymbolicCfiCache::as_rust(cache).as_slice().as_ptr())
    }
}

ffi_fn! {
    /// Returns the size of the raw buffer of the CFI cache.
    unsafe fn symbolic_cficache_get_size(cache: *const SymbolicCfiCache) -> Result<usize> {
        Ok(SymbolicCfiCache::as_rust(cache).as_slice().len())
    }
}

ffi_fn! {
    /// Releases memory held by an unmanaged `SymbolicCfiCache` instance.
    unsafe fn symbolic_cficache_free(cache: *mut SymbolicCfiCache) {
        SymbolicCfiCache::drop(cache);
    }
}

ffi_fn! {
    /// Returns the latest CFI cache version.
    unsafe fn symbolic_cficache_latest_version() -> Result<u32> {
        Ok(CFICACHE_LATEST_VERSION)
    }
}
