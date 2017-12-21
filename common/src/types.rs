use std::borrow::Cow;
use std::fmt;
use std::mem;

#[cfg(feature = "with_dwarf")]
use gimli;

use errors::{ErrorKind, Result};

/// Represents endianness.
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
    Ppc32,
    Ppc64,
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
    Ppc,
    Ppc64,
    #[doc(hidden)] __Max,
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
    pub fn from_mach(cputype: u32, cpusubtype: u32) -> Arch {
        use goblin::mach::constants::cputype::*;
        match (cputype, cpusubtype) {
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
            (CPU_TYPE_POWERPC, CPU_SUBTYPE_POWERPC_ALL) => Arch::Ppc,
            (CPU_TYPE_POWERPC64, CPU_SUBTYPE_POWERPC_ALL) => Arch::Ppc64,
            _ => Arch::Unknown,
        }
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
            Arch::Ppc => (CPU_TYPE_POWERPC, CPU_SUBTYPE_POWERPC_ALL),
            Arch::Ppc64 => (CPU_TYPE_POWERPC64, CPU_SUBTYPE_POWERPC_ALL),
            _ => {
                return Err(ErrorKind::NotFound("Unknown architecture for macho").into());
            }
        };
        Ok((rv.0 as u32, rv.1 as u32))
    }

    /// Constructs an architecture from ELF flags
    #[cfg(feature = "with_objects")]
    pub fn from_elf(machine: u16) -> Arch {
        use goblin::elf::header::*;
        match machine {
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
            EM_PPC => Arch::Ppc,
            EM_PPC64 => Arch::Ppc64,
            _ => Arch::Unknown,
        }
    }

    /// Constructs an architecture from ELF flags
    #[cfg(feature = "with_objects")]
    pub fn from_breakpad(string: &str) -> Arch {
        use Arch::*;
        match string {
            "x86" => X86,
            "x86_64" => X86_64,
            "ppc" => Ppc,
            "ppc64" => Ppc64,
            _ => Unknown,
        }
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
            Ppc => CpuFamily::Ppc32,
            Ppc64 => CpuFamily::Ppc64,
        }
    }

    /// Returns the native pointer size
    pub fn pointer_size(&self) -> Option<usize> {
        use Arch::*;
        match *self {
            Unknown | __Max => None,
            X86_64 | X86_64h | Arm64 | Arm64V8 | Ppc64 => Some(8),
            X86 | Arm | ArmV5 | ArmV6 | ArmV6m | ArmV7 | ArmV7f | ArmV7s | ArmV7k |
                ArmV7m | ArmV7em | Ppc => Some(4),
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
            Ppc => "ppc",
            Ppc64 => "ppc64",
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
    __Max,
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
    #[cfg(feature = "with_dwarf")]
    pub fn from_dwarf_lang(lang: gimli::DwLang) -> Language {
        match lang {
            gimli::DW_LANG_C | gimli::DW_LANG_C11 |
            gimli::DW_LANG_C89 | gimli::DW_LANG_C99 => Language::C,
            gimli::DW_LANG_C_plus_plus | gimli::DW_LANG_C_plus_plus_03 |
            gimli::DW_LANG_C_plus_plus_11 |
            gimli::DW_LANG_C_plus_plus_14 => Language::Cpp,
            gimli::DW_LANG_D => Language::D,
            gimli::DW_LANG_Go => Language::Go,
            gimli::DW_LANG_ObjC => Language::ObjC,
            gimli::DW_LANG_ObjC_plus_plus => Language::ObjCpp,
            gimli::DW_LANG_Rust => Language::Rust,
            gimli::DW_LANG_Swift => Language::Swift,
            _ => Language::Unknown,
        }
    }

    /// Parses a language from its name
    pub fn parse(name: &str) -> Language {
        use Language::*;
        match name {
            "c" => C,
            "cpp" => Cpp,
            "d" => D,
            "go" => Go,
            "objc" => ObjC,
            "objcpp" => ObjCpp,
            "rust" => Rust,
            "swift" => Swift,
            _ => Unknown,
        }
    }

    /// Returns the name of the language
    pub fn name(&self) -> &'static str {
        use Language::*;
        match *self {
            Unknown | __Max => "unknown",
            C => "c",
            Cpp => "cpp",
            D => "d",
            Go => "go",
            ObjC => "objc",
            ObjCpp => "objcpp",
            Rust => "rust",
            Swift => "swift",
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Language::*;
        write!(f, "{}", match *self {
            Unknown | __Max => "unknown",
            C => "C",
            Cpp => "C++",
            D => "D",
            Go => "Go",
            ObjC => "Objective-C",
            ObjCpp => "Objective-C++",
            Rust => "Rust",
            Swift => "Swift",
        })
    }
}

/// Represents a potentially mangled symbol
#[derive(Debug, Eq, PartialEq, Hash)]
pub struct Name<'a> {
    string: Cow<'a, str>,
    lang: Option<Language>,
}

impl<'a> Name<'a> {
    /// Constructs a new mangled symbol
    pub fn new<S>(string: S) -> Name<'a>
    where
        S: Into<Cow<'a, str>>
    {
        Name {
            string: string.into(),
            lang: None,
        }
    }

    /// Constructs a new mangled symbol with known language
    pub fn with_language<S>(string: S, lang: Language) -> Name<'a>
    where
        S: Into<Cow<'a, str>>
    {
        let lang_opt = match lang {
            // Ignore unknown languages and apply heuristics instead
            Language::Unknown | Language::__Max => None,
            _ => Some(lang),
        };

        Name {
            string: string.into(),
            lang: lang_opt,
        }
    }

    /// The raw, mangled string of the symbol
    pub fn as_str(&self) -> &str {
        &self.string
    }

    /// The language of the mangled symbol
    pub fn language(&self) -> Option<Language> {
        self.lang
    }
}

impl<'a> AsRef<str> for Name<'a> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<'a> Into<String> for Name<'a> {
    fn into(self) -> String {
        self.string.into()
    }
}

impl<'a> Into<Cow<'a, str>> for Name<'a> {
    fn into(self) -> Cow<'a, str> {
        self.string
    }
}

impl<'a> fmt::Display for Name<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Represents the physical object file format.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
pub enum ObjectKind {
    Breakpad,
    Elf,
    MachO,
}

impl ObjectKind {
    /// Returns the name of the object kind.
    pub fn name(&self) -> &'static str {
        use ObjectKind::*;
        match *self {
            Breakpad => "breakpad",
            Elf => "elf",
            MachO => "macho",
        }
    }
}

/// Represents the designated use of the object file and hints at its contents.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
pub enum ObjectClass {
    /// There is no object class specified for this object file.
    None,

    /// The Relocatable file type is the format used for intermediate object
    /// files. It is a very compact format containing all its sections in one
    /// segment. The compiler and assembler usually create one Relocatable file
    /// for each source code file. By convention, the file name extension for
    /// this format is .o.
    Relocatable,

    /// The Executable file type is the format used by standard executable
    /// programs.
    Executable,

    /// The Library file type is for dynamic shared libraries. It contains
    /// some additional tables to support multiple modules. By convention, the
    /// file name extension for this format is .dylib, except for the main
    /// shared library of a framework, which does not usually have a file name
    /// extension.
    Library,

    /// The Dump file type is used to store core files, which are
    /// traditionally created when a program crashes. Core files store the
    /// entire address space of a process at the time it crashed. You can
    /// later run gdb on the core file to figure out why the crash occurred.
    Dump,

    /// The Debug file type designates files that store symbol information
    /// for a corresponding binary file.
    Debug,

    /// The Other type represents any valid object class that does not fit any
    /// of the other classes. These are mostly CPU or OS dependent, or unique
    /// to a single kind of object.
    Other,
}

impl ObjectClass {
    pub fn name(&self) -> &'static str {
        use ObjectClass::*;
        match *self {
            None => "none",
            Relocatable => "rel",
            Executable => "exe",
            Library => "lib",
            Dump => "dump",
            Debug => "dbg",
            Other => "other",
        }
    }

    pub fn parse(string: &str) -> Result<ObjectClass> {
        use ObjectClass::*;
        Ok(match string {
            "none" => None,
            "rel" => Relocatable,
            "exe" => Executable,
            "lib" => Library,
            "dump" => Dump,
            "dbg" => Debug,
            "other" => Other,
            _ => return Err(ErrorKind::Parse("unknown object class").into()),
        })
    }

    #[cfg(feature = "with_objects")]
    pub fn from_mach(mach_type: u32) -> ObjectClass {
        use goblin::mach::header::*;
        use ObjectClass::*;

        match mach_type {
            MH_OBJECT => Relocatable,
            MH_EXECUTE => Executable,
            MH_DYLIB => Library,
            MH_CORE => Dump,
            MH_DSYM => Debug,
            _ => Other,
        }
    }

    #[cfg(feature = "with_objects")]
    pub fn to_mach(&self) -> Result<u32> {
        use goblin::mach::header::*;
        use ObjectClass::*;

        Ok(match *self {
            Relocatable => MH_OBJECT,
            Executable => MH_EXECUTE,
            Library => MH_DYLIB,
            Dump => MH_CORE,
            Debug => MH_DSYM,
            _ => return Err(ErrorKind::NotFound("unknown file_type for MachO").into()),
        })
    }

    #[cfg(feature = "with_objects")]
    pub fn from_elf(elf_type: u16) -> ObjectClass {
        use goblin::elf::header::*;
        use ObjectClass::*;

        match elf_type {
            ET_NONE => None,
            ET_REL => Relocatable,
            ET_EXEC => Executable,
            ET_DYN => Library,
            ET_CORE => Dump,
            _ => Other,
        }
    }

    #[cfg(feature = "with_objects")]
    pub fn from_elf_full(elf_type: u16, has_interpreter: bool) -> ObjectClass {
        let class = ObjectClass::from_elf(elf_type);

        // When stripping debug information into a separate file with objcopy,
        // the eh_type field still reads ET_EXEC. However, the interpreter is
        // removed. Since an executable without interpreter does not make any
        // sense, we assume ``Debug`` in this case.
        if class == ObjectClass::Executable && !has_interpreter {
            ObjectClass::Debug
        } else {
            class
        }
    }

    #[cfg(feature = "with_objects")]
    pub fn to_elf(&self) -> Result<u16> {
        use goblin::elf::header::*;
        use ObjectClass::*;

        Ok(match *self {
            None => ET_NONE,
            Relocatable => ET_REL,
            Executable => ET_EXEC,
            Library => ET_DYN,
            Dump => ET_CORE,
            Debug => ET_EXEC,
            _ => return Err(ErrorKind::NotFound("unknown file_type for ELF").into()),
        })
    }
}

/// Represents the kind of debug information inside an object.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
pub enum DebugKind {
    Dwarf,
    Breakpad,
}

impl DebugKind {
    /// Returns the name of the object kind.
    pub fn name(&self) -> &'static str {
        use DebugKind::*;
        match *self {
            Dwarf => "dwarf",
            Breakpad => "breakpad",
        }
    }
}
