//! Common types and errors used in `symbolic`.

use std::borrow::Cow;
use std::fmt;
use std::str;

#[cfg(feature = "serde")]
use serde_::{Deserialize, Serialize};

/// Represents a family of CPUs.
///
/// This is strongly connected to the [`Arch`] type, but reduces the selection to a range of
/// families with distinct properties, such as a generally common instruction set and pointer size.
///
/// This enumeration is represented as `u32` for C-bindings and lowlevel APIs.
///
/// [`Arch`]: enum.Arch.html
#[repr(u32)]
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CpuFamily {
    /// Any other CPU family that is not explicitly supported.
    Unknown = 0,
    /// 32-bit little-endian CPUs using the Intel 8086 instruction set, also known as `x86`.
    Intel32 = 1,
    /// 64-bit little-endian, also known as `x86_64`, now widely used by Intel and AMD.
    Amd64 = 2,
    /// 32-bit ARM.
    Arm32 = 3,
    /// 64-bit ARM (e.g. ARMv8-A).
    Arm64 = 4,
    /// 32-bit big-endian PowerPC.
    Ppc32 = 5,
    /// 64-bit big-endian PowerPC.
    Ppc64 = 6,
    /// 32-bit MIPS.
    Mips32 = 7,
    /// 64-bit MIPS.
    Mips64 = 8,
    /// ILP32 ABI on 64-bit ARM.
    Arm64_32 = 9,
    /// Virtual WASM 32-bit architecture.
    Wasm32 = 10,
}

impl CpuFamily {
    /// Returns the native pointer size.
    ///
    /// This commonly defines the size of CPU registers including the instruction pointer, and the
    /// size of all pointers on the platform.
    ///
    /// This function returns `None` if the CPU family is unknown.
    ///
    /// # Examples
    ///
    /// ```
    /// use symbolic_common::CpuFamily;
    ///
    /// assert_eq!(CpuFamily::Amd64.pointer_size(), Some(8));
    /// assert_eq!(CpuFamily::Intel32.pointer_size(), Some(4));
    /// ```
    pub fn pointer_size(self) -> Option<usize> {
        match self {
            CpuFamily::Unknown => None,
            CpuFamily::Wasm32 => Some(4),
            CpuFamily::Amd64
            | CpuFamily::Arm64
            | CpuFamily::Ppc64
            | CpuFamily::Mips64
            | CpuFamily::Arm64_32 => Some(8),
            CpuFamily::Intel32 | CpuFamily::Arm32 | CpuFamily::Ppc32 | CpuFamily::Mips32 => Some(4),
        }
    }

    /// Returns instruction alignment if fixed.
    ///
    /// Some instruction sets, such as Intel's x86, use variable length instruction encoding.
    /// Others, such as ARM, have fixed length instructions. This method returns `Some` for fixed
    /// size instructions and `None` for variable-length instruction sizes.
    ///
    /// # Examples
    ///
    /// ```
    /// use symbolic_common::CpuFamily;
    ///
    /// // variable length on x86_64:
    /// assert_eq!(CpuFamily::Amd64.instruction_alignment(), None);
    ///
    /// // 4-byte alignment on all 64-bit ARM variants:
    /// assert_eq!(CpuFamily::Arm64.instruction_alignment(), Some(4));
    /// ```
    pub fn instruction_alignment(self) -> Option<u64> {
        match self {
            CpuFamily::Wasm32 => Some(4),
            CpuFamily::Arm32 => Some(2),
            CpuFamily::Arm64 | CpuFamily::Arm64_32 => Some(4),
            CpuFamily::Ppc32 | CpuFamily::Mips32 | CpuFamily::Mips64 => Some(4),
            CpuFamily::Ppc64 => Some(8),
            CpuFamily::Intel32 | CpuFamily::Amd64 => None,
            CpuFamily::Unknown => None,
        }
    }

    /// Returns the name of the instruction pointer register.
    ///
    /// The instruction pointer register holds a pointer to currrent code execution at all times.
    /// This is a differrent register on each CPU family. The size of the value in this register is
    /// specified by [`pointer_size`].
    ///
    /// Returns `None` if the CPU family is unknown.
    ///
    /// # Examples
    ///
    /// ```
    /// use symbolic_common::CpuFamily;
    ///
    /// assert_eq!(CpuFamily::Amd64.ip_register_name(), Some("rip"));
    /// ```
    ///
    /// [`pointer_size`]: enum.CpuFamily.html#method.pointer_size
    pub fn ip_register_name(self) -> Option<&'static str> {
        // NOTE: These values do not correspond to the register names defined in this file, but to
        // the names exposed by breakpad. This mapping is implemented in `data_structures.cpp`.
        match self {
            CpuFamily::Intel32 => Some("eip"),
            CpuFamily::Amd64 => Some("rip"),
            CpuFamily::Arm32 | CpuFamily::Arm64 | CpuFamily::Arm64_32 => Some("pc"),
            CpuFamily::Ppc32 | CpuFamily::Ppc64 => Some("srr0"),
            CpuFamily::Mips32 | CpuFamily::Mips64 => Some("pc"),
            CpuFamily::Wasm32 => None,
            CpuFamily::Unknown => None,
        }
    }
}

impl Default for CpuFamily {
    fn default() -> Self {
        CpuFamily::Unknown
    }
}

/// An error returned for an invalid [`Arch`](enum.Arch.html).
#[derive(Debug)]
pub struct UnknownArchError;

impl fmt::Display for UnknownArchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown architecture")
    }
}

impl std::error::Error for UnknownArchError {}

/// An enumeration of CPU architectures and variants.
///
/// The architectues are grouped into families, which can be retrieved by [`cpu_family`]. There are
/// `*Unknown` variants for each architecture to maintain forward-compatibility. This allows to
/// support architectures where the family is known but the subtype is not.
///
/// Each architecture has a canonical name, returned by [`Arch::name`]. Likewise, architectures can
/// be parsed from their string names. In addition to that, in some cases aliases are supported. For
/// instance, `"x86"` is aliased as `"i386"`.
///
/// This enumeration is represented as `u32` for C-bindings and lowlevel APIs. The values are
/// grouped by CPU family for forward compatibility.
///
/// [`cpu_family`]: enum.Arch.html#method.cpu_family
/// [`Arch::name`]: enum.Arch.html#method.name
#[repr(u32)]
#[non_exhaustive]
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Arch {
    Unknown = 0,
    X86 = 101,
    X86Unknown = 199,
    Amd64 = 201,
    Amd64h = 202,
    Amd64Unknown = 299,
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
    Mips = 701,
    Mips64 = 801,
    Arm64_32 = 901,
    Arm64_32V8 = 902,
    Arm64_32Unknown = 999,
    Wasm32 = 1001,
}

impl Arch {
    /// Creates an `Arch` from its `u32` representation.
    ///
    /// Returns `Arch::Unknown` for all unknown values.
    ///
    /// # Examples
    ///
    /// ```
    /// use symbolic_common::Arch;
    ///
    /// // Will print "X86"
    /// println!("{:?}", Arch::from_u32(101));
    /// ```
    pub fn from_u32(val: u32) -> Arch {
        match val {
            0 => Arch::Unknown,
            1 | 101 => Arch::X86,
            199 => Arch::X86Unknown,
            2 | 201 => Arch::Amd64,
            3 | 202 => Arch::Amd64h,
            299 => Arch::Amd64Unknown,
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
            701 => Arch::Mips,
            801 => Arch::Mips64,
            901 => Arch::Arm64_32,
            902 => Arch::Arm64_32V8,
            999 => Arch::Arm64_32Unknown,
            1001 => Arch::Wasm32,
            _ => Arch::Unknown,
        }
    }

    /// Returns the CPU family of the CPU architecture.
    ///
    /// # Examples
    ///
    /// ```
    /// use symbolic_common::Arch;
    ///
    /// // Will print "Intel32"
    /// println!("{:?}", Arch::X86.cpu_family());
    /// ```
    pub fn cpu_family(self) -> CpuFamily {
        match self {
            Arch::Unknown => CpuFamily::Unknown,
            Arch::X86 | Arch::X86Unknown => CpuFamily::Intel32,
            Arch::Amd64 | Arch::Amd64h | Arch::Amd64Unknown => CpuFamily::Amd64,
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
            Arch::Mips => CpuFamily::Mips32,
            Arch::Mips64 => CpuFamily::Mips64,
            Arch::Arm64_32 | Arch::Arm64_32V8 | Arch::Arm64_32Unknown => CpuFamily::Arm64_32,
            Arch::Wasm32 => CpuFamily::Wasm32,
        }
    }

    /// Returns the canonical name of the CPU architecture.
    ///
    /// This follows the Apple conventions for naming architectures. For instance, Intel 32-bit
    /// architectures are canonically named `"x86"`, even though `"i386"` would also be a valid
    /// name.
    ///
    /// For architectures with variants or subtypes, that subtype is encoded into the name. For
    /// instance the ARM v7-M architecture is named with a full `"armv7m".
    ///
    /// # Examples
    ///
    /// ```
    /// use symbolic_common::Arch;
    ///
    /// // Will print "x86"
    /// println!("{}", Arch::X86.name());
    /// ```
    pub fn name(self) -> &'static str {
        match self {
            Arch::Unknown => "unknown",
            Arch::Wasm32 => "wasm32",
            Arch::X86 => "x86",
            Arch::X86Unknown => "x86_unknown",
            Arch::Amd64 => "x86_64",
            Arch::Amd64h => "x86_64h",
            Arch::Amd64Unknown => "x86_64_unknown",
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
            Arch::Mips => "mips",
            Arch::Mips64 => "mips64",
            Arch::Arm64_32 => "arm64_32",
            Arch::Arm64_32V8 => "arm64_32_v8",
            Arch::Arm64_32Unknown => "arm64_32_unknown",
        }
    }

    /// Returns whether this architecture is well-known.
    ///
    /// This is trivially `true` for all architectures other than the `*Unknown` variants.
    ///
    /// # Examples
    ///
    /// ```
    /// use symbolic_common::Arch;
    ///
    /// assert!(Arch::X86.well_known());
    /// assert!(!Arch::X86Unknown.well_known());
    /// ```
    pub fn well_known(self) -> bool {
        !matches!(
            self,
            Arch::Unknown
                | Arch::ArmUnknown
                | Arch::Arm64Unknown
                | Arch::X86Unknown
                | Arch::Amd64Unknown
                | Arch::Arm64_32Unknown
        )
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
        Ok(match string.to_ascii_lowercase().as_str() {
            "unknown" => Arch::Unknown,
            // this is an alias that is known among macho users
            "i386" => Arch::X86,
            "x86" => Arch::X86,
            "x86_unknown" => Arch::X86Unknown,
            "x86_64" | "amd64" => Arch::Amd64,
            "x86_64h" => Arch::Amd64h,
            "x86_64_unknown" => Arch::Amd64Unknown,
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
            "mips" => Arch::Mips,
            "mips64" => Arch::Mips64,
            "arm64_32" => Arch::Arm64_32,
            "arm64_32_v8" => Arch::Arm64_32V8,
            "arm64_32_unknown" => Arch::Arm64_32Unknown,

            // apple crash report variants
            "x86-64" => Arch::Amd64,
            "arm-64" => Arch::Arm64,

            // wasm extensions
            "wasm32" => Arch::Wasm32,

            _ => return Err(UnknownArchError),
        })
    }
}

/// An error returned for an invalid [`Language`](enum.Language.html).
#[derive(Debug)]
pub struct UnknownLanguageError;

impl fmt::Display for UnknownLanguageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown language")
    }
}

impl std::error::Error for UnknownLanguageError {}

/// A programming language declared in debugging information.
///
/// In the context of function names or source code, the lanugage can help to determine appropriate
/// strategies for demangling names or syntax highlighting. See the [`Name`] type, which declares a
/// function name with an optional language.
///
/// This enumeration is represented as `u32` for C-bindings and lowlevel APIs.
///
/// [`Name`]: struct.Name.html
#[repr(u32)]
#[non_exhaustive]
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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
    CSharp = 9,
    VisualBasic = 10,
    FSharp = 11,
}

impl Language {
    /// Creates an `Language` from its `u32` representation.
    ///
    /// Returns `Language::Unknown` for all unknown values.
    ///
    /// # Examples
    ///
    /// ```
    /// use symbolic_common::Language;
    ///
    /// // Will print "C"
    /// println!("{:?}", Language::from_u32(1));
    /// ```
    pub fn from_u32(val: u32) -> Language {
        match val {
            0 => Self::Unknown,
            1 => Self::C,
            2 => Self::Cpp,
            3 => Self::D,
            4 => Self::Go,
            5 => Self::ObjC,
            6 => Self::ObjCpp,
            7 => Self::Rust,
            8 => Self::Swift,
            9 => Self::CSharp,
            10 => Self::VisualBasic,
            11 => Self::FSharp,
            _ => Self::Unknown,
        }
    }

    /// Returns the name of the language.
    ///
    /// The name is always given in lower case without special characters or spaces, suitable for
    /// serialization and parsing. For a human readable name, use the `Display` implementation,
    /// instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use symbolic_common::Language;
    ///
    /// // Will print "objcpp"
    /// println!("{}", Language::ObjCpp.name());
    ///
    /// // Will print "Objective-C++"
    /// println!("{}", Language::ObjCpp);
    /// ```
    pub fn name(self) -> &'static str {
        match self {
            Language::Unknown => "unknown",
            Language::C => "c",
            Language::Cpp => "cpp",
            Language::D => "d",
            Language::Go => "go",
            Language::ObjC => "objc",
            Language::ObjCpp => "objcpp",
            Language::Rust => "rust",
            Language::Swift => "swift",
            Language::CSharp => "csharp",
            Language::VisualBasic => "visualbasic",
            Language::FSharp => "fsharp",
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
            Language::Unknown => "unknown",
            Language::C => "C",
            Language::Cpp => "C++",
            Language::D => "D",
            Language::Go => "Go",
            Language::ObjC => "Objective-C",
            Language::ObjCpp => "Objective-C++",
            Language::Rust => "Rust",
            Language::Swift => "Swift",
            Language::CSharp => "C#",
            Language::VisualBasic => "Visual Basic",
            Language::FSharp => "F#",
        };

        write!(f, "{}", formatted)
    }
}

impl str::FromStr for Language {
    type Err = UnknownLanguageError;

    fn from_str(string: &str) -> Result<Language, UnknownLanguageError> {
        Ok(match string {
            "unknown" => Language::Unknown,
            "c" => Language::C,
            "cpp" => Language::Cpp,
            "d" => Language::D,
            "go" => Language::Go,
            "objc" => Language::ObjC,
            "objcpp" => Language::ObjCpp,
            "rust" => Language::Rust,
            "swift" => Language::Swift,
            "csharp" => Language::CSharp,
            "visualbasic" => Language::VisualBasic,
            "fsharp" => Language::FSharp,
            _ => return Err(UnknownLanguageError),
        })
    }
}

/// A [`Name`]s mangling state.
///
/// By default, the mangling of a [`Name`] is not known, but an explicit mangling state can be set
/// for Names that are guaranteed to be unmangled.
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_")
)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NameMangling {
    /// The [`Name`] is definitely mangled.
    Mangled,
    /// The [`Name`] is not mangled.
    Unmangled,
    /// The mangling of the [`Name`] is not known.
    Unknown,
}

impl Default for NameMangling {
    fn default() -> Self {
        NameMangling::Unknown
    }
}

/// The name of a potentially mangled symbol.
///
/// Debugging information often only contains mangled names in their symbol and debug information
/// data. The mangling schema depends on the compiler and programming language. `Name` is a wrapper
/// type for potentially mangled names and an optionally declared language. To demangle the name,
/// see the `demangle` feature of `symbolic`.
///
/// Not all sources declare a programming language. In such a case, the [`language`] will be
/// `Unknown`. However, it may still be inferred for demangling by inspecting the mangled string.
///
/// Names can refer either functions, types, fields, or virtual constructs. Their semantics are
/// fully defined by the language and the compiler.
///
/// # Examples
///
/// Create a name and print it:
///
/// ```
/// use symbolic_common::Name;
///
/// let name = Name::from("_ZN3foo3barEv");
/// assert_eq!(name.to_string(), "_ZN3foo3barEv");
/// ```
///
/// Create a name with a language and explicit mangling state.
/// Alternate formatting prints the language:
///
/// ```
/// use symbolic_common::{Language, Name, NameMangling};
///
/// let name = Name::new("_ZN3foo3barEv", NameMangling::Mangled, Language::Cpp);
/// assert_eq!(format!("{:#}", name), "_ZN3foo3barEv [C++]");
/// ```
///
/// [`language`]: struct.Name.html#method.language
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_")
)]
pub struct Name<'a> {
    string: Cow<'a, str>,
    lang: Language,
    #[cfg_attr(feature = "serde", serde(default))]
    mangling: NameMangling,
}

impl<'a> Name<'a> {
    /// Constructs a new Name with given mangling and language.
    ///
    /// In case both the mangling state and the language are unknown, a simpler alternative to use
    /// is [`Name::from`].
    ///
    ///
    /// # Example
    ///
    /// ```
    /// use symbolic_common::{Language, Name, NameMangling};
    ///
    /// let name = Name::new("_ZN3foo3barEv", NameMangling::Mangled, Language::Cpp);
    /// assert_eq!(format!("{:#}", name), "_ZN3foo3barEv [C++]");
    /// ```
    #[inline]
    pub fn new<S>(string: S, mangling: NameMangling, lang: Language) -> Self
    where
        S: Into<Cow<'a, str>>,
    {
        Name {
            string: string.into(),
            lang,
            mangling,
        }
    }

    /// Returns the raw, mangled string of the name.
    ///
    /// # Example
    ///
    /// ```
    /// use symbolic_common::{Language, Name, NameMangling};
    ///
    /// let name = Name::new("_ZN3foo3barEv", NameMangling::Mangled, Language::Cpp);
    /// assert_eq!(name.as_str(), "_ZN3foo3barEv");
    /// ```
    ///
    /// This is also available as an `AsRef<str>` implementation:
    ///
    /// ```
    /// use symbolic_common::{Language, Name, NameMangling};
    ///
    /// let name = Name::new("_ZN3foo3barEv", NameMangling::Mangled, Language::Cpp);
    /// assert_eq!(name.as_ref(), "_ZN3foo3barEv");
    /// ```
    pub fn as_str(&self) -> &str {
        &self.string
    }

    /// Set the `Name`'s language.
    pub fn set_language(&mut self, language: Language) -> &mut Self {
        self.lang = language;
        self
    }

    /// The language of the mangled symbol.
    ///
    /// If the language is not declared in the source, this returns `Language::Unknown`. The
    /// language may still be inferred using `detect_language`, which is declared on the `Demangle`
    /// extension trait.
    ///
    /// # Example
    ///
    /// ```
    /// use symbolic_common::{Language, Name, NameMangling};
    ///
    /// let name = Name::new("_ZN3foo3barEv", NameMangling::Mangled, Language::Cpp);
    /// assert_eq!(name.language(), Language::Cpp);
    /// ```
    pub fn language(&self) -> Language {
        self.lang
    }

    /// Set the `Name`'s mangling state.
    pub fn set_mangling(&mut self, mangling: NameMangling) -> &mut Self {
        self.mangling = mangling;
        self
    }

    /// Returns the `Name`'s mangling state.
    ///
    /// # Example
    ///
    /// ```
    /// use symbolic_common::{Language, Name, NameMangling};
    ///
    /// let unmangled = Name::new("foo::bar", NameMangling::Unmangled, Language::Unknown);
    /// assert_eq!(unmangled.mangling(), NameMangling::Unmangled);
    /// ```
    pub fn mangling(&self) -> NameMangling {
        self.mangling
    }

    /// Converts this name into a [`Cow`].
    ///
    /// # Example
    ///
    /// ```
    /// use symbolic_common::Name;
    ///
    /// let name = Name::from("_ZN3foo3barEv");
    /// assert_eq!(name.into_cow(), "_ZN3foo3barEv");
    /// ```
    pub fn into_cow(self) -> Cow<'a, str> {
        self.string
    }

    /// Converts this name into a [`String`].
    ///
    /// # Example
    ///
    /// ```
    /// use symbolic_common::Name;
    ///
    /// let name = Name::from("_ZN3foo3barEv");
    /// assert_eq!(name.into_string(), "_ZN3foo3barEv");
    /// ```
    pub fn into_string(self) -> String {
        self.string.into_owned()
    }
}

impl AsRef<str> for Name<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<Name<'_>> for String {
    fn from(name: Name) -> Self {
        name.string.into()
    }
}

impl<'a, S> From<S> for Name<'a>
where
    S: Into<Cow<'a, str>>,
{
    fn from(string: S) -> Self {
        Self::new(string, NameMangling::Unknown, Language::Unknown)
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
        #[allow(clippy::extra_unused_lifetimes)]
        impl<'a, 'b> PartialEq<$rhs> for $lhs {
            #[inline]
            fn eq(&self, other: &$rhs) -> bool {
                PartialEq::eq(&self.string, other)
            }
        }

        #[allow(clippy::extra_unused_lifetimes)]
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

#[cfg(feature = "serde")]
mod derive_serde {
    /// Helper macro to implement string based serialization and deserialization.
    ///
    /// If a type implements `FromStr` and `Display` then this automatically
    /// implements a serializer/deserializer for that type that dispatches
    /// appropriately.
    macro_rules! impl_str_serde {
        ($type:ty) => {
            impl ::serde_::ser::Serialize for $type {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: ::serde_::ser::Serializer,
                {
                    serializer.serialize_str(self.name())
                }
            }

            impl<'de> ::serde_::de::Deserialize<'de> for $type {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: ::serde_::de::Deserializer<'de>,
                {
                    <::std::borrow::Cow<str>>::deserialize(deserializer)?
                        .parse()
                        .map_err(::serde_::de::Error::custom)
                }
            }
        };
    }

    impl_str_serde!(super::Arch);
    impl_str_serde!(super::Language);
}
