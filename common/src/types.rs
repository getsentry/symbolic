//! Common types and errors used in `symbolic`.

use std::borrow::Cow;
use std::fmt;
use std::mem;
use std::str;

use failure::Fail;

/// Represents a family of CPUs.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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

impl Default for CpuFamily {
    fn default() -> Self {
        CpuFamily::Unknown
    }
}

/// An error returned for unknown or invalid `Arch`s.
#[derive(Clone, Copy, Debug, Fail)]
#[fail(display = "unknown architecture")]
pub struct UnknownArchError;

/// An enum of supported architectures.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[allow(non_camel_case_types)]
#[repr(u32)]
pub enum Arch {
    Unknown = 0,
    X86 = 101,
    X86Unknown = 199,
    // TODO(ja): Rename to Amd64 (TODO: Implications on CABI?)
    X86_64 = 201,
    X86_64h = 202,
    X86_64Unknown = 299,
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
    ArmUnknown = 399,
    Arm64 = 401,
    Arm64V8 = 402,
    Arm64e = 403,
    Arm64Unknown = 499,
    Ppc = 501,
    Ppc64 = 601,
}

impl Arch {
    /// Creates an arch from the u32 it represents.
    pub fn from_u32(val: u32) -> Arch {
        match val {
            0 => Arch::Unknown,
            1 | 101 => Arch::X86,
            199 => Arch::X86Unknown,
            2 | 201 => Arch::X86_64,
            3 | 202 => Arch::X86_64h,
            299 => Arch::X86_64Unknown,
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
            399 => Arch::ArmUnknown,
            14 | 401 => Arch::Arm64,
            15 | 402 => Arch::Arm64V8,
            16 | 403 => Arch::Arm64e,
            499 => Arch::Arm64Unknown,
            17 | 501 => Arch::Ppc,
            18 | 601 => Arch::Ppc64,
            _ => Arch::Unknown,
        }
    }

    //     /// Constructs an architecture from ELF flags.
    //     #[cfg(feature = "with_objects")]
    //     pub fn from_breakpad(string: &str) -> Result<Arch, UnknownArchError> {
    //         Ok(match string {
    //             "x86" => Arch::X86,
    //             // This is different in minidumps and breakpad symbols
    //             "x86_64" | "amd64" => Arch::X86_64,
    //             "arm" => Arch::Arm,
    //             "arm64" => Arch::Arm64,
    //             "ppc" => Arch::Ppc,
    //             "ppc64" => Arch::Ppc64,
    //             _ => return Err(UnknownArchError),
    //         })
    //     }

    //     /// Returns the breakpad name for this Arch.
    //     pub fn to_breakpad(self) -> &'static str {
    //         match self.cpu_family() {
    //             CpuFamily::Intel32 => "x86",
    //             // Use the breakpad symbol constant here
    //             CpuFamily::Intel64 => "x86_64",
    //             CpuFamily::Arm32 => "arm",
    //             CpuFamily::Arm64 => "arm64",
    //             CpuFamily::Ppc32 => "ppc",
    //             CpuFamily::Ppc64 => "ppc64",
    //             CpuFamily::Unknown => "unknown",
    //         }
    //     }

    /// Returns the CPU family.
    pub fn cpu_family(self) -> CpuFamily {
        match self {
            Arch::Unknown => CpuFamily::Unknown,
            Arch::X86 | Arch::X86Unknown => CpuFamily::Intel32,
            Arch::X86_64 | Arch::X86_64h | Arch::X86_64Unknown => CpuFamily::Intel64,
            Arch::Arm64 | Arch::Arm64V8 | Arch::Arm64e | Arch::Arm64Unknown => CpuFamily::Arm64,
            Arch::Arm
            | Arch::ArmV5
            | Arch::ArmV6
            | Arch::ArmV6m
            | Arch::ArmV7
            | Arch::ArmV7f
            | Arch::ArmV7s
            | Arch::ArmV7k
            | Arch::ArmV7m
            | Arch::ArmV7em
            | Arch::ArmUnknown => CpuFamily::Arm32,
            Arch::Ppc => CpuFamily::Ppc32,
            Arch::Ppc64 => CpuFamily::Ppc64,
        }
    }

    /// Returns the name of the arch.
    pub fn name(self) -> &'static str {
        match self {
            Arch::Unknown => "unknown",
            Arch::X86 => "x86",
            Arch::X86Unknown => "x86_unknown",
            Arch::X86_64 => "x86_64",
            Arch::X86_64h => "x86_64h",
            Arch::X86_64Unknown => "x86_64_unknown",
            Arch::Arm64 => "arm64",
            Arch::Arm64V8 => "arm64v8",
            Arch::Arm64e => "arm64e",
            Arch::Arm64Unknown => "arm64_unknown",
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
            Arch::ArmUnknown => "arm_unknown",
            Arch::Ppc => "ppc",
            Arch::Ppc64 => "ppc64",
        }
    }

    /// Returns the native pointer size.
    pub fn pointer_size(self) -> Option<usize> {
        match self.cpu_family() {
            CpuFamily::Unknown => None,
            CpuFamily::Intel64 | CpuFamily::Arm64 | CpuFamily::Ppc64 => Some(8),
            CpuFamily::Intel32 | CpuFamily::Arm32 | CpuFamily::Ppc32 => Some(4),
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

    /// Returns whether this architecture is well-known.
    pub fn well_known(self) -> bool {
        match self {
            Arch::Unknown
            | Arch::ArmUnknown
            | Arch::Arm64Unknown
            | Arch::X86Unknown
            | Arch::X86_64Unknown => false,
            _ => true,
        }
    }
}

impl Default for Arch {
    fn default() -> Arch {
        Arch::Unknown
    }
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl str::FromStr for Arch {
    type Err = UnknownArchError;

    fn from_str(string: &str) -> Result<Arch, UnknownArchError> {
        Ok(match string {
            "unknown" => Arch::Unknown,
            // this is an alias that is known among macho users
            "i386" => Arch::X86,
            "x86" => Arch::X86,
            "x86_unknown" => Arch::X86Unknown,
            "x86_64" | "amd64" => Arch::X86_64,
            "x86_64h" => Arch::X86_64h,
            "x86_64_unknown" => Arch::X86_64Unknown,
            "arm64" => Arch::Arm64,
            "arm64v8" => Arch::Arm64V8,
            "arm64e" => Arch::Arm64e,
            "arm64_unknown" => Arch::Arm64Unknown,
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
            "arm_unknown" => Arch::ArmUnknown,
            "ppc" => Arch::Ppc,
            "ppc64" => Arch::Ppc64,
            _ => return Err(UnknownArchError),
        })
    }
}

/// An error returned for unknown or invalid `Language`s.
#[derive(Clone, Copy, Debug, Fail)]
#[fail(display = "unknown language")]
pub struct UnknownLanguageError;

/// Supported programming languages for demangling.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u32)]
pub enum Language {
    Unknown = 0,
    C = 1,
    Cpp = 2,
    D = 3,
    Go = 4,
    ObjC = 5,
    ObjCpp = 6,
    Rust = 7,
    Swift = 8,
    #[doc(hidden)]
    __Max = 9,
}

impl Language {
    /// Creates a language from the u32 it represents.
    pub fn from_u32(val: u32) -> Language {
        if val >= (Language::__Max as u32) {
            Language::Unknown
        } else {
            unsafe { mem::transmute(val) }
        }
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

/// Represents a potentially mangled symbol.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Name<'a> {
    string: Cow<'a, str>,
    lang: Language,
}

impl<'a> Name<'a> {
    /// Constructs a new mangled symbol.
    #[inline]
    pub fn new<S>(string: S) -> Self
    where
        S: Into<Cow<'a, str>>,
    {
        Name {
            string: string.into(),
            lang: Language::Unknown,
        }
    }

    /// Constructs a new mangled symbol with known language.
    #[inline]
    pub fn with_language<S>(string: S, lang: Language) -> Self
    where
        S: Into<Cow<'a, str>>,
    {
        let lang = match lang {
            Language::__Max => Language::Unknown,
            _ => lang,
        };

        Name {
            string: string.into(),
            lang,
        }
    }

    /// The raw, mangled string of the symbol.
    pub fn as_str(&self) -> &str {
        &self.string
    }

    /// The language of the mangled symbol.
    pub fn language(&self) -> Language {
        self.lang
    }

    /// Returns the backing of this name.
    pub fn into_cow(self) -> Cow<'a, str> {
        self.string
    }
}

impl AsRef<str> for Name<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Into<String> for Name<'_> {
    fn into(self) -> String {
        self.string.into()
    }
}

impl<'a, S> From<S> for Name<'a>
where
    S: Into<Cow<'a, str>>,
{
    fn from(string: S) -> Self {
        Self::new(string)
    }
}

impl fmt::Display for Name<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())?;

        if f.alternate() && self.lang != Language::Unknown {
            write!(f, " [{}]", self.lang)?;
        }

        Ok(())
    }
}

macro_rules! impl_eq {
    ($lhs:ty, $rhs: ty) => {
        impl<'a, 'b> PartialEq<$rhs> for $lhs {
            #[inline]
            fn eq(&self, other: &$rhs) -> bool {
                PartialEq::eq(&self.string, other)
            }
        }

        impl<'a, 'b> PartialEq<$lhs> for $rhs {
            #[inline]
            fn eq(&self, other: &$lhs) -> bool {
                PartialEq::eq(self, &other.string)
            }
        }
    };
}

impl_eq! { Name<'a>, str }
impl_eq! { Name<'a>, &'b str }
impl_eq! { Name<'a>, String }
impl_eq! { Name<'a>, std::borrow::Cow<'b, str> }

// /// An error returned for unknown or invalid `ObjectKind`s.
// #[derive(Debug, Fail, Clone, Copy)]
// #[fail(display = "unknown object kind")]
// pub struct UnknownObjectKindError;

// /// Represents the physical object file format.
// #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
// pub enum ObjectKind {
//     Breakpad,
//     Elf,
//     MachO,
// }

// impl ObjectKind {
//     /// Returns the name of the object kind.
//     pub fn name(self) -> &'static str {
//         match self {
//             ObjectKind::Breakpad => "breakpad",
//             ObjectKind::Elf => "elf",
//             ObjectKind::MachO => "macho",
//         }
//     }
// }

// impl fmt::Display for ObjectKind {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         write!(f, "{}", self.name())
//     }
// }

// impl str::FromStr for ObjectKind {
//     type Err = UnknownObjectKindError;

//     fn from_str(string: &str) -> Result<ObjectKind, UnknownObjectKindError> {
//         Ok(match string {
//             "breakpad" => ObjectKind::Breakpad,
//             "elf" => ObjectKind::Elf,
//             "macho" => ObjectKind::MachO,
//             _ => return Err(UnknownObjectKindError),
//         })
//     }
// }

// /// An error returned for unknown or invalid `ObjectClass`es.
// #[derive(Debug, Fail, Clone, Copy)]
// #[fail(display = "unknown object class")]
// pub struct UnknownObjectClassError;

// /// Represents the designated use of the object file and hints at its contents.
// #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
// pub enum ObjectClass {
//     /// There is no object class specified for this object file.
//     None,

//     /// The Relocatable file type is the format used for intermediate object
//     /// files. It is a very compact format containing all its sections in one
//     /// segment. The compiler and assembler usually create one Relocatable file
//     /// for each source code file. By convention, the file name extension for
//     /// this format is .o.
//     Relocatable,

//     /// The Executable file type is the format used by standard executable
//     /// programs.
//     Executable,

//     /// The Library file type is for dynamic shared libraries. It contains
//     /// some additional tables to support multiple modules. By convention, the
//     /// file name extension for this format is .dylib, except for the main
//     /// shared library of a framework, which does not usually have a file name
//     /// extension.
//     Library,

//     /// The Dump file type is used to store core files, which are
//     /// traditionally created when a program crashes. Core files store the
//     /// entire address space of a process at the time it crashed. You can
//     /// later run gdb on the core file to figure out why the crash occurred.
//     Dump,

//     /// The Debug file type designates files that store symbol information
//     /// for a corresponding binary file.
//     Debug,

//     /// The Other type represents any valid object class that does not fit any
//     /// of the other classes. These are mostly CPU or OS dependent, or unique
//     /// to a single kind of object.
//     Other,
// }

// impl ObjectClass {
//     pub fn name(self) -> &'static str {
//         match self {
//             ObjectClass::None => "none",
//             ObjectClass::Relocatable => "rel",
//             ObjectClass::Executable => "exe",
//             ObjectClass::Library => "lib",
//             ObjectClass::Dump => "dump",
//             ObjectClass::Debug => "dbg",
//             ObjectClass::Other => "other",
//         }
//     }

//     pub fn human_name(self) -> &'static str {
//         match self {
//             ObjectClass::None => "file",
//             ObjectClass::Relocatable => "object",
//             ObjectClass::Executable => "executable",
//             ObjectClass::Library => "library",
//             ObjectClass::Dump => "memory dump",
//             ObjectClass::Debug => "debug companion",
//             ObjectClass::Other => "file",
//         }
//     }

//     #[cfg(feature = "with_objects")]
//     pub fn from_mach(mach_type: u32) -> ObjectClass {
//         use goblin::mach::header::*;

//         match mach_type {
//             MH_OBJECT => ObjectClass::Relocatable,
//             MH_EXECUTE => ObjectClass::Executable,
//             MH_DYLIB => ObjectClass::Library,
//             MH_CORE => ObjectClass::Dump,
//             MH_DSYM => ObjectClass::Debug,
//             _ => ObjectClass::Other,
//         }
//     }

//     #[cfg(feature = "with_objects")]
//     pub fn from_elf(elf_type: u16) -> ObjectClass {
//         use goblin::elf::header::*;

//         match elf_type {
//             ET_NONE => ObjectClass::None,
//             ET_REL => ObjectClass::Relocatable,
//             ET_EXEC => ObjectClass::Executable,
//             ET_DYN => ObjectClass::Library,
//             ET_CORE => ObjectClass::Dump,
//             _ => ObjectClass::Other,
//         }
//     }

//     #[cfg(feature = "with_objects")]
//     pub fn from_elf_full(elf_type: u16, has_interpreter: bool) -> ObjectClass {
//         let class = ObjectClass::from_elf(elf_type);

//         // When stripping debug information into a separate file with objcopy,
//         // the eh_type field still reads ET_EXEC. However, the interpreter is
//         // removed. Since an executable without interpreter does not make any
//         // sense, we assume ``Debug`` in this case.
//         if class == ObjectClass::Executable && !has_interpreter {
//             ObjectClass::Debug
//         } else {
//             class
//         }
//     }
// }

// impl fmt::Display for ObjectClass {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         if f.alternate() {
//             write!(f, "{}", self.human_name())
//         } else {
//             write!(f, "{}", self.name())
//         }
//     }
// }

// impl str::FromStr for ObjectClass {
//     type Err = UnknownObjectClassError;

//     fn from_str(string: &str) -> Result<ObjectClass, UnknownObjectClassError> {
//         Ok(match string {
//             "none" => ObjectClass::None,
//             "rel" => ObjectClass::Relocatable,
//             "exe" => ObjectClass::Executable,
//             "lib" => ObjectClass::Library,
//             "dump" => ObjectClass::Dump,
//             "dbg" => ObjectClass::Debug,
//             "other" => ObjectClass::Other,
//             _ => return Err(UnknownObjectClassError),
//         })
//     }
// }

// /// An error returned for unknown or invalid `DebugKind`s.
// #[derive(Debug, Fail, Clone, Copy)]
// #[fail(display = "unknown debug kind")]
// pub struct UnknownDebugKindError;

// /// Represents the kind of debug information inside an object.
// #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone)]
// pub enum DebugKind {
//     Dwarf,
//     Breakpad,
// }

// impl DebugKind {
//     /// Returns the name of the object kind.
//     pub fn name(self) -> &'static str {
//         match self {
//             DebugKind::Dwarf => "dwarf",
//             DebugKind::Breakpad => "breakpad",
//         }
//     }
// }

// impl fmt::Display for DebugKind {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         write!(f, "{}", self.name())
//     }
// }

// impl str::FromStr for DebugKind {
//     type Err = UnknownDebugKindError;

//     fn from_str(string: &str) -> Result<DebugKind, UnknownDebugKindError> {
//         Ok(match string {
//             "dwarf" => DebugKind::Dwarf,
//             "breakpad" => DebugKind::Breakpad,
//             _ => return Err(UnknownDebugKindError),
//         })
//     }
// }

// TODO(ja): Implement serde
// #[cfg(feature = "with_serde")]
// mod derive_serde {
//     use super::*;
//     use serde_plain::{derive_deserialize_from_str, derive_serialize_from_display};

//     derive_deserialize_from_str!(Arch, "Arch");
//     derive_serialize_from_display!(Arch);

//     derive_deserialize_from_str!(Language, "Language");
//     derive_serialize_from_display!(Language);

//     derive_deserialize_from_str!(ObjectKind, "ObjectKind");
//     derive_serialize_from_display!(ObjectKind);

//     derive_deserialize_from_str!(ObjectClass, "ObjectClass");
//     derive_serialize_from_display!(ObjectClass);

//     derive_deserialize_from_str!(DebugKind, "DebugKind");
//     derive_serialize_from_display!(DebugKind);
// }
