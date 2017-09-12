use std::fmt;

use mach_object;

use errors::{ErrorKind, Result};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum Endianity {
    Little,
    Big,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum CpuFamily {
    Pentium,
    Arm,
    Unknown,
}

/// An enum of supported architectures.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
#[allow(non_camel_case_types)]
pub enum Arch {
    X86,
    X86_64,
    Arm64,
    ArmV5,
    ArmV6,
    ArmV7,
    ArmV7f,
    ArmV7s,
    ArmV7k,
    ArmV7m,
    ArmV7em,
}

impl Arch {
    /// Constructs an architecture from mach CPU types
    pub fn from_mach(cputype: u32, cpusubtype: u32) -> Result<Arch> {
        let ty = cputype as i32;
        let subty = cpusubtype as i32;
        if let Some(arch) = mach_object::get_arch_name_from_types(ty, subty) {
            Arch::parse(arch)
        } else {
            Err(ErrorKind::ParseError("unknown architecture").into())
        }
    }

    /// Constructs an architecture from ELF flags
    pub fn from_elf(machine: u16) -> Result<Arch> {
        use goblin::elf::header::*;
        Ok(match machine {
            EM_386 => Arch::X86,
            EM_X86_64 => Arch::X86_64,
            // FIXME: This is incorrect! ARM information is located in the .ARM.attributes section
            EM_ARM => Arch::ArmV7,
            _ => return Err(ErrorKind::ParseError("unknown architecture").into()),
        })
    }

    /// Parses an architecture from a string.
    pub fn parse(string: &str) -> Result<Arch> {
        use Arch::*;
        Ok(match string {
            "x86" => X86,
            "x86_64" => X86_64,
            "arm64" => Arm64,
            "armv5" => ArmV5,
            "armv6" => ArmV6,
            "armv7" => ArmV7,
            "armv7f" => ArmV7f,
            "armv7s" => ArmV7s,
            "armv7k" => ArmV7k,
            "armv7m" => ArmV7m,
            "armv7em" => ArmV7em,
            _ => {
                return Err(ErrorKind::ParseError("unknown architecture").into());
            }
        })
    }

    /// Returns the CPU family
    pub fn cpu_family(&self) -> CpuFamily {
        use Arch::*;
        match *self {
            X86 | X86_64 => CpuFamily::Pentium,
            Arm64 | ArmV5 | ArmV6 | ArmV7 | ArmV7f | ArmV7s | ArmV7k | ArmV7m | ArmV7em => {
                CpuFamily::Arm
            }
        }
    }

    /// Returns the native pointer size
    pub fn pointer_size(&self) -> Option<usize> {
        use Arch::*;
        match *self {
            X86_64 | Arm64 => Some(8),
            X86 | ArmV5 | ArmV6 | ArmV7 | ArmV7f | ArmV7s | ArmV7k | ArmV7m | ArmV7em => Some(4),
        }
    }
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Arch::*;
        write!(
            f,
            "{}",
            match *self {
                X86 => "x86",
                X86_64 => "x86_64",
                Arm64 => "arm64",
                ArmV5 => "armv5",
                ArmV6 => "armv6",
                ArmV7 => "armv7",
                ArmV7f => "armv7f",
                ArmV7s => "armv7s",
                ArmV7k => "armv7k",
                ArmV7m => "armv7m",
                ArmV7em => "armv7em",
            }
        )
    }
}
