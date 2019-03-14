//! Common types and errors used in `symbolic`.

use std::borrow::Cow;
use std::fmt;
use std::mem;
use std::str;

use failure::Fail;

/// Names for x86 CPU registers by register number.
static I386: &[&'static str] = &[
    "$eax", "$ecx", "$edx", "$ebx", "$esp", "$ebp", "$esi", "$edi", "$eip", "$eflags", "$unused1",
    "$st0", "$st1", "$st2", "$st3", "$st4", "$st5", "$st6", "$st7", "$unused2", "$unused3",
    "$xmm0", "$xmm1", "$xmm2", "$xmm3", "$xmm4", "$xmm5", "$xmm6", "$xmm7", "$mm0", "$mm1", "$mm2",
    "$mm3", "$mm4", "$mm5", "$mm6", "$mm7", "$fcw", "$fsw", "$mxcsr", "$es", "$cs", "$ss", "$ds",
    "$fs", "$gs", "$unused4", "$unused5", "$tr", "$ldtr",
];

/// Names for x86_64 CPU registers by register number.
static X86_64: &[&'static str] = &[
    "$rax", "$rdx", "$rcx", "$rbx", "$rsi", "$rdi", "$rbp", "$rsp", "$r8", "$r9", "$r10", "$r11",
    "$r12", "$r13", "$r14", "$r15", "$rip", "$xmm0", "$xmm1", "$xmm2", "$xmm3", "$xmm4", "$xmm5",
    "$xmm6", "$xmm7", "$xmm8", "$xmm9", "$xmm10", "$xmm11", "$xmm12", "$xmm13", "$xmm14", "$xmm15",
    "$st0", "$st1", "$st2", "$st3", "$st4", "$st5", "$st6", "$st7", "$mm0", "$mm1", "$mm2", "$mm3",
    "$mm4", "$mm5", "$mm6", "$mm7", "$rflags", "$es", "$cs", "$ss", "$ds", "$fs", "$gs",
    "$unused1", "$unused2", "$fs.base", "$gs.base", "$unused3", "$unused4", "$tr", "$ldtr",
    "$mxcsr", "$fcw", "$fsw",
];

/// Names for 32bit ARM CPU registers by register number.
static ARM: &[&'static str] = &[
    "r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "r12", "sp", "lr",
    "pc", "f0", "f1", "f2", "f3", "f4", "f5", "f6", "f7", "fps", "cpsr", "", "", "", "", "", "",
    "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "",
    "", "", "", "", "", "", "", "", "s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9",
    "s10", "s11", "s12", "s13", "s14", "s15", "s16", "s17", "s18", "s19", "s20", "s21", "s22",
    "s23", "s24", "s25", "s26", "s27", "s28", "s29", "s30", "s31", "f0", "f1", "f2", "f3", "f4",
    "f5", "f6", "f7",
];

/// Names for 64bit ARM CPU registers by register number.
static ARM64: &[&'static str] = &[
    "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10", "x11", "x12", "x13", "x14",
    "x15", "x16", "x17", "x18", "x19", "x20", "x21", "x22", "x23", "x24", "x25", "x26", "x27",
    "x28", "x29", "x30", "sp", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "",
    "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "v0", "v1", "v2", "v3", "v4", "v5",
    "v6", "v7", "v8", "v9", "v10", "v11", "v12", "v13", "v14", "v15", "v16", "v17", "v18", "v19",
    "v20", "v21", "v22", "v23", "v24", "v25", "v26", "v27", "v28", "v29", "v30", "v31",
];

/// Represents a family of CPUs.
///
/// This is strongly connected to the [`Arch`] type, but reduces the selection to a range of
/// families with distinct properties, such as a generally common instruction set and pointer size.
///
/// [`Arch`]: enum.Arch.html
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u32)]
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
}

impl Default for CpuFamily {
    fn default() -> Self {
        CpuFamily::Unknown
    }
}

/// An error returned for an invalid [`Arch`](enum.Arch.html).
#[derive(Clone, Copy, Debug, Fail)]
#[fail(display = "unknown architecture")]
pub struct UnknownArchError;

/// An enum of CPU architectures and variants.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u32)]
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
}

impl Arch {
    /// Creates an arch from the u32 it represents.
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
            _ => Arch::Unknown,
        }
    }

    /// Returns the CPU family.
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
        }
    }

    /// Returns the name of the arch.
    pub fn name(self) -> &'static str {
        match self {
            Arch::Unknown => "unknown",
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
        }
    }

    /// Returns the native pointer size.
    pub fn pointer_size(self) -> Option<usize> {
        match self.cpu_family() {
            CpuFamily::Unknown => None,
            CpuFamily::Amd64 | CpuFamily::Arm64 | CpuFamily::Ppc64 => Some(8),
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
            CpuFamily::Intel32 | CpuFamily::Amd64 => None,
            CpuFamily::Unknown => None,
        }
    }

    /// The name of the IP register if known.
    pub fn ip_register_name(self) -> Option<&'static str> {
        match self.cpu_family() {
            CpuFamily::Intel32 => Some("eip"),
            CpuFamily::Amd64 => Some("rip"),
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
            | Arch::Amd64Unknown => false,
            _ => true,
        }
    }

    /// Returns the name of a register in a given architecture.
    pub fn register_name(self, register: u16) -> Option<&'static str> {
        let index = register as usize;

        let opt = match self.cpu_family() {
            CpuFamily::Intel32 => I386.get(index),
            CpuFamily::Amd64 => X86_64.get(index),
            CpuFamily::Arm64 => ARM64.get(index),
            CpuFamily::Arm32 => ARM.get(index),
            _ => None,
        };

        opt.cloned()
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
            _ => return Err(UnknownArchError),
        })
    }
}

/// An error returned for an invalid [`Language`](enum.Language.html).
#[derive(Clone, Copy, Debug, Fail)]
#[fail(display = "unknown language")]
pub struct UnknownLanguageError;

/// Supported programming languages for demangling.
#[allow(missing_docs)]
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
