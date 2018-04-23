use symbolic_common::types::{Arch, CpuFamily, UnknownArchError};

/// Returns the name of a register in a given architecture.
///
/// This can fail if register names for the CPU family are not known or the register number is
/// invalid.
pub fn get_register_name(arch: Arch, register: u8) -> Result<&'static str, UnknownArchError> {
    let index = register as usize;

    Ok(match arch.cpu_family() {
        CpuFamily::Intel32 => I386[index],
        CpuFamily::Intel64 => X86_64[index],
        CpuFamily::Arm64 => ARM64[index],
        CpuFamily::Arm32 => ARM[index],
        _ => return Err(UnknownArchError),
    })
}

/// Names for x86 CPU registers by register number.
static I386: &'static [&'static str] = &[
    "$eax", "$ecx", "$edx", "$ebx", "$esp", "$ebp", "$esi", "$edi", "$eip", "$eflags", "$unused1",
    "$st0", "$st1", "$st2", "$st3", "$st4", "$st5", "$st6", "$st7", "$unused2", "$unused3",
    "$xmm0", "$xmm1", "$xmm2", "$xmm3", "$xmm4", "$xmm5", "$xmm6", "$xmm7", "$mm0", "$mm1", "$mm2",
    "$mm3", "$mm4", "$mm5", "$mm6", "$mm7", "$fcw", "$fsw", "$mxcsr", "$es", "$cs", "$ss", "$ds",
    "$fs", "$gs", "$unused4", "$unused5", "$tr", "$ldtr",
];

/// Names for x86_64 CPU registers by register number.
static X86_64: &'static [&'static str] = &[
    "$rax", "$rdx", "$rcx", "$rbx", "$rsi", "$rdi", "$rbp", "$rsp", "$r8", "$r9", "$r10", "$r11",
    "$r12", "$r13", "$r14", "$r15", "$rip", "$xmm0", "$xmm1", "$xmm2", "$xmm3", "$xmm4", "$xmm5",
    "$xmm6", "$xmm7", "$xmm8", "$xmm9", "$xmm10", "$xmm11", "$xmm12", "$xmm13", "$xmm14", "$xmm15",
    "$st0", "$st1", "$st2", "$st3", "$st4", "$st5", "$st6", "$st7", "$mm0", "$mm1", "$mm2", "$mm3",
    "$mm4", "$mm5", "$mm6", "$mm7", "$rflags", "$es", "$cs", "$ss", "$ds", "$fs", "$gs",
    "$unused1", "$unused2", "$fs.base", "$gs.base", "$unused3", "$unused4", "$tr", "$ldtr",
    "$mxcsr", "$fcw", "$fsw",
];

/// Names for 32bit ARM CPU registers by register number.
static ARM: &'static [&'static str] = &[
    "r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "r12", "sp", "lr",
    "pc", "f0", "f1", "f2", "f3", "f4", "f5", "f6", "f7", "fps", "cpsr", "", "", "", "", "", "",
    "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "",
    "", "", "", "", "", "", "", "", "s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9",
    "s10", "s11", "s12", "s13", "s14", "s15", "s16", "s17", "s18", "s19", "s20", "s21", "s22",
    "s23", "s24", "s25", "s26", "s27", "s28", "s29", "s30", "s31", "f0", "f1", "f2", "f3", "f4",
    "f5", "f6", "f7",
];

/// Names for 64bit ARM CPU registers by register number.
static ARM64: &'static [&'static str] = &[
    "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10", "x11", "x12", "x13", "x14",
    "x15", "x16", "x17", "x18", "x19", "x20", "x21", "x22", "x23", "x24", "x25", "x26", "x27",
    "x28", "x29", "x30", "sp", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "",
    "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "v0", "v1", "v2", "v3", "v4", "v5",
    "v6", "v7", "v8", "v9", "v10", "v11", "v12", "v13", "v14", "v15", "v16", "v17", "v18", "v19",
    "v20", "v21", "v22", "v23", "v24", "v25", "v26", "v27", "v28", "v29", "v30", "v31",
];
