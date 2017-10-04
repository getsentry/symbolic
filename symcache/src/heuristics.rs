use symbolic_common::{Arch, CpuFamily};

const SIGILL: u32 = 4;
const SIGBUS: u32 = 10;
const SIGSEGV: u32 = 11;


/// Helper to determine best instructions.
pub struct InstructionInfo {
    /// The address of the instruction we want to use as a base.
    pub addr: u64,
    /// The architecture we are dealing with.
    pub arch: Arch,
    /// This is true if the frame is the cause of the crash.
    pub crashing_frame: bool,
    /// If a signal is know that triggers the crash, it can be stored here.
    pub signal: Option<u32>,
    /// The optional value of the IP register.
    pub ip_reg: Option<u64>,
}

impl InstructionInfo {
    /// Returns true if the signal indicates a crash.
    pub fn is_crash_signal(&self) -> bool {
        match self.signal {
            Some(SIGILL) | Some(SIGBUS) | Some(SIGSEGV) => true,
            _ => false
        }
    }

    /// Return the previous instruction to the current one if we can
    /// determine this for the current architecture.
    pub fn get_previous_instruction(&self) -> Option<u64> {
        match self.arch.cpu_family() {
            CpuFamily::Arm64 => Some(self.addr - (self.addr % 4) - 4),
            CpuFamily::Arm32 => Some(self.addr - (self.addr % 2) - 2),
            _ => None
        }
    }

    /// Give the information in the instruction info this returns the
    /// most accurate instruction.
    pub fn find_best_instruction(&self) -> u64 {
        let mut prev = false;

        if !self.crashing_frame {
            prev = true;
        } else if let Some(ip) = self.ip_reg {
            if ip != self.addr && self.is_crash_signal() {
                prev = true;
            }
        }

        round_to_instruction_end(if prev {
            self.get_previous_instruction().unwrap_or(self.addr.saturating_sub(1))
        } else {
            self.addr
        }, self.arch)
    }
}

fn round_to_instruction_end(addr: u64, arch: Arch) -> u64 {
    match arch.cpu_family() {
        CpuFamily::Arm64 => addr - (addr % 4) + 3,
        CpuFamily::Arm32 => addr - (addr % 2) + 1,
        _ => addr
    }
}
