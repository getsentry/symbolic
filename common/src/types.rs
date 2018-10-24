//! Common types and errors used in `symbolic`.

use std::borrow::Cow;
use std::fmt;
use std::mem;
use std::str;

#[cfg(feature = "with_dwarf")]
use gimli;

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

/// Represents a family of CPUs.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
#[repr(u32)]
pub enum CpuFamily {
    Unknown,
    Intel32,
    Intel64,
    Arm32,
    Arm64,
    Ppc32,
    Ppc64,
}

/// An error returned for unknown or invalid `Arch`s.
#[derive(Debug, Fail, Clone, Copy)]
#[fail(display = "unknown architecture")]
pub struct UnknownArchError;

/// An enum of supported architectures.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
#[allow(non_camel_case_types)]
#[repr(u32)]
pub enum Arch {
    Unknown = 0,
    X86 = 101,
    X86_64 = 201,
    X86_64h = 202,
    Arm = 301,
    ArmV5 = 302,
    ArmV6 = 303,
    ArmV6m = 304,
    ArmV7 = 305,
    ArmV7f = 306,
    ArmV7s = 307,
    ArmV7k = 308,
    ArmV7m = 309,
    ArmV7em = 310,
    Arm64 = 401,
    Arm64V8 = 402,
    Arm64e = 403,
    Ppc = 501,
    Ppc64 = 502,
}

impl Arch {
    /// Creates an arch from the u32 it represents.
    pub fn from_u32(val: u32) -> Result<Arch, UnknownArchError> {
        Ok(match val {
            0 => Arch::Unknown,
            1 | 101 => Arch::X86,
            2 | 201 => Arch::X86_64,
            3 | 202 => Arch::X86_64h,
            4 | 301 => Arch::Arm,
            5 | 302 => Arch::ArmV5,
            6 | 303 => Arch::ArmV6,
            7 | 304 => Arch::ArmV6m,
            8 | 305 => Arch::ArmV7,
            9 | 306 => Arch::ArmV7f,
            10 | 307 => Arch::ArmV7s,
            11 | 308 => Arch::ArmV7k,
            12 | 309 => Arch::ArmV7m,
            13 | 310 => Arch::ArmV7em,
            14 | 401 => Arch::Arm64,
            15 | 402 => Arch::Arm64V8,
            16 | 403 => Arch::Arm64e,
            17 | 501 => Arch::Ppc,
            18 | 502 => Arch::Ppc64,
            _ => return Err(UnknownArchError),
        })
    }

    /// Constructs an architecture from mach CPU types.
    #[cfg(feature = "with_objects")]
    pub fn from_mach(cputype: u32, cpusubtype: u32) -> Result<Arch, UnknownArchError> {
        use goblin::mach::constants::cputype::*;
        // from dyld-519.2.2
        const CPU_SUBTYPE_ARM64_E: u32 = 2;
        Ok(match (cputype, cpusubtype) {
            (CPU_TYPE_I386, CPU_SUBTYPE_I386_ALL) => Arch::X86,
            (CPU_TYPE_X86_64, CPU_SUBTYPE_X86_64_ALL) => Arch::X86_64,
            (CPU_TYPE_X86_64, CPU_SUBTYPE_X86_64_H) => Arch::X86_64h,
            (CPU_TYPE_ARM64, CPU_SUBTYPE_ARM64_ALL) => Arch::Arm64,
            (CPU_TYPE_ARM64, CPU_SUBTYPE_ARM64_V8) => Arch::Arm64V8,
            (CPU_TYPE_ARM64, CPU_SUBTYPE_ARM64_E) => Arch::Arm64e,
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
            (CPU_TYPE_ARM, _) => Arch::Arm,
            (CPU_TYPE_POWERPC, CPU_SUBTYPE_POWERPC_ALL) => Arch::Ppc,
            (CPU_TYPE_POWERPC64, CPU_SUBTYPE_POWERPC_ALL) => Arch::Ppc64,
            _ => return Err(UnknownArchError),
        })
    }

    /// Constructs an architecture from ELF flags.
    #[cfg(feature = "with_objects")]
    pub fn from_elf(machine: u16) -> Result<Arch, UnknownArchError> {
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
            EM_PPC => Arch::Ppc,
            EM_PPC64 => Arch::Ppc64,
            _ => return Err(UnknownArchError),
        })
    }

    /// Constructs an architecture from ELF flags.
    #[cfg(feature = "with_objects")]
    pub fn from_breakpad(string: &str) -> Result<Arch, UnknownArchError> {
        Ok(match string {
            "x86" => Arch::X86,
            // This is different in minidumps and breakpad symbols
            "x86_64" | "amd64" => Arch::X86_64,
            "arm" => Arch::Arm,
            "arm64" => Arch::Arm64,
            "ppc" => Arch::Ppc,
            "ppc64" => Arch::Ppc64,
            _ => return Err(UnknownArchError),
        })
    }

    /// Returns the breakpad name for this Arch.
    pub fn to_breakpad(self) -> &'static str {
        match self.cpu_family() {
            CpuFamily::Intel32 => "x86",
            // Use the breakpad symbol constant here
            CpuFamily::Intel64 => "x86_64",
            CpuFamily::Arm32 => "arm",
            CpuFamily::Arm64 => "arm64",
            CpuFamily::Ppc32 => "ppc",
            CpuFamily::Ppc64 => "ppc64",
            CpuFamily::Unknown => "unknown",
        }
    }

    /// Returns the CPU family.
    pub fn cpu_family(self) -> CpuFamily {
        match self {
            Arch::Unknown => CpuFamily::Unknown,
            Arch::X86 => CpuFamily::Intel32,
            Arch::X86_64 | Arch::X86_64h => CpuFamily::Intel64,
            Arch::Arm64 | Arch::Arm64V8 | Arch::Arm64e => CpuFamily::Arm64,
            Arch::Arm
            | Arch::ArmV5
            | Arch::ArmV6
            | Arch::ArmV6m
            | Arch::ArmV7
            | Arch::ArmV7f
            | Arch::ArmV7s
            | Arch::ArmV7k
            | Arch::ArmV7m
            | Arch::ArmV7em => CpuFamily::Arm32,
            Arch::Ppc => CpuFamily::Ppc32,
            Arch::Ppc64 => CpuFamily::Ppc64,
        }
    }

    /// Returns the native pointer size.
    pub fn pointer_size(self) -> Option<usize> {
        match self {
            Arch::Unknown => None,
            Arch::X86_64
            | Arch::X86_64h
            | Arch::Arm64
            | Arch::Arm64V8
            | Arch::Arm64e
            | Arch::Ppc64 => Some(8),
            Arch::X86
            | Arch::Arm
            | Arch::ArmV5
            | Arch::ArmV6
            | Arch::ArmV6m
            | Arch::ArmV7
            | Arch::ArmV7f
            | Arch::ArmV7s
            | Arch::ArmV7k
            | Arch::ArmV7m
            | Arch::ArmV7em
            | Arch::Ppc => Some(4),
        }
    }

    /// Returns the name of the arch.
    pub fn name(self) -> &'static str {
        match self {
            Arch::Unknown => "unknown",
            Arch::X86 => "x86",
            Arch::X86_64 => "x86_64",
            Arch::X86_64h => "x86_64h",
            Arch::Arm64 => "arm64",
            Arch::Arm64V8 => "arm64v8",
            Arch::Arm64e => "arm64e",
            Arch::Arm => "arm",
            Arch::ArmV5 => "armv5",
            Arch::ArmV6 => "armv6",
            Arch::ArmV6m => "armv6m",
            Arch::ArmV7 => "armv7",
            Arch::ArmV7f => "armv7f",
            Arch::ArmV7s => "armv7s",
            Arch::ArmV7k => "armv7k",
            Arch::ArmV7m => "armv7m",
            Arch::ArmV7em => "armv7em",
            Arch::Ppc => "ppc",
            Arch::Ppc64 => "ppc64",
        }
    }

    /// The name of the IP register if known.
    pub fn ip_register_name(self) -> Option<&'static str> {
        match self.cpu_family() {
            CpuFamily::Intel32 => Some("eip"),
            CpuFamily::Intel64 => Some("rip"),
            CpuFamily::Arm32 | CpuFamily::Arm64 => Some("pc"),
            CpuFamily::Ppc32 | CpuFamily::Ppc64 => Some("srr0"),
            CpuFamily::Unknown => None,
        }
    }

    /// Returns instruction alignment if fixed.
    pub fn instruction_alignment(self) -> Option<u64> {
        match self.cpu_family() {
            CpuFamily::Arm32 => Some(2),
            CpuFamily::Arm64 => Some(4),
            CpuFamily::Ppc32 => Some(4),
            CpuFamily::Ppc64 => Some(8),
            CpuFamily::Intel32 | CpuFamily::Intel64 => None,
            CpuFamily::Unknown => None,
        }
    }
}

impl Default for Arch {
    fn default() -> Arch {
        Arch::Unknown
    }
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl str::FromStr for Arch {
    type Err = UnknownArchError;

    fn from_str(string: &str) -> Result<Arch, UnknownArchError> {
        Ok(match string {
            // this is an alias that is known among macho users
            "i386" => Arch::X86,
            "x86" => Arch::X86,
            "x86_64" => Arch::X86_64,
            "x86_64h" => Arch::X86_64h,
            "arm64" => Arch::Arm64,
            "arm64v8" => Arch::Arm64V8,
            "arm64e" => Arch::Arm64e,
            "arm" => Arch::Arm,
            "armv5" => Arch::ArmV5,
            "armv6" => Arch::ArmV6,
            "armv6m" => Arch::ArmV6m,
            "armv7" => Arch::ArmV7,
            "armv7f" => Arch::ArmV7f,
            "armv7s" => Arch::ArmV7s,
            "armv7k" => Arch::ArmV7k,
            "armv7m" => Arch::ArmV7m,
            "armv7em" => Arch::ArmV7em,
            "ppc" => Arch::Ppc,
            "ppc64" => Arch::Ppc64,
            _ => return Err(UnknownArchError),
        })
    }
}

#[cfg(feature = "with_serde")]
derive_deserialize_from_str!(Arch, "Arch");

#[cfg(feature = "with_serde")]
derive_serialize_from_display!(Arch);

/// An error returned for unknown or invalid `Language`s.
#[derive(Debug, Fail, Clone, Copy)]
#[fail(display = "unknown language")]
pub struct UnknownLanguageError;

/// Supported programming languages for demangling.
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
    /// Creates a language from the u32 it represents.
    pub fn from_u32(val: u32) -> Result<Language, UnknownLanguageError> {
        if val >= (Language::__Max as u32) {
            Err(UnknownLanguageError)
        } else {
            Ok(unsafe { mem::transmute(val) })
        }
    }

    /// Converts a DWARF language tag into a supported language.
    #[cfg(feature = "with_dwarf")]
    pub fn from_dwarf_lang(lang: gimli::DwLang) -> Result<Language, UnknownLanguageError> {
        Ok(match lang {
            gimli::DW_LANG_C | gimli::DW_LANG_C11 | gimli::DW_LANG_C89 | gimli::DW_LANG_C99 => {
                Language::C
            }
            gimli::DW_LANG_C_plus_plus
            | gimli::DW_LANG_C_plus_plus_03
            | gimli::DW_LANG_C_plus_plus_11
            | gimli::DW_LANG_C_plus_plus_14 => Language::Cpp,
            gimli::DW_LANG_D => Language::D,
            gimli::DW_LANG_Go => Language::Go,
            gimli::DW_LANG_ObjC => Language::ObjC,
            gimli::DW_LANG_ObjC_plus_plus => Language::ObjCpp,
            gimli::DW_LANG_Rust => Language::Rust,
            gimli::DW_LANG_Swift => Language::Swift,
            _ => return Err(UnknownLanguageError),
        })
    }

    /// Returns the name of the language.
    pub fn name(self) -> &'static str {
        match self {
            Language::Unknown | Language::__Max => "unknown",
            Language::C => "c",
            Language::Cpp => "cpp",
            Language::D => "d",
            Language::Go => "go",
            Language::ObjC => "objc",
            Language::ObjCpp => "objcpp",
            Language::Rust => "rust",
            Language::Swift => "swift",
        }
    }
}

impl Default for Language {
    fn default() -> Language {
        Language::Unknown
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let formatted = match *self {
            Language::Unknown | Language::__Max => "unknown",
            Language::C => "C",
            Language::Cpp => "C++",
            Language::D => "D",
            Language::Go => "Go",
            Language::ObjC => "Objective-C",
            Language::ObjCpp => "Objective-C++",
            Language::Rust => "Rust",
            Language::Swift => "Swift",
        };

        write!(f, "{}", formatted)
    }
}

impl str::FromStr for Language {
    type Err = UnknownLanguageError;

    fn from_str(string: &str) -> Result<Language, UnknownLanguageError> {
        Ok(match string {
            "c" => Language::C,
            "cpp" => Language::Cpp,
            "d" => Language::D,
            "go" => Language::Go,
            "objc" => Language::ObjC,
            "objcpp" => Language::ObjCpp,
            "rust" => Language::Rust,
            "swift" => Language::Swift,
            _ => return Err(UnknownLanguageError),
        })
    }
}

#[cfg(feature = "with_serde")]
derive_deserialize_from_str!(Language, "Language");

#[cfg(feature = "with_serde")]
derive_serialize_from_display!(Language);

/// Represents a potentially mangled symbol.
#[derive(Debug, Eq, PartialEq, Hash)]
pub struct Name<'a> {
    string: Cow<'a, str>,
    lang: Option<Language>,
}

impl<'a> Name<'a> {
    /// Constructs a new mangled symbol.
    pub fn new<S>(string: S) -> Name<'a>
    where
        S: Into<Cow<'a, str>>,
    {
        Name {
            string: string.into(),
            lang: None,
        }
    }

    /// Constructs a new mangled symbol with known language.
    pub fn with_language<S>(string: S, lang: Language) -> Name<'a>
    where
        S: Into<Cow<'a, str>>,
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

    /// The raw, mangled string of the symbol.
    pub fn as_str(&self) -> &str {
        &self.string
    }

    /// The language of the mangled symbol.
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

/// An error returned for unknown or invalid `ObjectKind`s.
#[derive(Debug, Fail, Clone, Copy)]
#[fail(display = "unknown object kind")]
pub struct UnknownObjectKindError;

/// Represents the physical object file format.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
pub enum ObjectKind {
    Breakpad,
    Elf,
    MachO,
}

impl ObjectKind {
    /// Returns the name of the object kind.
    pub fn name(self) -> &'static str {
        match self {
            ObjectKind::Breakpad => "breakpad",
            ObjectKind::Elf => "elf",
            ObjectKind::MachO => "macho",
        }
    }
}

impl fmt::Display for ObjectKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl str::FromStr for ObjectKind {
    type Err = UnknownObjectKindError;

    fn from_str(string: &str) -> Result<ObjectKind, UnknownObjectKindError> {
        Ok(match string {
            "breakpad" => ObjectKind::Breakpad,
            "elf" => ObjectKind::Elf,
            "macho" => ObjectKind::MachO,
            _ => return Err(UnknownObjectKindError),
        })
    }
}

#[cfg(feature = "with_serde")]
derive_deserialize_from_str!(ObjectKind, "ObjectKind");

#[cfg(feature = "with_serde")]
derive_serialize_from_display!(ObjectKind);

/// An error returned for unknown or invalid `ObjectClass`es.
#[derive(Debug, Fail, Clone, Copy)]
#[fail(display = "unknown object class")]
pub struct UnknownObjectClassError;

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
    pub fn name(self) -> &'static str {
        match self {
            ObjectClass::None => "none",
            ObjectClass::Relocatable => "rel",
            ObjectClass::Executable => "exe",
            ObjectClass::Library => "lib",
            ObjectClass::Dump => "dump",
            ObjectClass::Debug => "dbg",
            ObjectClass::Other => "other",
        }
    }

    pub fn human_name(self) -> &'static str {
        match self {
            ObjectClass::None => "file",
            ObjectClass::Relocatable => "object",
            ObjectClass::Executable => "executable",
            ObjectClass::Library => "library",
            ObjectClass::Dump => "memory dump",
            ObjectClass::Debug => "debug companion",
            ObjectClass::Other => "file",
        }
    }

    #[cfg(feature = "with_objects")]
    pub fn from_mach(mach_type: u32) -> ObjectClass {
        use goblin::mach::header::*;

        match mach_type {
            MH_OBJECT => ObjectClass::Relocatable,
            MH_EXECUTE => ObjectClass::Executable,
            MH_DYLIB => ObjectClass::Library,
            MH_CORE => ObjectClass::Dump,
            MH_DSYM => ObjectClass::Debug,
            _ => ObjectClass::Other,
        }
    }

    #[cfg(feature = "with_objects")]
    pub fn from_elf(elf_type: u16) -> ObjectClass {
        use goblin::elf::header::*;

        match elf_type {
            ET_NONE => ObjectClass::None,
            ET_REL => ObjectClass::Relocatable,
            ET_EXEC => ObjectClass::Executable,
            ET_DYN => ObjectClass::Library,
            ET_CORE => ObjectClass::Dump,
            _ => ObjectClass::Other,
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
}

impl fmt::Display for ObjectClass {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            write!(f, "{}", self.human_name())
        } else {
            write!(f, "{}", self.name())
        }
    }
}

impl str::FromStr for ObjectClass {
    type Err = UnknownObjectClassError;

    fn from_str(string: &str) -> Result<ObjectClass, UnknownObjectClassError> {
        Ok(match string {
            "none" => ObjectClass::None,
            "rel" => ObjectClass::Relocatable,
            "exe" => ObjectClass::Executable,
            "lib" => ObjectClass::Library,
            "dump" => ObjectClass::Dump,
            "dbg" => ObjectClass::Debug,
            "other" => ObjectClass::Other,
            _ => return Err(UnknownObjectClassError),
        })
    }
}

#[cfg(feature = "with_serde")]
derive_deserialize_from_str!(ObjectClass, "ObjectClass");

#[cfg(feature = "with_serde")]
derive_serialize_from_display!(ObjectClass);

/// An error returned for unknown or invalid `DebugKind`s.
#[derive(Debug, Fail, Clone, Copy)]
#[fail(display = "unknown debug kind")]
pub struct UnknownDebugKindError;

/// Represents the kind of debug information inside an object.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
pub enum DebugKind {
    Dwarf,
    Breakpad,
}

impl DebugKind {
    /// Returns the name of the object kind.
    pub fn name(self) -> &'static str {
        match self {
            DebugKind::Dwarf => "dwarf",
            DebugKind::Breakpad => "breakpad",
        }
    }
}

impl fmt::Display for DebugKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl str::FromStr for DebugKind {
    type Err = UnknownDebugKindError;

    fn from_str(string: &str) -> Result<DebugKind, UnknownDebugKindError> {
        Ok(match string {
            "dwarf" => DebugKind::Dwarf,
            "breakpad" => DebugKind::Breakpad,
            _ => return Err(UnknownDebugKindError),
        })
    }
}

#[cfg(feature = "with_serde")]
derive_deserialize_from_str!(DebugKind, "DebugKind");

#[cfg(feature = "with_serde")]
derive_serialize_from_display!(DebugKind);

pub use debugid::*;
