use std::mem;
use std::fmt;

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
    X86_64h,
    Arm,
    ArmV5,
    ArmV6,
    ArmV6m,
    ArmV7,
    ArmV7f,
    ArmV7s,
    ArmV7k,
    ArmV7m,
    ArmV7em,
    Arm64,
    Arm64V8,
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
        use goblin::mach::constants::cputype::*;
        Ok(match (cputype, cpusubtype) {
            (CPU_TYPE_I386, CPU_SUBTYPE_I386_ALL) => Arch::X86,
            (CPU_TYPE_X86_64, CPU_SUBTYPE_X86_64_ALL) => Arch::X86_64,
            (CPU_TYPE_X86_64, CPU_SUBTYPE_X86_64_H) => Arch::X86_64h,
            (CPU_TYPE_ARM64, CPU_SUBTYPE_ARM64_ALL) => Arch::Arm64,
            (CPU_TYPE_ARM64, CPU_SUBTYPE_ARM64_V8) => Arch::Arm64V8,
            (CPU_TYPE_ARM, CPU_SUBTYPE_ARM_ALL) => Arch::Arm,
            (CPU_TYPE_ARM, CPU_SUBTYPE_ARM_V5TEJ) => Arch::ArmV5,
            (CPU_TYPE_ARM, CPU_SUBTYPE_ARM_V6) => Arch::ArmV6,
            (CPU_TYPE_ARM, CPU_SUBTYPE_ARM_V6M) => Arch::ArmV6m,
            (CPU_TYPE_ARM, CPU_SUBTYPE_ARM_V7) => Arch::ArmV7,
            (CPU_TYPE_ARM, CPU_SUBTYPE_ARM_V7F) => Arch::ArmV7f,
            (CPU_TYPE_ARM, CPU_SUBTYPE_ARM_V7S) => Arch::ArmV7s,
            (CPU_TYPE_ARM, CPU_SUBTYPE_ARM_V7K) => Arch::ArmV7k,
            (CPU_TYPE_ARM, CPU_SUBTYPE_ARM_V7M) => Arch::ArmV7m,
            (CPU_TYPE_ARM, CPU_SUBTYPE_ARM_V7EM) => Arch::ArmV7em,
            _ => {
                return Err(ErrorKind::Parse("unknown architecture").into());
            }
        })
    }

    /// Returns the macho arch for this arch.
    #[cfg(feature = "with_objects")]
    pub fn to_mach(&self) -> Result<(u32, u32)> {
        use goblin::mach::constants::cputype::*;
        let rv = match *self {
            Arch::X86 => (CPU_TYPE_I386, CPU_SUBTYPE_I386_ALL),
            Arch::X86_64 => (CPU_TYPE_X86_64, CPU_SUBTYPE_X86_64_ALL),
            Arch::X86_64h => (CPU_TYPE_X86_64, CPU_SUBTYPE_X86_64_H),
            Arch::Arm64 => (CPU_TYPE_ARM64, CPU_SUBTYPE_ARM64_ALL),
            Arch::Arm64V8 => (CPU_TYPE_ARM64, CPU_SUBTYPE_ARM64_V8),
            Arch::Arm => (CPU_TYPE_ARM, CPU_SUBTYPE_ARM_ALL),
            Arch::ArmV5 => (CPU_TYPE_ARM, CPU_SUBTYPE_ARM_V5TEJ),
            Arch::ArmV6 => (CPU_TYPE_ARM, CPU_SUBTYPE_ARM_V6),
            Arch::ArmV6m => (CPU_TYPE_ARM, CPU_SUBTYPE_ARM_V6M),
            Arch::ArmV7 => (CPU_TYPE_ARM, CPU_SUBTYPE_ARM_V7),
            Arch::ArmV7f => (CPU_TYPE_ARM, CPU_SUBTYPE_ARM_V7F),
            Arch::ArmV7s => (CPU_TYPE_ARM, CPU_SUBTYPE_ARM_V7S),
            Arch::ArmV7k => (CPU_TYPE_ARM, CPU_SUBTYPE_ARM_V7K),
            Arch::ArmV7m => (CPU_TYPE_ARM, CPU_SUBTYPE_ARM_V7M),
            Arch::ArmV7em => (CPU_TYPE_ARM, CPU_SUBTYPE_ARM_V7EM),
            _ => {
                return Err(ErrorKind::NotFound("Unknown architecture for macho").into());
            }
        };
        Ok((rv.0 as u32, rv.1 as u32))
    }

    /// Constructs an architecture from ELF flags
    #[cfg(feature = "with_objects")]
    pub fn from_elf(machine: u16) -> Result<Arch> {
        use goblin::elf::header::*;
        Ok(match machine {
            EM_386 => Arch::X86,
            EM_X86_64 => Arch::X86_64,
            EM_AARCH64 => Arch::Arm64,
            // NOTE: This could actually be any of the other 32bit ARMs. Since we don't need this
            // information, we use the generic Arch::Arm. By reading CPU_arch and FP_arch attributes
            // from the SHT_ARM_ATTRIBUTES section it would be possible to distinguish the ARM arch
            // version and infer hard/soft FP.
            //
            // For more information, see:
            // http://code.metager.de/source/xref/gnu/src/binutils/readelf.c#11282
            // https://stackoverflow.com/a/20556156/4228225
            EM_ARM => Arch::Arm,
            _ => return Err(ErrorKind::Parse("unknown architecture").into()),
        })
    }

    /// Parses an architecture from a string.
    pub fn parse(string: &str) -> Result<Arch> {
        use Arch::*;
        Ok(match string {
            // this is an alias that is known among macho users
            "i386" => X86,
            "x86" => X86,
            "x86_64" => X86_64,
            "x86_64h" => X86_64h,
            "arm64" => Arm64,
            "arm64v8" => Arm64V8,
            "arm" => Arm,
            "armv5" => ArmV5,
            "armv6" => ArmV6,
            "armv6m" => ArmV6m,
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
            X86_64 | X86_64h => CpuFamily::Intel64,
            Arm64 | Arm64V8 => CpuFamily::Arm64,
            Arm | ArmV5 | ArmV6 | ArmV6m | ArmV7 | ArmV7f | ArmV7s | ArmV7k |
                ArmV7m | ArmV7em => CpuFamily::Arm32,
        }
    }

    /// Returns the native pointer size
    pub fn pointer_size(&self) -> Option<usize> {
        use Arch::*;
        match *self {
            Unknown | __Max => None,
            X86_64 | X86_64h | Arm64 | Arm64V8 => Some(8),
            X86 | Arm | ArmV5 | ArmV6 | ArmV6m | ArmV7 | ArmV7f | ArmV7s | ArmV7k |
                ArmV7m | ArmV7em => Some(4),
        }
    }

    /// Returns the name of the arch
    pub fn name(&self) -> &'static str {
        use Arch::*;
        match *self {
            Unknown | __Max => "unknown",
            X86 => "x86",
            X86_64 => "x86_64",
            X86_64h => "x86_64h",
            Arm64 => "arm64",
            Arm64V8 => "arm64V8",
            Arm => "arm",
            ArmV5 => "armv5",
            ArmV6 => "armv6",
            ArmV6m => "armv6m",
            ArmV7 => "armv7",
            ArmV7f => "armv7f",
            ArmV7s => "armv7s",
            ArmV7k => "armv7k",
            ArmV7m => "armv7m",
            ArmV7em => "armv7em",
        }
    }

    /// The name of the IP register if known.
    pub fn ip_reg_name(&self) -> Option<&'static str> {
        match self.cpu_family() {
            CpuFamily::Intel32 => Some("eip"),
            CpuFamily::Intel64 => Some("rip"),
            CpuFamily::Arm32 | CpuFamily::Arm64 => Some("pc"),
            _ => None,
        }
    }
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Supported programming languages for demangling
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
#[repr(u32)]
pub enum Language {
    Unknown,
    C,
    Cpp,
    D,
    Go,
    ObjC,
    ObjCpp,
    Rust,
    Swift,
    #[doc(hidden)]
    __Max
}

impl Language {
    /// Creates a language from the u32 it represents
    pub fn from_u32(val: u32) -> Result<Language> {
        if val >= (Language::__Max as u32) {
            Err(ErrorKind::Parse("unknown language").into())
        } else {
            Ok(unsafe { mem::transmute(val) })
        }
    }

    /// Converts a DWARF language tag into a supported language.
    #[cfg(feature="with_dwarf")]
    pub fn from_dwarf_lang(lang: gimli::DwLang) -> Option<Language> {
        match lang {
            gimli::DW_LANG_C | gimli::DW_LANG_C11 |
            gimli::DW_LANG_C89 | gimli::DW_LANG_C99 => Some(Language::C),
            gimli::DW_LANG_C_plus_plus | gimli::DW_LANG_C_plus_plus_03 |
            gimli::DW_LANG_C_plus_plus_11 |
            gimli::DW_LANG_C_plus_plus_14 => Some(Language::Cpp),
            gimli::DW_LANG_D => Some(Language::D),
            gimli::DW_LANG_Go => Some(Language::Go),
            gimli::DW_LANG_ObjC => Some(Language::ObjC),
            gimli::DW_LANG_ObjC_plus_plus => Some(Language::ObjCpp),
            gimli::DW_LANG_Rust => Some(Language::Rust),
            gimli::DW_LANG_Swift => Some(Language::Swift),
            _ => None,
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", match *self {
            Language::Unknown | Language::__Max => "unknown",
            Language::C => "C",
            Language::Cpp => "C++",
            Language::D => "D",
            Language::Go => "Go",
            Language::ObjC => "Objective-C",
            Language::ObjCpp => "Objective-C++",
            Language::Rust => "Rust",
            Language::Swift => "Swift"
        })
    }
}

/// Represents the kind of an object.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
pub enum ObjectKind {
    MachO,
    Elf,
}

impl ObjectKind {
    /// Returns the name of the object kind.
    pub fn name(&self) -> &'static str {
        match *self {
            ObjectKind::MachO => "macho",
            ObjectKind::Elf => "elf",
        }
    }
}
