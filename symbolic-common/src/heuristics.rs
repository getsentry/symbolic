//! Heuristics for correcting instruction pointers based on the CPU architecture.

use crate::types::{Arch, CpuFamily};

const SIGILL: u32 = 4;
const SIGBUS: u32 = 10;
const SIGSEGV: u32 = 11;

/// Helper to work with instruction addresses.
///
/// Directly symbolicated stack traces may show the wrong calling symbols, as the stack frame's
/// return addresses point a few bytes past the original call site, which may place the address
/// within a different symbol entirely.
///
/// The most useful function is [`caller_address`], which applies some heuristics to determine the
/// call site of a function call based on the return address.
///
/// # Examples
///
/// ```
/// use symbolic_common::{Arch, InstructionInfo};
///
/// const SIGSEGV: u32 = 11;
///
/// let caller_address = InstructionInfo::new(Arch::Arm64, 0x1337)
///     .is_crashing_frame(false)
///     .signal(Some(SIGSEGV))
///     .ip_register_value(Some(0x4242))
///     .caller_address();
///
/// assert_eq!(caller_address, 0x1330);
/// ```
///
/// # Background
///
/// When *calling* a function, it is necessary for the *called* function to know where it should
/// return to upon completion. To support this, a *return address* is supplied as part of the
/// standard function call semantics. This return address specifies the instruction that the called
/// function should jump to upon completion of its execution.
///
/// When a crash reporter generates a backtrace, it first collects the thread state of all active
/// threads, including the **actual** current execution address. The reporter then iterates over
/// those threads, walking backwards to find calling frames – what it's actually finding during this
/// process are the **return addresses**. The actual address of the call instruction is not recorded
/// anywhere. The only address available is the address at which execution should resume after
/// function return.
///
/// To make things more complicated, there is no guarantee that a return address be set to exactly
/// one instruction after the call. It's entirely proper for a function to remove itself from the
/// call stack by setting a different return address entirely. This is why you never see
/// `objc_msgSend` in your backtrace unless you actually crash inside of `objc_msgSend`. When
/// `objc_msgSend` jumps to a method's implementation, it leaves its caller's return address in
/// place, and `objc_msgSend` itself disappears from the stack trace. In the case of `objc_msgSend`,
/// the loss of that information is of no great importance, but it's hardly the only function that
/// elides its own code from the return address.
///
/// # Heuristics
///
/// To resolve this particular issue, it is necessary for the symbolication implementor to apply a
/// per-architecture heuristics to the return addresses, and thus derive the **likely** address of
/// the actual calling instruction. There is a high probability of correctness, but absolutely no
/// guarantee.
///
/// This derived address **should** be used as the symbolication address, but **should not** replace
/// the return address in the crash report. This derived address is a best guess, and if you replace
/// the return address in the report, the end-user will have lost access to the original canonical
/// data from which they could have made their own assessment.
///
/// These heuristics must not be applied to frame #0 on any thread. The first frame of all threads
/// contains the actual register state of that thread at the time that it crashed (if it's the
/// crashing thread), or at the time it was suspended (if it is a non-crashing thread). These
/// heuristics should only be applied to frames *after* frame #0 – that is, starting with frame #1.
///
/// Additionally, these heuristics assume that your symbolication implementation correctly handles
/// addresses that occur within an instruction, rather than directly at the start of a valid
/// instruction. This should be the case for any reasonable implementation, but is something to be
/// aware of when deploying these changes.
///
/// ## x86 and x86-64
///
/// x86 uses variable-width instruction encodings; subtract one byte from the return address to
/// derive an address that should be within the calling instruction. This will provide an address
/// within a calling instruction found directly prior to the return address.
///
/// ## ARMv6 and ARMv7
///
/// - **Step 1:** Strip the low order thumb bit from the return address. ARM uses the low bit to
///   inform the processor that it should enter thumb mode when jumping to the return address. Since
///   all instructions are at least 2 byte aligned, an actual instruction address will never have
///   the low bit set.
///
/// - **Step 2:** Subtract 2 Bytes. 32-bit ARM instructions are either 2 or 4 bytes long, depending
///   on the use of thumb. This will place the symbolication address within the likely calling
///   instruction. All ARM64 instructions are 4 bytes long; subtract 4 bytes from the return address
///   to derive the likely address of the calling instruction.
///
/// # More Information
///
/// The above information was taken and slightly updated from the now-gone *PLCrashReporter Wiki*.
/// An old copy can still be found in the [internet archive].
///
/// [internet archive]: https://web.archive.org/web/20161012225323/https://opensource.plausible.coop/wiki/display/PLCR/Automated+Crash+Report+Analysis
/// [`caller_address`]: struct.InstructionInfo.html#method.caller_address
#[derive(Clone, Debug)]
pub struct InstructionInfo {
    addr: u64,
    arch: Arch,
    crashing_frame: bool,
    signal: Option<u32>,
    ip_reg: Option<u64>,
}

impl InstructionInfo {
    /// Creates a new instruction info instance.
    ///
    /// By default, the frame is not marked as *crashing frame*. The signal and instruction pointer
    /// register value are empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use symbolic_common::{Arch, InstructionInfo};
    ///
    /// let caller_address = InstructionInfo::new(Arch::X86, 0x1337)
    ///     .caller_address();
    /// ```
    pub fn new(arch: Arch, instruction_address: u64) -> Self {
        Self {
            arch,
            addr: instruction_address,
            crashing_frame: false,
            signal: None,
            ip_reg: None,
        }
    }

    /// Marks this as the crashing frame.
    ///
    /// The crashing frame is the first frame yielded by the stack walker. In such a frame, the
    /// instruction address is the location of the direct crash. This is used by
    /// [`should_adjust_caller`] to determine which frames need caller address adjustment.
    ///
    /// Defaults to `false`.
    ///
    /// [`should_adjust_caller`]: struct.InstructionInfo.html#method.should_adjust_caller
    pub fn is_crashing_frame(&mut self, flag: bool) -> &mut Self {
        self.crashing_frame = flag;
        self
    }

    /// Sets a POSIX signal number.
    ///
    /// The signal number is used by [`should_adjust_caller`] to determine which frames need caller
    /// address adjustment.
    ///
    /// [`should_adjust_caller`]: struct.InstructionInfo.html#method.should_adjust_caller
    pub fn signal(&mut self, signal: Option<u32>) -> &mut Self {
        self.signal = signal;
        self
    }

    /// Sets the value of the instruction pointer register.
    ///
    /// This should be the original register value at the time of the crash, and not a restored
    /// register value. This is used by [`should_adjust_caller`] to determine which frames need
    /// caller address adjustment.
    ///
    /// [`should_adjust_caller`]: struct.InstructionInfo.html#method.should_adjust_caller
    pub fn ip_register_value(&mut self, value: Option<u64>) -> &mut Self {
        self.ip_reg = value;
        self
    }

    /// Tries to resolve the start address of the current instruction.
    ///
    /// For architectures without fixed alignment (such as Intel with variable instruction lengths),
    /// this will return the same address. Otherwise, the address is aligned to the architecture's
    /// instruction alignment.
    ///
    /// # Examples
    ///
    /// For example, on 64-bit ARM, addresses are aligned at 4 byte boundaries. This applies to all
    /// 64-bit ARM variants, even unknown ones:
    ///
    /// ```
    /// use symbolic_common::{Arch, InstructionInfo};
    ///
    /// let info = InstructionInfo::new(Arch::Arm64, 0x1337);
    /// assert_eq!(info.aligned_address(), 0x1334);
    /// ```
    pub fn aligned_address(&self) -> u64 {
        if let Some(alignment) = self.arch.cpu_family().instruction_alignment() {
            self.addr - (self.addr % alignment)
        } else {
            self.addr
        }
    }

    /// Returns the instruction preceding the current one.
    ///
    /// For known architectures, this will return the start address of the instruction immediately
    /// before the current one in the machine code. This is likely the instruction that was just
    /// executed or that called a function returning at the current address.
    ///
    /// For unknown architectures or those using variable instruction size, the exact start address
    /// cannot be determined. Instead, an address *within* the preceding instruction will be
    /// returned. For this reason, the return value of this function should be considered an upper
    /// bound.
    ///
    /// # Examples
    ///
    /// On 64-bit ARM, instructions have 4 bytes in size. The previous address is therefore 4 bytes
    /// before the start of the current instruction (returned by [`aligned_address`]):
    ///
    /// ```
    /// use symbolic_common::{Arch, InstructionInfo};
    ///
    /// let info = InstructionInfo::new(Arch::Arm64, 0x1337);
    /// assert_eq!(info.previous_address(), 0x1330);
    /// ```
    ///
    /// On the contrary, Intel uses variable-length instruction encoding. In such a case, the best
    /// effort is to subtract 1 byte and hope that it points into the previous instruction:
    ///
    /// ```
    /// use symbolic_common::{Arch, InstructionInfo};
    ///
    /// let info = InstructionInfo::new(Arch::X86, 0x1337);
    /// assert_eq!(info.previous_address(), 0x1336);
    /// ```
    ///
    /// [`aligned_address`]: struct.InstructionInfo.html#method.aligned_address
    pub fn previous_address(&self) -> u64 {
        let instruction_size = self.arch.cpu_family().instruction_alignment().unwrap_or(1);

        // In MIPS, the return address apparently often points two instructions after the the
        // previous program counter. On other architectures, just subtract one instruction.
        let pc_offset = match self.arch.cpu_family() {
            CpuFamily::Mips32 | CpuFamily::Mips64 => 2 * instruction_size,
            _ => instruction_size,
        };

        self.aligned_address() - pc_offset
    }

    /// Returns whether the application attempted to jump to an invalid, privileged or misaligned
    /// address.
    ///
    /// This indicates that certain adjustments should be made on the caller instruction address.
    ///
    /// # Example
    ///
    /// ```
    /// use symbolic_common::{Arch, InstructionInfo};
    ///
    /// const SIGSEGV: u32 = 11;
    ///
    /// let is_crash = InstructionInfo::new(Arch::X86, 0x1337)
    ///     .signal(Some(SIGSEGV))
    ///     .is_crash_signal();
    ///
    /// assert!(is_crash);
    /// ```
    pub fn is_crash_signal(&self) -> bool {
        matches!(self.signal, Some(SIGILL) | Some(SIGBUS) | Some(SIGSEGV))
    }

    /// Determines whether the given address should be adjusted to resolve the call site of a stack
    /// frame.
    ///
    /// This generally applies to all frames except the crashing / suspended frame. However, if the
    /// process crashed with an illegal instruction, even the top-most frame needs to be adjusted to
    /// account for the signal handler.
    ///
    /// # Examples
    ///
    /// By default, all frames need to be adjusted. There are only few exceptions to this rule: The
    /// crashing frame is the first frame yielded in the stack trace and specifies the actual
    /// instruction pointer address. Therefore, it does not need to be adjusted:
    ///
    /// ```
    /// use symbolic_common::{Arch, InstructionInfo};
    ///
    /// let should_adjust = InstructionInfo::new(Arch::X86, 0x1337)
    ///     .is_crashing_frame(true)
    ///     .should_adjust_caller();
    ///
    /// assert!(!should_adjust);
    /// ```
    pub fn should_adjust_caller(&self) -> bool {
        // All frames other than the crashing frame (or suspended frame for
        // other threads) report the return address. This address (generally)
        // points to the instruction after the function call. Therefore, we
        // need to adjust the caller address for these frames.
        if !self.crashing_frame {
            return true;
        }

        // KSCrash applies a heuristic to remove the signal handler frame from
        // the top of the stack trace, if the crash was caused by certain
        // signals. However, that means that the top-most frame contains a
        // return address just like any other and needs to be adjusted.
        if let Some(ip) = self.ip_reg {
            if ip != self.addr && self.is_crash_signal() {
                return true;
            }
        }

        // The crashing frame usually contains the actual register contents,
        // which points to the exact instruction that crashed and must not be
        // adjusted.
        false
    }

    /// Determines the address of the call site based on a return address.
    ///
    /// In the top most frame (often referred to as context frame), this is the value of the
    /// instruction pointer register. In all other frames, the return address is generally one
    /// instruction after the jump / call.
    ///
    /// This function actually resolves an address _within_ the call instruction rather than its
    /// beginning. Also, in some cases the top most frame has been modified by certain signal
    /// handlers or return optimizations. A set of heuristics tries to recover this for well-known
    /// cases.
    ///
    /// # Examples
    ///
    /// Returns the aligned address for crashing frames:
    ///
    /// ```
    /// use symbolic_common::{Arch, InstructionInfo};
    ///
    /// let caller_address = InstructionInfo::new(Arch::Arm64, 0x1337)
    ///     .is_crashing_frame(true)
    ///     .caller_address();
    ///
    /// assert_eq!(caller_address, 0x1334);
    /// ```
    ///
    /// For all other frames, it returns the previous address:
    ///
    /// ```
    /// use symbolic_common::{Arch, InstructionInfo};
    ///
    /// let caller_address = InstructionInfo::new(Arch::Arm64, 0x1337)
    ///     .is_crashing_frame(false)
    ///     .caller_address();
    ///
    /// assert_eq!(caller_address, 0x1330);
    /// ```
    pub fn caller_address(&self) -> u64 {
        if self.should_adjust_caller() {
            self.previous_address()
        } else {
            self.aligned_address()
        }

        // NOTE: Currently, we only provide stack traces from KSCrash and
        // Breakpad. Both already apply a set of heuristics while stackwalking
        // in order to fix return addresses. It seems that no further heuristics
        // are necessary at the moment.
    }
}
