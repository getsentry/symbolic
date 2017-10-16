use std::fmt;
use std::borrow::Cow;
use std::ffi::CStr;
use std::marker::PhantomData;
use std::ops::Deref;
use std::os::raw::{c_char, c_int, c_void};

use symbolic_common::{ByteView, ErrorKind, Result};

use processor::StackFrame;

extern "C" {
    fn stack_frame_function_name(frame: *const StackFrame) -> *const c_char;
    fn stack_frame_source_file_name(frame: *const StackFrame) -> *const c_char;
    fn stack_frame_source_line(frame: *const StackFrame) -> c_int;
    fn stack_frame_delete(frame: *mut StackFrame);

    fn resolver_new(buffer: *const c_char, buffer_size: usize) -> *mut IResolver;
    fn resolver_delete(resolver: *mut IResolver);
    fn resolver_is_corrupt(resolver: *const IResolver) -> bool;
    fn resolver_resolve_frame(
        resolver: *const IResolver,
        frame: *const StackFrame,
    ) -> *mut StackFrame;
}

/// A resolved version of `StackFrame`. Contains source code locations and code
/// offsets, if the resolver was able to locate symbols for this frame.
/// Otherwise, the additional attributes are empty.
///
/// `ResolvedStackFrame` implements `Deref` for `StackFrame`, so that it can be used
/// interchangibly. See `StackFrame` for additional accessors.
pub struct ResolvedStackFrame {
    internal: *mut StackFrame,
}

impl ResolvedStackFrame {
    /// Creates a `ResolvedStackFrame` instance from a raw stack frame pointer.
    /// The pointer is assumed to be owned, and the underlying memory will be
    /// freed when this struct is dropped.
    pub(crate) fn from_ptr(internal: *mut StackFrame) -> ResolvedStackFrame {
        ResolvedStackFrame { internal }
    }

    /// Returns the function name that contains the instruction. Can be empty
    /// before running the `Resolver` or if debug symbols are missing.
    pub fn function_name(&self) -> Cow<str> {
        unsafe {
            let ptr = stack_frame_function_name(self.internal);
            CStr::from_ptr(ptr).to_string_lossy()
        }
    }

    /// Returns the source code line at which the instruction was declared.
    /// Can be empty before running the `Resolver` or if debug symbols are
    /// missing.
    pub fn source_file_name(&self) -> Cow<str> {
        unsafe {
            let ptr = stack_frame_source_file_name(self.internal);
            CStr::from_ptr(ptr).to_string_lossy()
        }
    }

    /// Returns the source code line at which the instruction was declared. Can
    /// be empty before running the `Resolver` or if debug symbols are missing.
    pub fn source_line(&self) -> c_int {
        unsafe { stack_frame_source_line(self.internal) }
    }
}

impl Deref for ResolvedStackFrame {
    type Target = StackFrame;

    fn deref(&self) -> &StackFrame {
        unsafe { &*self.internal }
    }
}

impl Drop for ResolvedStackFrame {
    fn drop(&mut self) {
        unsafe { stack_frame_delete(self.internal) };
    }
}

impl fmt::Debug for ResolvedStackFrame {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ResolvedStackFrame")
            .field("instruction", &self.instruction())
            .field("function_name", &self.function_name())
            .field("source_file_name", &self.source_file_name())
            .field("source_line", &self.source_line())
            .field("trust", &self.trust())
            .field("module", &self.module())
            .finish()
    }
}

type IResolver = c_void;

/// Source line resolver for stack frames. Handles Breakpad symbol files and
/// searches them for instructions.
///
/// To use this resolver, obtain a list of referenced modules from a
/// ProcessState and load all of them into the resolver. Once symbols have
/// been loaded for a `CodeModule`, the resolver can fill frames with source
/// line information.
///
/// See `ResolvedStackFrame` for all available information.
pub struct Resolver<'a> {
    internal: *mut IResolver,
    _ty: PhantomData<ByteView<'a>>,
}

impl<'a> Resolver<'a> {
    /// Creates a new `Resolver` instance from Breakpad symbols in a `ByteView`
    pub fn new(buffer: ByteView) -> Result<Resolver> {
        let internal = unsafe { resolver_new(buffer.as_ptr() as *const c_char, buffer.len()) };

        if internal.is_null() {
            Err(ErrorKind::Resolver("Could not load symbols".into()).into())
        } else {
            Ok(Resolver { internal, _ty: PhantomData })
        }
    }

    /// Returns whether this `Resolver` is corrupt or it can be used to
    /// resolve source line locations of `StackFrames`.
    pub fn corrupt(&self) -> bool {
        unsafe { resolver_is_corrupt(self.internal) }
    }

    /// Tries to locate the frame's instruction in the loaded code modules.
    /// Returns a resolved stack frame instance. If no  symbols can be found
    /// for the frame, a clone of the input is returned.
    pub fn resolve_frame(&self, frame: &StackFrame) -> ResolvedStackFrame {
        let ptr = unsafe { resolver_resolve_frame(self.internal, frame) };
        ResolvedStackFrame::from_ptr(ptr)
    }
}

impl<'a> Drop for Resolver<'a> {
    fn drop(&mut self) {
        unsafe { resolver_delete(self.internal) };
    }
}
