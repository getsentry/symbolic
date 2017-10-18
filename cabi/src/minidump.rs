use std::ffi::CStr;
use std::mem;
use std::os::raw::c_char;
use uuid::Uuid;

use symbolic_common::ByteView;
use symbolic_minidump::{CallStack, CodeModuleId, FrameInfoMap, ProcessState, StackFrame};

use core::SymbolicUuid;

/// Contains stack frame information (CFI) for images
#[repr(C)]
pub struct SymbolicFrameInfoMap;


/// Indicates how well the instruction pointer derived during stack walking is trusted
#[repr(C)]
pub enum SymbolicFrameTrust {
    None,
    Scan,
    CFIScan,
    FP,
    CFI,
    Prewalked,
    Context,
}

/// Contains the absolute instruction address and image information of a stack frame
#[repr(C)]
pub struct SymbolicStackFrame {
    pub instruction: u64,
    pub trust: SymbolicFrameTrust,
    pub image_uuid: SymbolicUuid,
    pub image_addr: u64,
    pub image_size: u64,
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

/// State of a crashed process
#[repr(C)]
pub struct SymbolicProcessState {
    pub threads: *mut SymbolicCallStack,
    pub thread_count: usize,
}

impl Drop for SymbolicProcessState {
    fn drop(&mut self) {
        unsafe {
            Vec::from_raw_parts(self.threads, self.thread_count, self.thread_count);
        }
    }
}

/// Maps a native UUID to the FFI type
unsafe fn map_uuid(uuid: &Uuid) -> SymbolicUuid {
    mem::transmute(*uuid.as_bytes())
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

/// Maps a `StackFrame` to its FFI type
unsafe fn map_stack_frame(frame: &StackFrame) -> SymbolicStackFrame {
    SymbolicStackFrame {
        instruction: frame.instruction(),
        trust: mem::transmute(frame.trust()),
        image_uuid: map_uuid(&frame.module().map_or(Uuid::nil(), |m| m.id().uuid())),
        image_addr: frame.module().map_or(0, |m| m.base_address()),
        image_size: frame.module().map_or(0, |m| m.size()),
    }
}

/// Maps a `CallStack` to its FFI type
unsafe fn map_call_stack(stack: &CallStack) -> SymbolicCallStack {
    let (frames, frame_count) = map_slice(stack.frames(), |f| map_stack_frame(f));
    SymbolicCallStack {
        thread_id: stack.thread_id(),
        frames,
        frame_count,
    }
}

/// Maps a `ProcessState` to its FFI type
unsafe fn map_process_state(state: &ProcessState) -> SymbolicProcessState {
    let (threads, thread_count) = map_slice(state.threads(), |s| map_call_stack(s));
    SymbolicProcessState {
        threads,
        thread_count,
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
            let map = smap as *mut FrameInfoMap<'static>;
            Box::from_raw(map);
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

        let state = ProcessState::from_minidump(byteview, map)?;
        let sstate = map_process_state(&state);

        Ok(Box::into_raw(Box::new(sstate)) as *mut SymbolicProcessState)
    }
}

ffi_fn! {
    /// Frees a process state object
    unsafe fn symbolic_process_state_free(sstate: *mut SymbolicProcessState) {
        if !sstate.is_null() {
            let state = sstate as *mut ProcessState<'static>;
            Box::from_raw(state);
        }
    }
}
