use symbolic_common::{Arch, CpuFamily};

const SIGILL: u32 = 4;
const SIGBUS: u32 = 10;
const SIGSEGV: u32 = 11;

/// Helper to work with instruction addresses.
///
/// The most useful function is `InstructionInfo::caller_address` which applies
/// some heuristics to determine the call site of a function call based on the
/// return address. See `InstructionInfo::caller_address` for more information.
///
/// See https://goo.gl/g17EAn for detailed information on this topic.
pub struct InstructionInfo {
    /// The address of the instruction we want to use as a base.
    pub addr: u64,
    /// The architecture we are dealing with.
    pub arch: Arch,
    /// This is true if the frame is the cause of the crash.
    pub crashing_frame: bool,
    /// If a signal is known that triggers the crash, it can be stored here.
    pub signal: Option<u32>,
    /// The optional value of the IP register.
    pub ip_reg: Option<u64>,
}

impl InstructionInfo {
    /// Tries to resolve the start address of the current instruction.
    ///
    /// For architectures without fixed alignment (such as Intel with variable
    /// instruction lengths), this will return the same address. Otherwise, the
    /// address is aligned to the architecture's instruction alignment.
    pub fn aligned_address(&self) -> u64 {
        if let Some(alignment) = self.arch.instruction_alignment() {
            self.addr - (self.addr % alignment)
        } else {
            self.addr
        }
    }

    /// Return the previous instruction to the current one if we can
    /// determine this for the current architecture.
    /// Returns the instruction preceding the current one.
    ///
    /// For known architectures, this will return the start address of the
    /// instruction immediately before the current one in the machine code.
    /// This is likely the instruction that was just executed or that called
    /// a function returning at the current address.
    ///
    /// For unknown architectures or those using variable instruction size, the
    /// exact start address cannot be determined. Instead, an address *within*
    /// the preceding instruction will be returned. For this reason, the return
    /// value of this function should be considered an upper bound.
    pub fn previous_address(&self) -> u64 {
        self.aligned_address() - self.arch.instruction_alignment().unwrap_or(1)
    }

    /// Returns whether the application attempted to jump to an invalid,
    /// privileged or misaligned address. This indicates, that certain
    /// adjustments should be made on the caller instruction address.
    pub fn is_crash_signal(&self) -> bool {
        match self.signal {
            Some(SIGILL) | Some(SIGBUS) | Some(SIGSEGV) => true,
            _ => false,
        }
    }

    /// Determines whether the given address should be adjusted to resolve the
    /// call site of a stack frame.
    ///
    /// This generally applies to all frames except the crashing / suspended
    /// frame. However, if the process crashed with an illegal instruction,
    /// even the top-most frame needs to be adjusted to account for the signal
    /// handler.
    pub fn should_adjust_caller(&self) -> bool {
        // All frames other than the crashing frame (or suspended frame for
        // other threads) report the return address. This address (generally)
        // points to the instruction after the function call. Therefore, we
        // need to adjust the caller address for these frames.
        if !self.crashing_frame {
            return true;
        }

        // The crashing frame usually contains the actual register contents,
        // which points to the exact instruction that crashed and must not be
        // adjusted. A notable exception to this is if we crashed with one of
        // the crash signals.
        // TODO: Document reason of this
        if let Some(ip) = self.ip_reg {
            if ip != self.addr && self.is_crash_signal() {
                return true;
            }
        }

        return false;
    }

    /// Give the information in the instruction info this returns the
    /// most accurate instruction.
    pub fn caller_address(&self) -> u64 {
        let addr = if self.should_adjust_caller() {
            self.previous_address()
        } else {
            self.aligned_address()
        };

        // For some crashes on ARM, the aligned address resolved to the wrong
        // symbol.
        // TODO: Document details for this
        match self.arch.cpu_family() {
            CpuFamily::Arm32 => addr + 1,
            CpuFamily::Arm64 => addr + 3,
            _ => addr,
        }
    }
}
