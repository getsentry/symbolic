use std::ffi::CStr;
use std::mem;
use std::os::raw::c_char;
use std::slice;
use uuid::Uuid;

use symbolic_common::{Arch, ByteView};
use symbolic_debuginfo::Object;
use symbolic_minidump::{BreakpadAsciiCfiWriter, CallStack, CodeModule, CodeModuleId, FrameInfoMap,
                        ProcessState, StackFrame, SystemInfo};

use core::{SymbolicStr, SymbolicUuid};
use debuginfo::SymbolicObject;

/// Contains stack frame information (CFI) for images
pub struct SymbolicFrameInfoMap;

/// Indicates how well the instruction pointer derived during stack walking is trusted
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

/// Carries information about a code module loaded into the process during the crash
#[repr(C)]
pub struct SymbolicCodeModule {
    pub uuid: SymbolicUuid,
    pub addr: u64,
    pub size: u64,
    pub name: SymbolicStr,
}

/// Contains the absolute instruction address and image information of a stack frame
#[repr(C)]
pub struct SymbolicStackFrame {
    pub return_address: u64,
    pub instruction: u64,
    pub trust: SymbolicFrameTrust,
    pub module: SymbolicCodeModule,
}

/// Represents a thread of the process state which holds a list of stack frames
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

/// OS and CPU information
#[repr(C)]
pub struct SymbolicSystemInfo {
    pub os_name: SymbolicStr,
    pub os_version: SymbolicStr,
    pub os_build: SymbolicStr,
    pub cpu_family: SymbolicStr,
    pub cpu_info: SymbolicStr,
    pub cpu_count: u32,
}

/// State of a crashed process
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

impl Drop for SymbolicProcessState {
    fn drop(&mut self) {
        unsafe {
            Vec::from_raw_parts(self.threads, self.thread_count, self.thread_count);
            Vec::from_raw_parts(self.modules, self.module_count, self.module_count);
        }
    }
}

/// Creates a packed array of mapped FFI elements from a slice
unsafe fn map_slice<T, S, F>(items: &[T], mut mapper: F) -> (*mut S, usize)
where
    F: FnMut(&T) -> S,
{
    let mut vec = Vec::with_capacity(items.len());
    for item in items {
        vec.push(mapper(item));
    }

    let ptr = mem::transmute(vec.as_ptr());
    let len = vec.len();

    mem::forget(vec);
    (ptr, len)
}

/// Creates a packed array of mapped FFI elements from an iterator
unsafe fn map_iter<T, S, I, F>(items: I, mut mapper: F) -> (*mut S, usize)
where
    I: Iterator<Item = T>,
    F: FnMut(&T) -> S,
{
    let mut vec = Vec::with_capacity(items.size_hint().0);
    for ref item in items {
        vec.push(mapper(item));
    }

    vec.shrink_to_fit();
    let ptr = mem::transmute(vec.as_ptr());
    let len = vec.len();

    mem::forget(vec);
    (ptr, len)
}

/// Maps a `CodeModule` to its FFI type
unsafe fn map_code_module(module: &CodeModule) -> SymbolicCodeModule {
    SymbolicCodeModule {
        uuid: module.id().uuid().into(),
        addr: module.base_address(),
        size: module.size(),
        name: SymbolicStr::from_string(module.code_file()),
    }
}

/// Maps a `StackFrame` to its FFI type
unsafe fn map_stack_frame(frame: &StackFrame, arch: Arch) -> SymbolicStackFrame {
    SymbolicStackFrame {
        instruction: frame.instruction(),
        return_address: frame.return_address(arch),
        trust: mem::transmute(frame.trust()),
        module: frame.module().map_or(mem::zeroed(), |m| map_code_module(m)),
    }
}

/// Maps a `CallStack` to its FFI type
unsafe fn map_call_stack(stack: &CallStack, arch: Arch) -> SymbolicCallStack {
    let (frames, frame_count) = map_slice(stack.frames(), |f| map_stack_frame(f, arch));
    SymbolicCallStack {
        thread_id: stack.thread_id(),
        frames,
        frame_count,
    }
}

/// Maps a `SystemInfo` to its FFI type
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

/// Maps a `ProcessState` to its FFI type
unsafe fn map_process_state(state: &ProcessState) -> SymbolicProcessState {
    let arch = state.system_info().cpu_arch();
    let (threads, thread_count) = map_slice(state.threads(), |s| map_call_stack(s, arch));
    let (modules, module_count) =
        map_iter(state.referenced_modules().iter(), |m| map_code_module(m));

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
    /// Creates a new frame info map
    unsafe fn symbolic_frame_info_map_new() -> Result<*mut SymbolicFrameInfoMap> {
        let map = Box::into_raw(Box::new(FrameInfoMap::new())) as *mut SymbolicFrameInfoMap;
        Ok(map)
    }
}

ffi_fn! {
    /// Adds CFI for a code module specified by the `suuid` argument
    unsafe fn symbolic_frame_info_map_add(
        smap: *const SymbolicFrameInfoMap,
        suuid: *const SymbolicUuid,
        path: *const c_char,
    ) -> Result<()> {
        let map = smap as *mut FrameInfoMap<'static>;
        let byteview = ByteView::from_path(CStr::from_ptr(path).to_str()?)?;
        let uuid = Uuid::from_bytes(&(*suuid).data[..]).unwrap_or(Uuid::nil());
        let id = CodeModuleId::from_uuid(uuid);

        (*map).insert(id, byteview);
        Ok(())
    }
}

ffi_fn! {
    /// Frees a frame info map object
    unsafe fn symbolic_frame_info_map_free(smap: *mut SymbolicFrameInfoMap) {
        if !smap.is_null() {
            Box::from_raw(smap as *mut FrameInfoMap<'static>);
        }
    }
}

ffi_fn! {
    /// Processes a minidump with optional CFI information and returns the state
    /// of the process at the time of the crash
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
        let sstate = map_process_state(&state);
        Ok(Box::into_raw(Box::new(sstate)))
    }
}

ffi_fn! {
    /// Processes a minidump with optional CFI information and returns the state
    /// of the process at the time of the crash
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
    /// Frees a process state object
    unsafe fn symbolic_process_state_free(sstate: *mut SymbolicProcessState) {
        if !sstate.is_null() {
            Box::from_raw(sstate);
        }
    }
}

#[repr(C)]
pub struct SymbolicCfiCache {
    pub bytes: *mut u8,
    pub len: usize,
}

impl Drop for SymbolicCfiCache {
    fn drop(&mut self) {
        unsafe {
            Vec::from_raw_parts(self.bytes, self.len, self.len);
        }
    }
}

ffi_fn! {
    unsafe fn symbolic_cfi_cache_from_object(
        sobj: *const SymbolicObject,
    ) -> Result<*mut SymbolicCfiCache> {
        let mut buffer = vec![] as Vec<u8>;

        {
            let mut writer = BreakpadAsciiCfiWriter::new(&mut buffer);
            writer.process(&*(sobj as *const Object))?;
        }

        buffer.shrink_to_fit();
        let bytes = mem::transmute(buffer.as_ptr());
        let len = buffer.len();
        mem::forget(buffer);

        let scache = SymbolicCfiCache { bytes, len };
        Ok(Box::into_raw(Box::new(scache)))
    }
}

ffi_fn! {
    unsafe fn symbolic_cfi_cache_free(scache: *mut SymbolicCfiCache) {
        if !scache.is_null() {
            Box::from_raw(scache);
        }
    }
}
