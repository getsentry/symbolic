//! Architecture specific register definitions.

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::str::FromStr;

use thiserror::Error;

use crate::CpuFamily;

/// Defines a register enum for an architecture, along with its DWARF register numbers and names.
macro_rules! define_arch_registers {
    (
        $(#[$enum_meta:meta])*
        pub enum $enum:ident {
            $($variant:ident = { name: $name:literal, dwarf: $dwarf:literal $(, alias: $alias:literal)? },)*
        }
    ) => {
        $(#[$enum_meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum $enum {
            $(
                #[doc = concat!("The `", $name, "` register.")]
                $(#[doc = concat!("\nThis register is also known as `", $alias, "`.")])?
                $variant,
            )*
        }

        impl $enum {
            /// Returns the register with the given DWARF register number.
            ///
            /// Returns `None` for numbers which are not a register, such as reserved numbers.
            fn from_dwarf(number: u16) -> Option<Self> {
                match number {
                    $($dwarf => Some(Self::$variant),)*
                    _ => None,
                }
            }

            /// Returns the DWARF register number of this register.
            ///
            /// The number of a register is fixed and will not change.
            fn to_dwarf(self) -> u16 {
                match self {
                    $(Self::$variant => $dwarf,)*
                }
            }
        }

        impl Display for $enum {
            fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
                f.write_str(match self {
                    $(Self::$variant => $name,)*
                })
            }
        }

        impl FromStr for $enum {
            type Err = InvalidRegisterError;

            fn from_str(name: &str) -> Result<Self, Self::Err> {
                match name {
                    $(
                        $name => Ok(Self::$variant),
                        $(
                            $alias => Ok(Self::$variant),
                        )?
                    )*
                    _ => Err(InvalidRegisterError),
                }
            }
        }
    };
}

define_arch_registers! {
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
    /// We can parse the registers from their canonical names via the [`FromStr`] implementation,
    /// as shown below.
    ///
    /// ```rust
    /// # use symbolic_common::register::Amd64Register;
    /// let rax = "rax".parse::<Amd64Register>().expect("\"rax\" is a valid Amd64 register");
    /// assert_eq!(rax, Amd64Register::Rax);
    /// ```
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

define_arch_registers! {
    /// An x86 (i386) register.
    ///
    /// The set of registers follows the DWARF register number mapping of the
    /// [System V i386 psABI][psabi] (section 2.4.2), without the reserved numbers.
    ///
    /// The registers are `eax` to `edi` (general purpose), `eip` (the return address), `eflags`,
    /// `st0` to `st7` (x87), `xmm0` to `xmm7` (vectors), `mm0` to `mm7` (MMX), `mxcsr`, the
    /// segment registers, `tr`, `ldtr`, `fs.base` and `gs.base`.
    ///
    /// We can parse the registers from their canonical names via the [`FromStr`] implementation,
    /// as shown below.
    ///
    /// ```rust
    /// # use symbolic_common::register::Intel32Register;
    /// let eax = "eax".parse::<Intel32Register>().expect("\"eax\" is a valid Intel32 register");
    /// assert_eq!(eax, Intel32Register::Eax);
    /// ```
    ///
    /// These DWARF register numbers are only valid for debug information (`.debug_info`), not for
    /// `.eh_frame` unwind information. On 32-bit Darwin, Clang/LLVM and GCC swap the numbers of
    /// `esp` and `ebp` (4 and 5) and shift `st0` to `st7` by one (12 to 19) in `.eh_frame`, but not
    /// in debug information.
    ///
    /// [psabi]: https://gitlab.com/x86-psabis/i386-ABI
    pub enum Intel32Register {
        Eax = { name: "eax", dwarf: 0 },
        Ecx = { name: "ecx", dwarf: 1 },
        Edx = { name: "edx", dwarf: 2 },
        Ebx = { name: "ebx", dwarf: 3 },
        Esp = { name: "esp", dwarf: 4 },
        Ebp = { name: "ebp", dwarf: 5 },
        Esi = { name: "esi", dwarf: 6 },
        Edi = { name: "edi", dwarf: 7 },
        Eip = { name: "eip", dwarf: 8 },
        Eflags = { name: "eflags", dwarf: 9 },
        St0 = { name: "st0", dwarf: 11 },
        St1 = { name: "st1", dwarf: 12 },
        St2 = { name: "st2", dwarf: 13 },
        St3 = { name: "st3", dwarf: 14 },
        St4 = { name: "st4", dwarf: 15 },
        St5 = { name: "st5", dwarf: 16 },
        St6 = { name: "st6", dwarf: 17 },
        St7 = { name: "st7", dwarf: 18 },
        Xmm0 = { name: "xmm0", dwarf: 21 },
        Xmm1 = { name: "xmm1", dwarf: 22 },
        Xmm2 = { name: "xmm2", dwarf: 23 },
        Xmm3 = { name: "xmm3", dwarf: 24 },
        Xmm4 = { name: "xmm4", dwarf: 25 },
        Xmm5 = { name: "xmm5", dwarf: 26 },
        Xmm6 = { name: "xmm6", dwarf: 27 },
        Xmm7 = { name: "xmm7", dwarf: 28 },
        Mm0 = { name: "mm0", dwarf: 29 },
        Mm1 = { name: "mm1", dwarf: 30 },
        Mm2 = { name: "mm2", dwarf: 31 },
        Mm3 = { name: "mm3", dwarf: 32 },
        Mm4 = { name: "mm4", dwarf: 33 },
        Mm5 = { name: "mm5", dwarf: 34 },
        Mm6 = { name: "mm6", dwarf: 35 },
        Mm7 = { name: "mm7", dwarf: 36 },
        Mxcsr = { name: "mxcsr", dwarf: 39 },
        Es = { name: "es", dwarf: 40 },
        Cs = { name: "cs", dwarf: 41 },
        Ss = { name: "ss", dwarf: 42 },
        Ds = { name: "ds", dwarf: 43 },
        Fs = { name: "fs", dwarf: 44 },
        Gs = { name: "gs", dwarf: 45 },
        Tr = { name: "tr", dwarf: 48 },
        Ldtr = { name: "ldtr", dwarf: 49 },
        FsBase = { name: "fs.base", dwarf: 93 },
        GsBase = { name: "gs.base", dwarf: 94 },
    }
}

define_arch_registers! {
    /// An AArch64 register.
    ///
    /// The set of registers follows the DWARF register mapping of the
    /// [Arm 64-bit Architecture ABI][aadwarf64] (AADWARF64, section 4.1), without the reserved
    /// numbers.
    ///
    /// The registers are `x0` to `x30` (general purpose), `sp`, `pc`, a few system registers,
    /// `v0` to `v31` (SIMD and floating point), and the SVE registers `z0` to `z31` (vectors),
    /// `p0` to `p15` (predicates), `vg` and `ffr`.
    ///
    /// Registers can be parsed from their canonical names via the [`FromStr`] implementation:
    ///
    /// ```rust
    /// # use symbolic_common::register::Arm64Register;
    /// let x0 = "x0".parse::<Arm64Register>().expect("\"x0\" is a valid Arm64 register");
    /// assert_eq!(x0, Arm64Register::X0);
    /// ```
    ///
    /// The registers `x29` and `x30` are also known as `fp` (frame pointer) and `lr` (link register),
    /// respectively. Each can be parsed using either name:
    ///
    /// ```rust
    /// # use symbolic_common::register::Arm64Register;
    /// assert_eq!("x29".parse::<Arm64Register>().unwrap(), Arm64Register::X29);
    /// assert_eq!("fp".parse::<Arm64Register>().unwrap(), Arm64Register::X29);
    ///
    /// assert_eq!("x30".parse::<Arm64Register>().unwrap(), Arm64Register::X30);
    /// assert_eq!("lr".parse::<Arm64Register>().unwrap(), Arm64Register::X30);
    /// ```
    ///
    /// [aadwarf64]: https://github.com/ARM-software/abi-aa/blob/main/aadwarf64/aadwarf64.rst#dwarf-register-names
    pub enum Arm64Register {
        X0 = { name: "x0", dwarf: 0 },
        X1 = { name: "x1", dwarf: 1 },
        X2 = { name: "x2", dwarf: 2 },
        X3 = { name: "x3", dwarf: 3 },
        X4 = { name: "x4", dwarf: 4 },
        X5 = { name: "x5", dwarf: 5 },
        X6 = { name: "x6", dwarf: 6 },
        X7 = { name: "x7", dwarf: 7 },
        X8 = { name: "x8", dwarf: 8 },
        X9 = { name: "x9", dwarf: 9 },
        X10 = { name: "x10", dwarf: 10 },
        X11 = { name: "x11", dwarf: 11 },
        X12 = { name: "x12", dwarf: 12 },
        X13 = { name: "x13", dwarf: 13 },
        X14 = { name: "x14", dwarf: 14 },
        X15 = { name: "x15", dwarf: 15 },
        X16 = { name: "x16", dwarf: 16 },
        X17 = { name: "x17", dwarf: 17 },
        X18 = { name: "x18", dwarf: 18 },
        X19 = { name: "x19", dwarf: 19 },
        X20 = { name: "x20", dwarf: 20 },
        X21 = { name: "x21", dwarf: 21 },
        X22 = { name: "x22", dwarf: 22 },
        X23 = { name: "x23", dwarf: 23 },
        X24 = { name: "x24", dwarf: 24 },
        X25 = { name: "x25", dwarf: 25 },
        X26 = { name: "x26", dwarf: 26 },
        X27 = { name: "x27", dwarf: 27 },
        X28 = { name: "x28", dwarf: 28 },
        // The minidump crate uses "fp" for register "x29".
        X29 = { name: "x29", dwarf: 29, alias: "fp" },
        // The minidump crate uses "lr" for register "x30".
        X30 = { name: "x30", dwarf: 30, alias: "lr" },
        Sp = { name: "sp", dwarf: 31 },
        Pc = { name: "pc", dwarf: 32 },
        ElrMode = { name: "elr_mode", dwarf: 33 },
        RaSignState = { name: "ra_sign_state", dwarf: 34 },
        TpidrroEl0 = { name: "tpidrro_el0", dwarf: 35 },
        TpidrEl0 = { name: "tpidr_el0", dwarf: 36 },
        TpidrEl1 = { name: "tpidr_el1", dwarf: 37 },
        TpidrEl2 = { name: "tpidr_el2", dwarf: 38 },
        TpidrEl3 = { name: "tpidr_el3", dwarf: 39 },
        Vg = { name: "vg", dwarf: 46 },
        Ffr = { name: "ffr", dwarf: 47 },
        P0 = { name: "p0", dwarf: 48 },
        P1 = { name: "p1", dwarf: 49 },
        P2 = { name: "p2", dwarf: 50 },
        P3 = { name: "p3", dwarf: 51 },
        P4 = { name: "p4", dwarf: 52 },
        P5 = { name: "p5", dwarf: 53 },
        P6 = { name: "p6", dwarf: 54 },
        P7 = { name: "p7", dwarf: 55 },
        P8 = { name: "p8", dwarf: 56 },
        P9 = { name: "p9", dwarf: 57 },
        P10 = { name: "p10", dwarf: 58 },
        P11 = { name: "p11", dwarf: 59 },
        P12 = { name: "p12", dwarf: 60 },
        P13 = { name: "p13", dwarf: 61 },
        P14 = { name: "p14", dwarf: 62 },
        P15 = { name: "p15", dwarf: 63 },
        V0 = { name: "v0", dwarf: 64 },
        V1 = { name: "v1", dwarf: 65 },
        V2 = { name: "v2", dwarf: 66 },
        V3 = { name: "v3", dwarf: 67 },
        V4 = { name: "v4", dwarf: 68 },
        V5 = { name: "v5", dwarf: 69 },
        V6 = { name: "v6", dwarf: 70 },
        V7 = { name: "v7", dwarf: 71 },
        V8 = { name: "v8", dwarf: 72 },
        V9 = { name: "v9", dwarf: 73 },
        V10 = { name: "v10", dwarf: 74 },
        V11 = { name: "v11", dwarf: 75 },
        V12 = { name: "v12", dwarf: 76 },
        V13 = { name: "v13", dwarf: 77 },
        V14 = { name: "v14", dwarf: 78 },
        V15 = { name: "v15", dwarf: 79 },
        V16 = { name: "v16", dwarf: 80 },
        V17 = { name: "v17", dwarf: 81 },
        V18 = { name: "v18", dwarf: 82 },
        V19 = { name: "v19", dwarf: 83 },
        V20 = { name: "v20", dwarf: 84 },
        V21 = { name: "v21", dwarf: 85 },
        V22 = { name: "v22", dwarf: 86 },
        V23 = { name: "v23", dwarf: 87 },
        V24 = { name: "v24", dwarf: 88 },
        V25 = { name: "v25", dwarf: 89 },
        V26 = { name: "v26", dwarf: 90 },
        V27 = { name: "v27", dwarf: 91 },
        V28 = { name: "v28", dwarf: 92 },
        V29 = { name: "v29", dwarf: 93 },
        V30 = { name: "v30", dwarf: 94 },
        V31 = { name: "v31", dwarf: 95 },
        Z0 = { name: "z0", dwarf: 96 },
        Z1 = { name: "z1", dwarf: 97 },
        Z2 = { name: "z2", dwarf: 98 },
        Z3 = { name: "z3", dwarf: 99 },
        Z4 = { name: "z4", dwarf: 100 },
        Z5 = { name: "z5", dwarf: 101 },
        Z6 = { name: "z6", dwarf: 102 },
        Z7 = { name: "z7", dwarf: 103 },
        Z8 = { name: "z8", dwarf: 104 },
        Z9 = { name: "z9", dwarf: 105 },
        Z10 = { name: "z10", dwarf: 106 },
        Z11 = { name: "z11", dwarf: 107 },
        Z12 = { name: "z12", dwarf: 108 },
        Z13 = { name: "z13", dwarf: 109 },
        Z14 = { name: "z14", dwarf: 110 },
        Z15 = { name: "z15", dwarf: 111 },
        Z16 = { name: "z16", dwarf: 112 },
        Z17 = { name: "z17", dwarf: 113 },
        Z18 = { name: "z18", dwarf: 114 },
        Z19 = { name: "z19", dwarf: 115 },
        Z20 = { name: "z20", dwarf: 116 },
        Z21 = { name: "z21", dwarf: 117 },
        Z22 = { name: "z22", dwarf: 118 },
        Z23 = { name: "z23", dwarf: 119 },
        Z24 = { name: "z24", dwarf: 120 },
        Z25 = { name: "z25", dwarf: 121 },
        Z26 = { name: "z26", dwarf: 122 },
        Z27 = { name: "z27", dwarf: 123 },
        Z28 = { name: "z28", dwarf: 124 },
        Z29 = { name: "z29", dwarf: 125 },
        Z30 = { name: "z30", dwarf: 126 },
        Z31 = { name: "z31", dwarf: 127 },
    }
}

/// Defines the architecture-independent register type and conversions from a shared CPU-family mapping.
macro_rules! define_register_wrapper {
    (
        $(
            $(#[$meta:meta])*
            $variant:ident($register:ident) => [$($family:path),+ $(,)?]
        ),+ $(,)?
    ) => {
        /// A register of any supported architecture.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        #[non_exhaustive]
        pub enum Register {
            $(
                $(#[$meta])*
                $variant($register),
            )+
        }

        impl Register {
            /// Returns the register with the given DWARF register number for the given CPU family.
            ///
            /// Returns `None` if the CPU family has no register mapping, or if the number is not a
            /// register of the CPU family.
            pub fn from_dwarf(cpu_family: CpuFamily, number: u16) -> Option<Self> {
                match cpu_family {
                    $(
                        $($family)|+ => {
                            $register::from_dwarf(number).map(Self::$variant)
                        }
                    )+
                    _ => None,
                }
            }

            /// Returns the DWARF register number of this register.
            ///
            /// DWARF register numbers are architecture dependent, so interpreting the number requires
            /// knowing the register's CPU family, as in [`Register::from_dwarf`]. The number of a
            /// register is fixed and will not change.
            pub fn to_dwarf(self) -> u16 {
                match self {
                    $(Self::$variant(register) => register.to_dwarf(),)+
                }
            }

            /// Return a typed register for the given register name and CPU family, if known and available.
            pub fn try_from_name(
                cpu_family: CpuFamily,
                name: &str,
            ) -> Option<Self> {
                match cpu_family {
                    $(
                        $($family)|+ => Some(Self::$variant(name.parse().ok()?)),
                    )+
                    _ => None,
                }
            }
        }

        impl Display for Register {
            fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
                match self {
                    $(Self::$variant(register) => register.fmt(f),)+
                }
            }
        }

        $(
            impl From<$register> for Register {
                fn from(register: $register) -> Self {
                    Self::$variant(register)
                }
            }
        )+
    };
}

define_register_wrapper! {
    /// An x86-64 register.
    Amd64(Amd64Register) => [CpuFamily::Amd64],

    /// An AArch64 register.
    Arm64(Arm64Register) => [CpuFamily::Arm64, CpuFamily::Arm64_32],

    /// An x86 register.
    Intel32(Intel32Register) => [CpuFamily::Intel32],
}

/// An error returned when parsing a register name fails.
#[derive(Debug, Error)]
#[error("invalid register")]
#[non_exhaustive]
pub struct InvalidRegisterError;
