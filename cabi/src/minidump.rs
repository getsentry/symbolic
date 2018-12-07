use std::ffi::CStr;
use std::mem;
use std::os::raw::c_char;
use std::slice;
use std::str::FromStr;

use symbolic::common::{byteview::ByteView, types::Arch};
use symbolic::debuginfo::Object;
use symbolic::minidump::cfi::{CfiCache, CFICACHE_LATEST_VERSION};
use symbolic::minidump::processor::{
    CallStack, CodeModule, CodeModuleId, FrameInfoMap, ProcessState, RegVal, StackFrame, SystemInfo,
};

use crate::core::SymbolicStr;
use crate::debuginfo::SymbolicObject;

/// Contains stack frame information (CFI) for images.
pub struct SymbolicFrameInfoMap;

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
    pub id: SymbolicStr,
    pub addr: u64,
    pub size: u64,
    pub name: SymbolicStr,
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
    pub unsafe fn from_process_state(state: &ProcessState) -> Self {
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
        id: module
            .id()
            .map(|id| id.to_string().into())
            .unwrap_or_default(),
        addr: module.base_address(),
        size: module.size(),
        name: SymbolicStr::from_string(module.code_file()),
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
        id: "".into(),
        addr: 0,
        size: 0,
        name: "".into(),
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
unsafe fn map_process_state(state: &ProcessState) -> SymbolicProcessState {
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
        let map = Box::into_raw(Box::new(FrameInfoMap::new())) as *mut SymbolicFrameInfoMap;
        Ok(map)
    }
}

ffi_fn! {
    /// Adds the CfiCache for a module specified by the `sid` argument.
    unsafe fn symbolic_frame_info_map_add(
        smap: *const SymbolicFrameInfoMap,
        sid: *const SymbolicStr,
        cficache: *mut SymbolicCfiCache,
    ) -> Result<()> {
        let map = smap as *mut FrameInfoMap<'static>;
        let id = CodeModuleId::from_str((*sid).as_str())?;
        let cache = *Box::from_raw(cficache as *mut CfiCache<'static>);

        (*map).insert(id, cache);
        Ok(())
    }
}

ffi_fn! {
    /// Frees a frame info map object.
    unsafe fn symbolic_frame_info_map_free(smap: *mut SymbolicFrameInfoMap) {
        if !smap.is_null() {
            Box::from_raw(smap as *mut FrameInfoMap<'static>);
        }
    }
}

ffi_fn! {
    /// Processes a minidump with optional CFI information and returns the state
    /// of the process at the time of the crash.
    unsafe fn symbolic_process_minidump(
        path: *const c_char,
        smap: *const SymbolicFrameInfoMap,
    ) -> Result<*mut SymbolicProcessState> {
        let byteview = ByteView::from_path(CStr::from_ptr(path).to_str()?)?;
        let map = if smap.is_null() {
            None
        } else {
            Some(&*(smap as *const FrameInfoMap<'static>))
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
        smap: *const SymbolicFrameInfoMap,
    ) -> Result<*mut SymbolicProcessState> {
        let bytes = slice::from_raw_parts(buffer as *const u8, length);
        let byteview = ByteView::from_slice(bytes);
        let map = if smap.is_null() {
            None
        } else {
            Some(&*(smap as *const FrameInfoMap<'static>))
        };

        let state = ProcessState::from_minidump(&byteview, map)?;
        let sstate = map_process_state(&state);
        Ok(Box::into_raw(Box::new(sstate)))
    }
}

ffi_fn! {
    /// Frees a process state object.
    unsafe fn symbolic_process_state_free(sstate: *mut SymbolicProcessState) {
        if !sstate.is_null() {
            Box::from_raw(sstate);
        }
    }
}

/// Represents a symbolic CFI cache.
pub struct SymbolicCfiCache;

ffi_fn! {
    /// Extracts call frame information (CFI) from an Object.
    unsafe fn symbolic_cficache_from_object(
        sobj: *const SymbolicObject,
    ) -> Result<*mut SymbolicCfiCache> {
        let cache = CfiCache::from_object(&*(sobj as *const Object))?;
        Ok(Box::into_raw(Box::new(cache)) as *mut SymbolicCfiCache)
    }
}

ffi_fn! {
    /// Loads a CFI cache from the given path.
    unsafe fn symbolic_cficache_from_path(path: *const c_char) -> Result<*mut SymbolicCfiCache> {
        let byteview = ByteView::from_path(CStr::from_ptr(path).to_str()?)?;
        let cache = CfiCache::from_bytes(byteview)?;
        Ok(Box::into_raw(Box::new(cache)) as *mut SymbolicCfiCache)
    }
}

ffi_fn! {
    /// Returns the file format version of the CFI cache.
    unsafe fn symbolic_cficache_get_version(scache: *const SymbolicCfiCache) -> Result<u32> {
        let cache = scache as *const CfiCache<'static>;
        Ok((*cache).version())
    }
}

ffi_fn! {
    /// Returns a pointer to the raw buffer of the CFI cache.
    unsafe fn symbolic_cficache_get_bytes(scache: *const SymbolicCfiCache) -> Result<*const u8> {
        let cache = scache as *const CfiCache<'static>;
        Ok((*cache).as_slice().as_ptr())
    }
}

ffi_fn! {
    /// Returns the size of the raw buffer of the CFI cache.
    unsafe fn symbolic_cficache_get_size(scache: *const SymbolicCfiCache) -> Result<usize> {
        let cache = scache as *const CfiCache<'static>;
        Ok((*cache).as_slice().len())
    }
}

ffi_fn! {
    /// Releases memory held by an unmanaged `SymbolicCfiCache` instance.
    unsafe fn symbolic_cficache_free(scache: *mut SymbolicCfiCache) {
        if !scache.is_null() {
            let cache = scache as *mut CfiCache<'static>;
            Box::from_raw(cache);
        }
    }
}

ffi_fn! {
    /// Returns the latest CFI cache version.
    unsafe fn symbolic_cficache_latest_version() -> Result<u32> {
        Ok(CFICACHE_LATEST_VERSION)
    }
}
