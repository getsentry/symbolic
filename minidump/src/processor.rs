use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CStr, CString};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::os::raw::{c_char, c_void};
use std::str::FromStr;
use std::{fmt, ptr, slice, str};

use failure::Fail;
use lazy_static::lazy_static;
use regex::Regex;

use symbolic_common::{Arch, ByteView, CpuFamily, DebugId, ParseDebugIdError, Uuid};

use crate::cfi::CfiCache;
use crate::utils;

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

    fn process_minidump(
        buffer: *const c_char,
        buffer_size: usize,
        symbols: *const SymbolEntry,
        symbol_count: usize,
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

/// An error returned when parsing invalid `CodeModuleId`s.
pub type ParseCodeModuleIdError = ParseDebugIdError;

/// Breakpad code module IDs.
///
/// **Example:**
///
/// ```
/// use std::str::FromStr;
/// use symbolic_minidump::processor::CodeModuleId;
/// # use symbolic_minidump::processor::ParseCodeModuleIdError;
///
/// # fn foo() -> Result<(), ParseCodeModuleIdError> {
/// let id = CodeModuleId::from_str("DFB8E43AF2423D73A453AEB6A777EF75a")?;
/// assert_eq!("DFB8E43AF2423D73A453AEB6A777EF75a".to_string(), id.to_string());
/// # Ok(())
/// # }
///
/// # fn main() { foo().unwrap() }
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

impl Into<DebugId> for CodeModuleId {
    fn into(self) -> DebugId {
        self.inner
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

// TODO(ja): Enable this again
// #[cfg(feature = "with_serde")]
// serde_plain::derive_deserialize_from_str!(CodeModuleId, "CodeModuleId");

// #[cfg(feature = "with_serde")]
// serde_plain::derive_serialize_from_display!(CodeModuleId);

/// Carries information about a code module loaded into the process during the
/// crash. The `debug_identifier` uniquely identifies this module.
#[repr(C)]
pub struct CodeModule(c_void);

impl CodeModule {
    /// Returns the unique identifier of this `CodeModule`.
    pub fn id(&self) -> Option<CodeModuleId> {
        CodeModuleId::from_str(&self.debug_identifier()).ok()
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
    /// This may contain a UUID, timestamp, version number, or any combination of this or other
    /// information, in an implementation-defined format.
    pub fn code_identifier(&self) -> String {
        unsafe {
            let ptr = code_module_code_identifier(self);
            utils::ptr_to_string(ptr)
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
        unsafe {
            let ptr = code_module_debug_identifier(self);
            utils::ptr_to_string(ptr)
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
#[repr(u32)]
#[derive(Debug)]
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

    // Return the actual return address, as saved on the stack or in a
    // register. See the comments for `StackFrame::instruction' for
    // details.
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
            let mut size = 0 as usize;
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

/// Represents a thread of the `ProcessState` which holds a list of `StackFrame`s.
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
            let mut size = 0 as usize;
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
            if let Some(captures) = LINUX_BUILD_RE.captures(&build) {
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
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
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
#[derive(Debug, Fail, Copy, Clone)]
#[fail(display = "minidump processing failed: {}", _0)]
pub struct ProcessMinidumpError(ProcessResult);

impl ProcessMinidumpError {
    /// Returns the kind of this error.
    pub fn kind(self) -> ProcessResult {
        self.0
    }
}

/// Internal type used to transfer Breakpad symbols over FFI.
#[repr(C)]
struct SymbolEntry {
    debug_identifier: *const c_char,
    symbol_size: usize,
    symbol_data: *const u8,
}

/// Container for call frame information (CFI) of `CodeModules`.
///
/// This information is required by the stackwalker in case framepointers are
/// missing in the raw stacktraces. Frame information is given as plain ASCII
/// text as specified in the Breakpad symbol file specification.
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
        let cfi_count = frame_infos.map_or(0, |s| s.len());
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
            process_minidump(
                buffer.as_ptr() as *const c_char,
                buffer.len(),
                cfi_entries.as_ptr(),
                cfi_count,
                &mut result,
            )
        };

        if result == ProcessResult::Ok && !internal.is_null() {
            Ok(ProcessState {
                internal,
                _ty: PhantomData,
            })
        } else {
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
            let mut size = 0 as usize;
            let data = process_state_threads(self.internal, &mut size);
            slice::from_raw_parts(data as *const &CallStack, size)
        }
    }

    /// Returns the full list of loaded `CodeModule`s.
    pub fn modules(&self) -> Vec<&CodeModule> {
        unsafe {
            let mut size = 0 as usize;
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
