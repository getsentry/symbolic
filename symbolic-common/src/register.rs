//! Architecture specific register definitions.
//!
//! Registers are identified by symbolic's own types rather than by the numbering of a particular
//! debug format, so that the same register can be named independently of where it was read from.

use std::fmt;

use crate::CpuFamily;

/// Defines a register enum for an architecture, along with its DWARF register numbers and names.
macro_rules! registers {
    (
        $(#[$enum_meta:meta])*
        pub enum $enum:ident {
            $($variant:ident = { name: $name:literal, dwarf: $dwarf:literal },)*
        }
    ) => {
        $(#[$enum_meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $enum {
            $(
                #[doc = concat!("The `", $name, "` register.")]
                $variant,
            )*
        }

        impl $enum {
            /// Returns the register with the given DWARF register number.
            ///
            /// Returns `None` for numbers which are not a register, such as reserved numbers.
            pub fn from_dwarf(number: u16) -> Option<Self> {
                match number {
                    $($dwarf => Some(Self::$variant),)*
                    _ => None,
                }
            }

            /// Returns the DWARF register number of this register.
            ///
            /// The number of a register is fixed and will not change.
            pub fn to_dwarf(self) -> u16 {
                match self {
                    $(Self::$variant => $dwarf,)*
                }
            }
        }

        impl fmt::Display for $enum {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(match self {
                    $(Self::$variant => $name,)*
                })
            }
        }
    };
}

registers! {
    /// An x86-64 register.
    ///
    /// The set of registers follows the DWARF register number mapping of the
    /// [System V AMD64 psABI][psabi] (section 3.6.2), without the reserved numbers.
    ///
    /// The registers are `rax` to `rsp` and `r8` to `r31` (general purpose), `rip` (the return
    /// address), `xmm0` to `xmm31` (vectors), `st0` to `st7` (x87), `mm0` to `mm7` (MMX),
    /// `rflags`, the segment registers, a few system and control registers, `k0` to `k7` (vector
    /// masks), and the AMX registers `tmm0` to `tmm7` and `tilecfg`.
    ///
    /// The variants themselves are independent of the register numbering of any debug format,
    /// use [`Amd64Register::from_dwarf`] and [`Amd64Register::to_dwarf`] to convert from and to
    /// DWARF register numbers.
    ///
    /// [psabi]: https://gitlab.com/x86-psabis/x86-64-ABI
    pub enum Amd64Register {
        Rax = { name: "rax", dwarf: 0 },
        Rdx = { name: "rdx", dwarf: 1 },
        Rcx = { name: "rcx", dwarf: 2 },
        Rbx = { name: "rbx", dwarf: 3 },
        Rsi = { name: "rsi", dwarf: 4 },
        Rdi = { name: "rdi", dwarf: 5 },
        Rbp = { name: "rbp", dwarf: 6 },
        Rsp = { name: "rsp", dwarf: 7 },
        R8 = { name: "r8", dwarf: 8 },
        R9 = { name: "r9", dwarf: 9 },
        R10 = { name: "r10", dwarf: 10 },
        R11 = { name: "r11", dwarf: 11 },
        R12 = { name: "r12", dwarf: 12 },
        R13 = { name: "r13", dwarf: 13 },
        R14 = { name: "r14", dwarf: 14 },
        R15 = { name: "r15", dwarf: 15 },
        Rip = { name: "rip", dwarf: 16 },
        Xmm0 = { name: "xmm0", dwarf: 17 },
        Xmm1 = { name: "xmm1", dwarf: 18 },
        Xmm2 = { name: "xmm2", dwarf: 19 },
        Xmm3 = { name: "xmm3", dwarf: 20 },
        Xmm4 = { name: "xmm4", dwarf: 21 },
        Xmm5 = { name: "xmm5", dwarf: 22 },
        Xmm6 = { name: "xmm6", dwarf: 23 },
        Xmm7 = { name: "xmm7", dwarf: 24 },
        Xmm8 = { name: "xmm8", dwarf: 25 },
        Xmm9 = { name: "xmm9", dwarf: 26 },
        Xmm10 = { name: "xmm10", dwarf: 27 },
        Xmm11 = { name: "xmm11", dwarf: 28 },
        Xmm12 = { name: "xmm12", dwarf: 29 },
        Xmm13 = { name: "xmm13", dwarf: 30 },
        Xmm14 = { name: "xmm14", dwarf: 31 },
        Xmm15 = { name: "xmm15", dwarf: 32 },
        St0 = { name: "st0", dwarf: 33 },
        St1 = { name: "st1", dwarf: 34 },
        St2 = { name: "st2", dwarf: 35 },
        St3 = { name: "st3", dwarf: 36 },
        St4 = { name: "st4", dwarf: 37 },
        St5 = { name: "st5", dwarf: 38 },
        St6 = { name: "st6", dwarf: 39 },
        St7 = { name: "st7", dwarf: 40 },
        Mm0 = { name: "mm0", dwarf: 41 },
        Mm1 = { name: "mm1", dwarf: 42 },
        Mm2 = { name: "mm2", dwarf: 43 },
        Mm3 = { name: "mm3", dwarf: 44 },
        Mm4 = { name: "mm4", dwarf: 45 },
        Mm5 = { name: "mm5", dwarf: 46 },
        Mm6 = { name: "mm6", dwarf: 47 },
        Mm7 = { name: "mm7", dwarf: 48 },
        Rflags = { name: "rflags", dwarf: 49 },
        Es = { name: "es", dwarf: 50 },
        Cs = { name: "cs", dwarf: 51 },
        Ss = { name: "ss", dwarf: 52 },
        Ds = { name: "ds", dwarf: 53 },
        Fs = { name: "fs", dwarf: 54 },
        Gs = { name: "gs", dwarf: 55 },
        FsBase = { name: "fs.base", dwarf: 58 },
        GsBase = { name: "gs.base", dwarf: 59 },
        Tr = { name: "tr", dwarf: 62 },
        Ldtr = { name: "ldtr", dwarf: 63 },
        Mxcsr = { name: "mxcsr", dwarf: 64 },
        Fcw = { name: "fcw", dwarf: 65 },
        Fsw = { name: "fsw", dwarf: 66 },
        Xmm16 = { name: "xmm16", dwarf: 67 },
        Xmm17 = { name: "xmm17", dwarf: 68 },
        Xmm18 = { name: "xmm18", dwarf: 69 },
        Xmm19 = { name: "xmm19", dwarf: 70 },
        Xmm20 = { name: "xmm20", dwarf: 71 },
        Xmm21 = { name: "xmm21", dwarf: 72 },
        Xmm22 = { name: "xmm22", dwarf: 73 },
        Xmm23 = { name: "xmm23", dwarf: 74 },
        Xmm24 = { name: "xmm24", dwarf: 75 },
        Xmm25 = { name: "xmm25", dwarf: 76 },
        Xmm26 = { name: "xmm26", dwarf: 77 },
        Xmm27 = { name: "xmm27", dwarf: 78 },
        Xmm28 = { name: "xmm28", dwarf: 79 },
        Xmm29 = { name: "xmm29", dwarf: 80 },
        Xmm30 = { name: "xmm30", dwarf: 81 },
        Xmm31 = { name: "xmm31", dwarf: 82 },
        K0 = { name: "k0", dwarf: 118 },
        K1 = { name: "k1", dwarf: 119 },
        K2 = { name: "k2", dwarf: 120 },
        K3 = { name: "k3", dwarf: 121 },
        K4 = { name: "k4", dwarf: 122 },
        K5 = { name: "k5", dwarf: 123 },
        K6 = { name: "k6", dwarf: 124 },
        K7 = { name: "k7", dwarf: 125 },
        R16 = { name: "r16", dwarf: 130 },
        R17 = { name: "r17", dwarf: 131 },
        R18 = { name: "r18", dwarf: 132 },
        R19 = { name: "r19", dwarf: 133 },
        R20 = { name: "r20", dwarf: 134 },
        R21 = { name: "r21", dwarf: 135 },
        R22 = { name: "r22", dwarf: 136 },
        R23 = { name: "r23", dwarf: 137 },
        R24 = { name: "r24", dwarf: 138 },
        R25 = { name: "r25", dwarf: 139 },
        R26 = { name: "r26", dwarf: 140 },
        R27 = { name: "r27", dwarf: 141 },
        R28 = { name: "r28", dwarf: 142 },
        R29 = { name: "r29", dwarf: 143 },
        R30 = { name: "r30", dwarf: 144 },
        R31 = { name: "r31", dwarf: 145 },
        Tmm0 = { name: "tmm0", dwarf: 146 },
        Tmm1 = { name: "tmm1", dwarf: 147 },
        Tmm2 = { name: "tmm2", dwarf: 148 },
        Tmm3 = { name: "tmm3", dwarf: 149 },
        Tmm4 = { name: "tmm4", dwarf: 150 },
        Tmm5 = { name: "tmm5", dwarf: 151 },
        Tmm6 = { name: "tmm6", dwarf: 152 },
        Tmm7 = { name: "tmm7", dwarf: 153 },
        Tilecfg = { name: "tilecfg", dwarf: 154 },
    }
}

/// A register of any supported architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Register {
    /// An x86-64 register.
    Amd64(Amd64Register),
}

impl Register {
    /// Returns the register with the given DWARF register number for the given CPU family.
    ///
    /// Returns `None` if the CPU family has no register mapping, or if the number is not a
    /// register of the CPU family.
    pub fn from_dwarf(cpu_family: CpuFamily, number: u16) -> Option<Self> {
        match cpu_family {
            CpuFamily::Amd64 => Amd64Register::from_dwarf(number).map(Self::Amd64),
            _ => None,
        }
    }

    /// Returns the DWARF register number of this register.
    ///
    /// The number of a register is fixed and will not change.
    pub fn to_dwarf(self) -> u16 {
        match self {
            Self::Amd64(register) => register.to_dwarf(),
        }
    }
}

impl fmt::Display for Register {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Amd64(register) => register.fmt(f),
        }
    }
}

impl From<Amd64Register> for Register {
    fn from(register: Amd64Register) -> Self {
        Self::Amd64(register)
    }
}
