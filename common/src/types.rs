use std::mem;
use std::fmt;

#[cfg(feature = "with_objects")]
use mach_object;
#[cfg(feature = "with_dwarf")]
use gimli;

use errors::{ErrorKind, Result};

/// Represents endianess.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum Endianness {
    Little,
    Big,
}

impl Default for Endianness {
    #[cfg(target_endian = "little")]
    #[inline]
    fn default() -> Endianness {
        Endianness::Little
    }

    #[cfg(target_endian = "big")]
    #[inline]
    fn default() -> Endianness {
        Endianness::Big
    }
}

#[cfg(feature = "with_dwarf")]
impl gimli::Endianity for Endianness {
    #[inline]
    fn is_big_endian(self) -> bool {
        self == Endianness::Big
    }
}

/// Represents a family of CPUs
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum CpuFamily {
    Intel32,
    Intel64,
    Arm32,
    Arm64,
    Unknown,
}

/// An enum of supported architectures.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
#[allow(non_camel_case_types)]
#[repr(u32)]
pub enum Arch {
    Unknown,
    X86,
    X86_64,
    ArmV5,
    ArmV6,
    ArmV7,
    ArmV7f,
    ArmV7s,
    ArmV7k,
    ArmV7m,
    ArmV7em,
    Arm64,
    #[doc(hidden)]
    __Max
}

impl Default for Arch {
    fn default() -> Arch {
        Arch::Unknown
    }
}

impl Arch {
    /// Creates an arch from the u32 it represents
    pub fn from_u32(val: u32) -> Result<Arch> {
        if val >= (Arch::__Max as u32) {
            Err(ErrorKind::Parse("unknown architecture").into())
        } else {
            Ok(unsafe { mem::transmute(val) })
        }
    }

    /// Constructs an architecture from mach CPU types
    #[cfg(feature = "with_objects")]
    pub fn from_mach(cputype: u32, cpusubtype: u32) -> Result<Arch> {
        let ty = cputype as i32;
        let subty = cpusubtype as i32;
        if let Some(arch) = mach_object::get_arch_name_from_types(ty, subty) {
            Arch::parse(arch)
        } else {
            Err(ErrorKind::Parse("unknown architecture").into())
        }
    }

    /// Constructs an architecture from ELF flags
    #[cfg(feature = "with_objects")]
    pub fn from_elf(machine: u16) -> Result<Arch> {
        use goblin::elf::header::*;
        Ok(match machine {
            EM_386 => Arch::X86,
            EM_X86_64 => Arch::X86_64,
            // FIXME: This is incorrect! ARM information is located in the .ARM.attributes section
            EM_ARM => Arch::ArmV7,
            _ => return Err(ErrorKind::Parse("unknown architecture").into()),
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
                return Err(ErrorKind::Parse("unknown architecture").into());
            }
        })
    }

    /// Returns the CPU family
    pub fn cpu_family(&self) -> CpuFamily {
        use Arch::*;
        match *self {
            Unknown | __Max => CpuFamily::Unknown,
            X86 => CpuFamily::Intel32,
            X86_64 => CpuFamily::Intel64,
            Arm64 => CpuFamily::Arm64,
            ArmV5 | ArmV6 | ArmV7 | ArmV7f | ArmV7s | ArmV7k | ArmV7m | ArmV7em => CpuFamily::Arm32,
        }
    }

    /// Returns the native pointer size
    pub fn pointer_size(&self) -> Option<usize> {
        use Arch::*;
        match *self {
            Unknown | __Max => None,
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
                Unknown | __Max => "unknown",
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
