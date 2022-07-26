//! Handling of Call Frame Information (stack frame info).
//!
//! The root type exposed by this crate is [`CfiCache`], which offers a high-level API to extract
//! CFI from object files and serialize a format that the Breakpad processor can understand.
//!
//! # Background
//!
//! Call Frame Information (CFI) is used by the [processor] to improve the quality of stacktraces
//! during stackwalking. When the executable was compiled with frame pointer omission, the call
//! stack does not contain sufficient information to resolve frames on its own. CFI contains
//! programs that can calculate the base address of a frame based on register values of the current
//! frame.
//!
//! Without CFI, the stackwalker needs to scan the stack memory for values that look like valid base
//! addresses. This fequently yields false-positives.
//!
//! [processor]: ../processor/index.html
//! [`CfiCache`]: struct.CfiCache.html

use std::collections::HashMap;
use std::convert::TryInto;
use std::error::Error;
use std::fmt;
use std::io::{self, Write};
use std::ops::Range;

use thiserror::Error;

use symbolic_common::{Arch, ByteView, CpuFamily, UnknownArchError};
use symbolic_debuginfo::breakpad::{BreakpadError, BreakpadObject, BreakpadStackRecord};
use symbolic_debuginfo::dwarf::gimli::{
    BaseAddresses, CfaRule, CieOrFde, DebugFrame, EhFrame, Error as GimliError,
    FrameDescriptionEntry, Reader, ReaderOffset, Register, RegisterRule, UnwindContext,
    UnwindSection,
};
use symbolic_debuginfo::dwarf::Dwarf;
use symbolic_debuginfo::macho::{
    CompactCfiOp, CompactCfiRegister, CompactUnwindInfoIter, CompactUnwindOp, MachError, MachObject,
};
use symbolic_debuginfo::pdb::pdb::{self, FallibleIterator, FrameData, Rva, StringTable};
use symbolic_debuginfo::pdb::PdbObject;
use symbolic_debuginfo::pe::{PeObject, RuntimeFunction, StackFrameOffset, UnwindOperation};
use symbolic_debuginfo::{Object, ObjectError, ObjectLike};

/// The magic file preamble to identify cficache files.
///
/// Files with version < 2 do not have the full preamble with magic+version, but rather start
/// straight away with a `STACK` record.
/// The magic here is a `u32` corresponding to the big-endian `CFIC`.
/// It will be written and read using native endianness, so mismatches between writer/reader will
/// result in a [`CfiErrorKind::BadFileMagic`] error.
pub const CFICACHE_MAGIC: u32 = u32::from_be_bytes(*b"CFIC");

/// The latest version of the file format.
pub const CFICACHE_LATEST_VERSION: u32 = 2;

// The preamble are 8 bytes, a 4-byte magic and 4 bytes for the version.
// The 4-byte magic should be read as little endian to check for endian mismatch.

// Version history:
//
// 1: Initial ASCII-only implementation
// 2: Implementation with a versioned preamble

/// Used to detect empty runtime function entries in PEs.
const EMPTY_FUNCTION: RuntimeFunction = RuntimeFunction {
    begin_address: 0,
    end_address: 0,
    unwind_info_address: 0,
};

/// Names for x86 CPU registers by register number.
static I386: &[&str] = &[
    "$eax", "$ecx", "$edx", "$ebx", "$esp", "$ebp", "$esi", "$edi", "$eip", "$eflags", "$unused1",
    "$st0", "$st1", "$st2", "$st3", "$st4", "$st5", "$st6", "$st7", "$unused2", "$unused3",
    "$xmm0", "$xmm1", "$xmm2", "$xmm3", "$xmm4", "$xmm5", "$xmm6", "$xmm7", "$mm0", "$mm1", "$mm2",
    "$mm3", "$mm4", "$mm5", "$mm6", "$mm7", "$fcw", "$fsw", "$mxcsr", "$es", "$cs", "$ss", "$ds",
    "$fs", "$gs", "$unused4", "$unused5", "$tr", "$ldtr",
];

/// Names for x86_64 CPU registers by register number.
static X86_64: &[&str] = &[
    "$rax", "$rdx", "$rcx", "$rbx", "$rsi", "$rdi", "$rbp", "$rsp", "$r8", "$r9", "$r10", "$r11",
    "$r12", "$r13", "$r14", "$r15", "$rip", "$xmm0", "$xmm1", "$xmm2", "$xmm3", "$xmm4", "$xmm5",
    "$xmm6", "$xmm7", "$xmm8", "$xmm9", "$xmm10", "$xmm11", "$xmm12", "$xmm13", "$xmm14", "$xmm15",
    "$st0", "$st1", "$st2", "$st3", "$st4", "$st5", "$st6", "$st7", "$mm0", "$mm1", "$mm2", "$mm3",
    "$mm4", "$mm5", "$mm6", "$mm7", "$rflags", "$es", "$cs", "$ss", "$ds", "$fs", "$gs",
    "$unused1", "$unused2", "$fs.base", "$gs.base", "$unused3", "$unused4", "$tr", "$ldtr",
    "$mxcsr", "$fcw", "$fsw",
];

/// Names for 32bit ARM CPU registers by register number.
static ARM: &[&str] = &[
    "r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "r12", "sp", "lr",
    "pc", "f0", "f1", "f2", "f3", "f4", "f5", "f6", "f7", "fps", "cpsr", "", "", "", "", "", "",
    "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "",
    "", "", "", "", "", "", "", "", "s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9",
    "s10", "s11", "s12", "s13", "s14", "s15", "s16", "s17", "s18", "s19", "s20", "s21", "s22",
    "s23", "s24", "s25", "s26", "s27", "s28", "s29", "s30", "s31", "f0", "f1", "f2", "f3", "f4",
    "f5", "f6", "f7",
];

/// Names for 64bit ARM CPU registers by register number.
static ARM64: &[&str] = &[
    "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10", "x11", "x12", "x13", "x14",
    "x15", "x16", "x17", "x18", "x19", "x20", "x21", "x22", "x23", "x24", "x25", "x26", "x27",
    "x28", "x29", "x30", "sp", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "",
    "", "", "", "", "", "", "", "", "", "", "", "", "", "", "", "v0", "v1", "v2", "v3", "v4", "v5",
    "v6", "v7", "v8", "v9", "v10", "v11", "v12", "v13", "v14", "v15", "v16", "v17", "v18", "v19",
    "v20", "v21", "v22", "v23", "v24", "v25", "v26", "v27", "v28", "v29", "v30", "v31",
];

/// Names for MIPS CPU registers by register number.
static MIPS: &[&str] = &[
    "$zero", "$at", "$v0", "$v1", "$a0", "$a1", "$a2", "$a3", "$t0", "$t1", "$t2", "$t3", "$t4",
    "$t5", "$t6", "$t7", "$s0", "$s1", "$s2", "$s3", "$s4", "$s5", "$s6", "$s7", "$t8", "$t9",
    "$k0", "$k1", "$gp", "$sp", "$fp", "$ra", "$lo", "$hi", "$pc", "$f0", "$f2", "$f3", "$f4",
    "$f5", "$f6", "$f7", "$f8", "$f9", "$f10", "$f11", "$f12", "$f13", "$f14", "$f15", "$f16",
    "$f17", "$f18", "$f19", "$f20", "$f21", "$f22", "$f23", "$f24", "$f25", "$f26", "$f27", "$f28",
    "$f29", "$f30", "$f31", "$fcsr", "$fir",
];

/// The error type for [`CfiError`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CfiErrorKind {
    /// Required debug sections are missing in the `Object` file.
    MissingDebugInfo,

    /// The debug information in the `Object` file is not supported.
    UnsupportedDebugFormat,

    /// The debug information in the `Object` file is invalid.
    BadDebugInfo,

    /// The `Object`s architecture is not supported by symbolic.
    UnsupportedArch,

    /// CFI for an invalid address outside the mapped range was encountered.
    InvalidAddress,

    /// Generic error when writing CFI information, likely IO.
    WriteFailed,

    /// Invalid magic bytes in the cfi cache header.
    BadFileMagic,
}

impl fmt::Display for CfiErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingDebugInfo => write!(f, "missing cfi debug sections"),
            Self::UnsupportedDebugFormat => write!(f, "unsupported debug format"),
            Self::BadDebugInfo => write!(f, "bad debug information"),
            Self::UnsupportedArch => write!(f, "unsupported architecture"),
            Self::InvalidAddress => write!(f, "invalid cfi address"),
            Self::WriteFailed => write!(f, "failed to write cfi"),
            Self::BadFileMagic => write!(f, "bad cfi cache magic"),
        }
    }
}

/// An error returned by [`AsciiCfiWriter`](struct.AsciiCfiWriter.html).
#[derive(Debug, Error)]
#[error("{kind}")]
pub struct CfiError {
    kind: CfiErrorKind,
    #[source]
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl CfiError {
    /// Creates a new CFI error from a known kind of error as well as an
    /// arbitrary error payload.
    fn new<E>(kind: CfiErrorKind, source: E) -> Self
    where
        E: Into<Box<dyn Error + Send + Sync>>,
    {
        let source = Some(source.into());
        Self { kind, source }
    }

    /// Returns the corresponding [`CfiErrorKind`] for this error.
    pub fn kind(&self) -> CfiErrorKind {
        self.kind
    }
}

impl From<CfiErrorKind> for CfiError {
    fn from(kind: CfiErrorKind) -> Self {
        Self { kind, source: None }
    }
}

impl From<io::Error> for CfiError {
    fn from(e: io::Error) -> Self {
        Self::new(CfiErrorKind::WriteFailed, e)
    }
}

impl From<UnknownArchError> for CfiError {
    fn from(_: UnknownArchError) -> Self {
        // UnknownArchError does not carry any useful information
        CfiErrorKind::UnsupportedArch.into()
    }
}

impl From<BreakpadError> for CfiError {
    fn from(e: BreakpadError) -> Self {
        Self::new(CfiErrorKind::BadDebugInfo, e)
    }
}

impl From<ObjectError> for CfiError {
    fn from(e: ObjectError) -> Self {
        Self::new(CfiErrorKind::BadDebugInfo, e)
    }
}

impl From<pdb::Error> for CfiError {
    fn from(e: pdb::Error) -> Self {
        Self::new(CfiErrorKind::BadDebugInfo, e)
    }
}

impl From<GimliError> for CfiError {
    fn from(e: GimliError) -> Self {
        Self::new(CfiErrorKind::BadDebugInfo, e)
    }
}

impl From<MachError> for CfiError {
    fn from(e: MachError) -> Self {
        Self::new(CfiErrorKind::BadDebugInfo, e)
    }
}

/// Temporary helper trait to set the address size on any unwind section.
trait UnwindSectionExt<R>: UnwindSection<R>
where
    R: Reader,
{
    fn set_address_size(&mut self, address_size: u8);
}

impl<R: Reader> UnwindSectionExt<R> for EhFrame<R> {
    fn set_address_size(&mut self, address_size: u8) {
        self.set_address_size(address_size)
    }
}

impl<R: Reader> UnwindSectionExt<R> for DebugFrame<R> {
    fn set_address_size(&mut self, address_size: u8) {
        self.set_address_size(address_size)
    }
}

/// Context information for unwinding.
struct UnwindInfo<U> {
    arch: Arch,
    load_address: u64,
    section: U,
    bases: BaseAddresses,
}

impl<U> UnwindInfo<U> {
    pub fn new<'d: 'o, 'o, O, R>(object: &O, addr: u64, mut section: U) -> Self
    where
        O: ObjectLike<'d, 'o>,
        R: Reader,
        U: UnwindSectionExt<R>,
    {
        let arch = object.arch();
        let load_address = object.load_address();

        // CFI can have relative offsets to the virtual address of the respective debug
        // section (either `.eh_frame` or `.debug_frame`). We need to supply this offset to the
        // entries iterator before starting to interpret instructions. The other base addresses are
        // not needed for CFI.
        let bases = BaseAddresses::default().set_eh_frame(addr);

        // Based on the architecture, pointers inside eh_frame and debug_frame have different sizes.
        // Configure the section to read them appropriately.
        if let Some(pointer_size) = arch.cpu_family().pointer_size() {
            section.set_address_size(pointer_size as u8);
        }

        UnwindInfo {
            arch,
            load_address,
            section,
            bases,
        }
    }
}

/// Returns the name of a register in a given architecture used in CFI programs.
///
/// Each CPU family specifies its own register sets, wherer the registers are numbered. This
/// resolves the name of the register for the given family, if defined. Returns `None` if the
/// CPU family is unknown, or the register is not defined for the family.
///
/// **Note**: The CFI register name differs from [`ip_register_name`](CpuFamily::ip_register_name).
/// For instance, on x86-64
/// the instruction pointer is returned as `$rip` instead of just `rip`. This differentiation is
/// made to be compatible with the Google Breakpad library.
fn cfi_register_name(arch: CpuFamily, register: u16) -> Option<&'static str> {
    let index = register as usize;

    let opt = match arch {
        CpuFamily::Intel32 => I386.get(index),
        CpuFamily::Amd64 => X86_64.get(index),
        CpuFamily::Arm64 | CpuFamily::Arm64_32 => ARM64.get(index),
        CpuFamily::Arm32 => ARM.get(index),
        CpuFamily::Mips32 | CpuFamily::Mips64 => MIPS.get(index),
        _ => None,
    };

    opt.copied().filter(|name| !name.is_empty())
}
/// A service that converts call frame information (CFI) from an object file to Breakpad ASCII
/// format and writes it to the given writer.
///
/// The default way to use this writer is to create a writer, pass it to the `AsciiCfiWriter` and
/// then process an object:
///
/// ```rust,no_run
/// use symbolic_common::ByteView;
/// use symbolic_debuginfo::Object;
/// use symbolic_cfi::AsciiCfiWriter;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let view = ByteView::open("/path/to/object")?;
/// let object = Object::parse(&view)?;
///
/// let mut writer = Vec::new();
/// AsciiCfiWriter::new(&mut writer).process(&object)?;
/// # Ok(())
/// # }
/// ```
///
/// For writers that implement `Default`, there is a convenience method that creates an instance and
/// returns it right away:
///
/// ```rust,no_run
/// use symbolic_common::ByteView;
/// use symbolic_debuginfo::Object;
/// use symbolic_cfi::AsciiCfiWriter;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let view = ByteView::open("/path/to/object")?;
/// let object = Object::parse(&view)?;
///
/// let buffer = AsciiCfiWriter::<Vec<u8>>::transform(&object)?;
/// # Ok(())
/// # }
/// ```
pub struct AsciiCfiWriter<W: Write> {
    inner: W,
}

impl<W: Write> AsciiCfiWriter<W> {
    /// Creates a new `AsciiCfiWriter` that outputs to a writer.
    pub fn new(inner: W) -> Self {
        AsciiCfiWriter { inner }
    }

    /// Extracts CFI from the given object file.
    pub fn process(&mut self, object: &Object<'_>) -> Result<(), CfiError> {
        match object {
            Object::Breakpad(o) => self.process_breakpad(o),
            Object::MachO(o) => self.process_macho(o),
            Object::Elf(o) => self.process_dwarf(o, false),
            Object::Pdb(o) => self.process_pdb(o),
            Object::Pe(o) => self.process_pe(o),
            Object::Wasm(o) => self.process_dwarf(o, false),
            Object::SourceBundle(_) => Ok(()),
        }
    }

    /// Returns the wrapped writer from this instance.
    pub fn into_inner(self) -> W {
        self.inner
    }

    fn process_breakpad(&mut self, object: &BreakpadObject<'_>) -> Result<(), CfiError> {
        for record in object.stack_records() {
            match record? {
                BreakpadStackRecord::Cfi(r) => {
                    writeln!(
                        self.inner,
                        "STACK CFI INIT {:x} {:x} {}",
                        r.start, r.size, r.init_rules
                    )?;

                    for d in r.deltas().flatten() {
                        writeln!(self.inner, "STACK CFI {:x} {}", d.address, d.rules)?;
                    }

                    Ok(())
                }
                BreakpadStackRecord::Win(r) => writeln!(
                    self.inner,
                    "STACK WIN {} {:x} {:x} {:x} {:x} {:x} {:x} {:x} {:x} {} {}",
                    r.ty as usize,
                    r.code_start,
                    r.code_size,
                    r.prolog_size,
                    r.epilog_size,
                    r.params_size,
                    r.saved_regs_size,
                    r.locals_size,
                    r.max_stack_size,
                    if r.program_string.is_some() { "1" } else { "0" },
                    if let Some(ps) = r.program_string {
                        ps
                    } else if r.uses_base_pointer {
                        "1"
                    } else {
                        "0"
                    }
                ),
            }?
        }

        Ok(())
    }

    fn process_macho<'d>(&mut self, object: &MachObject<'d>) -> Result<(), CfiError> {
        let compact_unwind_info = object.compact_unwind_info()?;

        // If we have compact_unwind_info, then any important entries in
        // the eh_frame section will be explicitly requested by the
        // Compact Unwinding Info. So skip processing that section for now.
        let should_skip_eh_frame = compact_unwind_info.is_some();
        let result = self.process_dwarf(object, should_skip_eh_frame);

        if let Some(compact_unwind_info) = compact_unwind_info {
            let eh_section = object.section("eh_frame");
            let eh_frame_info = eh_section.as_ref().map(|section| {
                let endian = object.endianity();
                let frame = EhFrame::new(&section.data, endian);
                UnwindInfo::new(object, section.address, frame)
            });
            self.read_compact_unwind_info(compact_unwind_info, eh_frame_info.as_ref(), object)?;
        }
        result
    }

    fn process_dwarf<'d: 'o, 'o, O>(
        &mut self,
        object: &O,
        skip_eh_frame: bool,
    ) -> Result<(), CfiError>
    where
        O: ObjectLike<'d, 'o> + Dwarf<'o>,
    {
        let endian = object.endianity();

        // First load information from the DWARF debug_frame section. It does not contain any
        // references to other DWARF sections.
        // Don't return on error because eh_frame can contain some information
        let debug_frame_result = if let Some(section) = object.section("debug_frame") {
            let frame = DebugFrame::new(&section.data, endian);
            let info = UnwindInfo::new(object, section.address, frame);
            self.read_cfi(&info)
        } else {
            Ok(())
        };

        if !skip_eh_frame {
            if let Some(section) = object.section("eh_frame") {
                // Independently, Linux C++ exception handling information can also provide unwind info.
                let frame = EhFrame::new(&section.data, endian);
                let info = UnwindInfo::new(object, section.address, frame);
                self.read_cfi(&info)?;
            }
        }

        debug_frame_result
    }

    fn read_compact_unwind_info<'d, U, R>(
        &mut self,
        mut iter: CompactUnwindInfoIter<'d>,
        eh_frame_info: Option<&UnwindInfo<U>>,
        object: &MachObject<'d>,
    ) -> Result<(), CfiError>
    where
        R: Reader + Eq,
        U: UnwindSection<R>,
    {
        fn write_reg_name<W: Write>(
            writer: &mut W,
            register: CompactCfiRegister,
            iter: &CompactUnwindInfoIter,
            cpu_family: CpuFamily,
        ) -> Result<(), CfiError> {
            if register.is_cfa() {
                write!(writer, ".cfa")?;
            } else if register == CompactCfiRegister::instruction_pointer() {
                write!(writer, ".ra")?;
            } else {
                // For whatever reason breakpad doesn't prefix registers with $ on ARM.
                match cpu_family {
                    CpuFamily::Arm32 | CpuFamily::Arm64 | CpuFamily::Arm64_32 => {
                        write!(writer, "{}", register.name(iter).unwrap())?;
                    }
                    _ => {
                        write!(writer, "${}", register.name(iter).unwrap())?;
                    }
                }
            }
            Ok(())
        }
        // Preload the symbols as this is expensive to do in the loop.
        let symbols = object.symbol_map();
        let cpu_family = object.arch().cpu_family();

        // Initialize an unwind context once and reuse it for the entire section.
        let mut ctx = UnwindContext::new();

        while let Some(entry) = iter.next()? {
            if entry.len == 0 {
                // We saw some duplicate entries (which yield entries with `len == 0`) for example
                // in `libsystem_kernel.dylib`. In this case just skip the zero-length entry.
                continue;
            }
            match entry.instructions(&iter) {
                CompactUnwindOp::None => {
                    // We have seen some of these `CompactUnwindOp::None` correspond to some tiny
                    // stackless functions, such as `__kill` from `libsystem_kernel.dylib` or similar.
                    //
                    // Because they don't have a normal CFI record, we would fall back to frame pointers
                    // or stack scanning when unwinding, which will cause us to skip the caller
                    // frame, or fail unwinding completely.
                    //
                    // To overcome this problem we will emit a CFI record that basically says that
                    // the function has no stack space of its own. Since these compact unwind records
                    // can be the result of merging multiple of these adjacent functions, they can
                    // span more instructions/bytes than one single symbol.
                    //
                    // This can potentially lead to false positives. However in that case, the unwinding
                    // code will detect the bogus return address and fall back to frame pointers or
                    // scanning either way.

                    let start_addr = entry.instruction_address;
                    match cpu_family {
                        CpuFamily::Amd64 => {
                            writeln!(
                                self.inner,
                                "STACK CFI INIT {:x} {:x} .cfa: $rsp 8 + .ra: .cfa -8 + ^",
                                start_addr, entry.len
                            )?;
                        }
                        CpuFamily::Arm64 => {
                            // Assume this is a stackless leaf, return address is in lr (x30).
                            writeln!(
                                self.inner,
                                "STACK CFI INIT {:x} {:x} .cfa: sp .ra: x30",
                                start_addr, entry.len
                            )?;
                        }
                        _ => {
                            // Do nothing
                        }
                    }
                }
                CompactUnwindOp::UseDwarfFde { offset_in_eh_frame } => {
                    // We need to grab the CFI info from the eh_frame section
                    if let Some(info) = eh_frame_info {
                        let offset = U::Offset::from(R::Offset::from_u32(offset_in_eh_frame));
                        if let Ok(fde) =
                            info.section
                                .fde_from_offset(&info.bases, offset, U::cie_from_offset)
                        {
                            let start_addr = entry.instruction_address.into();
                            let sym_name = symbols.lookup(start_addr).and_then(|sym| sym.name());

                            if sym_name == Some("_sigtramp") && cpu_family == CpuFamily::Amd64 {
                                // This specific function has some hand crafted dwarf expressions
                                // that we currently can't process. They encode how to restore the
                                // registers from a machine context accessible via `$rbx`
                                // See: https://github.com/apple/darwin-libplatform/blob/215b09856ab5765b7462a91be7076183076600df/src/setjmp/x86_64/_sigtramp.s#L198-L258

                                let mc_offset = 48;
                                let rbp_offset = 64;
                                let rsp_offset = 72;
                                let rip_offset = 144;

                                write!(
                                    self.inner,
                                    "STACK CFI INIT {:x} {:x} ",
                                    start_addr, entry.len
                                )?;
                                write!(
                                    self.inner,
                                    "$rbp: $rbx {} + ^ {} + ^ ",
                                    mc_offset, rbp_offset
                                )?;
                                write!(
                                    self.inner,
                                    ".cfa: $rbx {} + ^ {} + ^ ",
                                    mc_offset, rsp_offset,
                                )?;
                                writeln!(
                                    self.inner,
                                    ".ra: $rbx {} + ^ {} + ^",
                                    mc_offset, rip_offset
                                )?;
                            } else {
                                self.process_fde(info, &mut ctx, &fde)?;
                            }
                        }
                    }
                }
                CompactUnwindOp::CfiOps(ops) => {
                    // We just need to output a bunch of CFI expressions in a single CFI INIT
                    let mut line = Vec::new();
                    let start_addr = entry.instruction_address;
                    let length = entry.len;
                    write!(line, "STACK CFI INIT {:x} {:x} ", start_addr, length)?;

                    for instruction in ops {
                        // These two operations differ only in whether there should
                        // be a deref (^) at the end, so we can flatten away their
                        // differences and merge paths.
                        let (dest_reg, src_reg, offset, should_deref) = match instruction {
                            CompactCfiOp::RegisterAt {
                                dest_reg,
                                src_reg,
                                offset_from_src,
                            } => (dest_reg, src_reg, offset_from_src, true),
                            CompactCfiOp::RegisterIs {
                                dest_reg,
                                src_reg,
                                offset_from_src,
                            } => (dest_reg, src_reg, offset_from_src, false),
                        };

                        write_reg_name(&mut line, dest_reg, &iter, cpu_family)?;
                        write!(line, ": ")?;
                        write_reg_name(&mut line, src_reg, &iter, cpu_family)?;
                        write!(line, " {} + ", offset)?;
                        if should_deref {
                            write!(line, "^ ")?;
                        }
                    }

                    let line = line.strip_suffix(b" ").unwrap_or(&line);

                    self.inner
                        .write_all(line)
                        .and_then(|_| writeln!(self.inner))?;
                }
            }
        }
        Ok(())
    }

    fn read_cfi<U, R>(&mut self, info: &UnwindInfo<U>) -> Result<(), CfiError>
    where
        R: Reader + Eq,
        U: UnwindSection<R>,
    {
        // Initialize an unwind context once and reuse it for the entire section.
        let mut ctx = UnwindContext::new();

        let mut entries = info.section.entries(&info.bases);
        while let Some(entry) = entries.next()? {
            // We skip all Common Information Entries and only process Frame Description Items here.
            // The iterator yields partial FDEs which need their associated CIE passed in via a
            // callback. This function is provided by the UnwindSection (frame), which then parses
            // the CIE and returns it for the FDE.
            if let CieOrFde::Fde(partial_fde) = entry {
                if let Ok(fde) = partial_fde.parse(U::cie_from_offset) {
                    self.process_fde(info, &mut ctx, &fde)?
                }
            }
        }

        Ok(())
    }

    fn process_fde<R, U>(
        &mut self,
        info: &UnwindInfo<U>,
        ctx: &mut UnwindContext<R>,
        fde: &FrameDescriptionEntry<R>,
    ) -> Result<(), CfiError>
    where
        R: Reader + Eq,
        U: UnwindSection<R>,
    {
        // Retrieves the register that specifies the return address. We need to assign a special
        // format to this register for Breakpad.
        let ra = fde.cie().return_address_register();

        // Interpret all DWARF instructions of this Frame Description Entry. This gives us an unwind
        // table that contains rules for retrieving registers at every instruction address. These
        // rules can directly be transcribed to breakpad STACK CFI records.
        let mut table = fde.rows(&info.section, &info.bases, ctx)?;

        // Collect all rows first, as we need to know the final end address in order to write the
        // CFI INIT record describing the extent of the whole unwind table.
        let mut rows = Vec::new();
        loop {
            match table.next_row() {
                Ok(None) => break,
                Ok(Some(row)) => rows.push(row.clone()),
                Err(GimliError::UnknownCallFrameInstruction(_)) => continue,
                // NOTE: Temporary workaround for https://github.com/gimli-rs/gimli/pull/487
                Err(GimliError::TooManyRegisterRules) => continue,
                Err(e) => return Err(e.into()),
            }
        }

        if let Some(first_row) = rows.first() {
            // Calculate the start address and total range covered by the CFI INIT record and its
            // subsequent CFI records. This information will be written into the CFI INIT record.
            let start = first_row.start_address();
            let length = rows.last().unwrap().end_address() - start;

            // Verify that the CFI entry is in range of the mapped module. Zero values are a special
            // case and seem to indicate that the entry is no longer valid. However, also skip other
            // entries since the rest of the file may still be valid.
            if start < info.load_address {
                return Ok(());
            }

            // Every register rule in the table will be cached so that it can be compared with
            // subsequent occurrences. Only registers with changed rules will be written.
            let mut rule_cache = HashMap::new();
            let mut cfa_cache = None;

            // Write records for every entry in the unwind table.
            for row in &rows {
                let mut written = false;
                let mut line = Vec::new();

                // Depending on whether this is the first row or any subsequent row, print a INIT or
                // normal STACK CFI record.
                if row.start_address() == start {
                    let start_addr = start - info.load_address;
                    write!(line, "STACK CFI INIT {:x} {:x}", start_addr, length)?;
                } else {
                    let start_addr = row.start_address() - info.load_address;
                    write!(line, "STACK CFI {:x}", start_addr)?;
                }

                // Write the mandatory CFA rule for this row, followed by optional register rules.
                // The actual formatting of the rules depends on their rule type.
                if cfa_cache != Some(row.cfa()) {
                    cfa_cache = Some(row.cfa());
                    written |= Self::write_cfa_rule(&mut line, info.arch, row.cfa())?;
                }

                // Print only registers that have changed rules to their previous occurrence to
                // reduce the number of rules per row. Then, cache the new occurrence for the next
                // row.
                let mut ra_written = false;
                for &(register, ref rule) in row.registers() {
                    if !rule_cache.get(&register).map_or(false, |c| c == &rule) {
                        rule_cache.insert(register, rule);
                        if register == ra {
                            ra_written = true;
                        }
                        written |=
                            Self::write_register_rule(&mut line, info.arch, register, rule, ra)?;
                    }
                }
                // On MIPS: if no explicit rule was encountered for the return address,
                // emit a rule stating that the return address should be recovered from the
                // $ra register.
                if row.start_address() == start
                    && !ra_written
                    && matches!(info.arch, Arch::Mips | Arch::Mips64)
                {
                    write!(line, " .ra: $ra")?;
                }

                if written {
                    self.inner
                        .write_all(&line)
                        .and_then(|_| writeln!(self.inner))?;
                }
            }
        }

        Ok(())
    }

    fn write_cfa_rule<R: Reader, T: Write>(
        mut target: T,
        arch: Arch,
        rule: &CfaRule<R>,
    ) -> Result<bool, CfiError> {
        let formatted = match rule {
            CfaRule::RegisterAndOffset { register, offset } => {
                match cfi_register_name(arch.cpu_family(), register.0) {
                    Some(register) => format!("{} {} +", register, *offset),
                    None => return Ok(false),
                }
            }
            CfaRule::Expression(_) => return Ok(false),
        };

        write!(target, " .cfa: {}", formatted)?;
        Ok(true)
    }

    fn write_register_rule<R: Reader, T: Write>(
        mut target: T,
        arch: Arch,
        register: Register,
        rule: &RegisterRule<R>,
        ra: Register,
    ) -> Result<bool, CfiError> {
        let formatted = match rule {
            RegisterRule::Undefined => return Ok(false),
            RegisterRule::SameValue => match cfi_register_name(arch.cpu_family(), register.0) {
                Some(reg) => reg.into(),
                None => return Ok(false),
            },
            RegisterRule::Offset(offset) => format!(".cfa {} + ^", offset),
            RegisterRule::ValOffset(offset) => format!(".cfa {} +", offset),
            RegisterRule::Register(register) => {
                match cfi_register_name(arch.cpu_family(), register.0) {
                    Some(reg) => reg.into(),
                    None => return Ok(false),
                }
            }
            RegisterRule::Expression(_) => return Ok(false),
            RegisterRule::ValExpression(_) => return Ok(false),
            RegisterRule::Architectural => return Ok(false),
        };

        // Breakpad requires an explicit name for the return address register. In all other cases,
        // we use platform specific names for each register as specified by Breakpad.
        let register_name = if register == ra {
            ".ra"
        } else {
            match cfi_register_name(arch.cpu_family(), register.0) {
                Some(reg) => reg,
                None => return Ok(false),
            }
        };

        write!(target, " {}: {}", register_name, formatted)?;
        Ok(true)
    }

    fn process_pdb(&mut self, pdb: &PdbObject<'_>) -> Result<(), CfiError> {
        let mut pdb = pdb.inner().write();
        let frame_table = pdb.frame_table()?;
        let address_map = pdb.address_map()?;

        // See `PdbDebugSession::build`.
        let string_table = match pdb.string_table() {
            Ok(string_table) => Some(string_table),
            Err(pdb::Error::StreamNameNotFound) => None,
            Err(e) => return Err(e.into()),
        };

        let mut frames = frame_table.iter();
        let mut last_frame: Option<FrameData> = None;

        while let Some(frame) = frames.next()? {
            // Frame data information sometimes contains code_size values close to the maximum `u32`
            // value, such as `0xffffff6e`. Documentation does not describe the meaning of such
            // values, but clearly they are not actual code sizes. Since these values also always
            // occur with a `code_start` close to the end of a function's code range, it seems
            // likely that these belong to the function epilog and code_size has a different meaning
            // in this case. Until this value is understood, skip these entries.
            if frame.code_size > i32::max_value() as u32 {
                continue;
            }

            // Only print a stack record if information has changed from the last list. It is
            // surprisingly common (especially in system library PDBs) for DIA to return a series of
            // identical IDiaFrameData objects. For kernel32.pdb from Windows XP SP2 on x86, this
            // check reduces the size of the dumped symbol file by a third.
            if let Some(ref last) = last_frame {
                if frame.ty == last.ty
                    && frame.code_start == last.code_start
                    && frame.code_size == last.code_size
                    && frame.prolog_size == last.prolog_size
                {
                    continue;
                }
            }

            // Address ranges need to be translated to the RVA address space. The prolog and the
            // code portions of the frame have to be treated independently as they may have
            // independently changed in size, or may even have been split.
            let prolog_size = u32::from(frame.prolog_size);
            let prolog_end = frame.code_start + prolog_size;
            let code_end = frame.code_start + frame.code_size;

            let mut prolog_ranges = address_map
                .rva_ranges(frame.code_start..prolog_end)
                .collect::<Vec<_>>();

            let mut code_ranges = address_map
                .rva_ranges(prolog_end..code_end)
                .collect::<Vec<_>>();

            // Check if the prolog and code bytes remain contiguous and only output a single record.
            // This is only done for compactness of the symbol file. Since the majority of PDBs
            // other than the Kernel do not have translated address spaces, this will be true for
            // most records.
            let is_contiguous = prolog_ranges.len() == 1
                && code_ranges.len() == 1
                && prolog_ranges[0].end == code_ranges[0].start;

            if is_contiguous {
                self.write_pdb_stackinfo(
                    string_table.as_ref(),
                    &frame,
                    prolog_ranges[0].start,
                    code_ranges[0].end,
                    prolog_ranges[0].end - prolog_ranges[0].start,
                )?;
            } else {
                // Output the prolog first, and then code frames in RVA order.
                prolog_ranges.sort_unstable_by_key(|range| range.start);
                code_ranges.sort_unstable_by_key(|range| range.start);

                for Range { start, end } in prolog_ranges {
                    self.write_pdb_stackinfo(
                        string_table.as_ref(),
                        &frame,
                        start,
                        end,
                        end - start,
                    )?;
                }

                for Range { start, end } in code_ranges {
                    self.write_pdb_stackinfo(string_table.as_ref(), &frame, start, end, 0)?;
                }
            }

            last_frame = Some(frame);
        }

        Ok(())
    }

    fn write_pdb_stackinfo(
        &mut self,
        string_table: Option<&StringTable<'_>>,
        frame: &FrameData,
        start: Rva,
        end: Rva,
        prolog_size: u32,
    ) -> Result<(), CfiError> {
        let code_size = end - start;
        let has_program = frame.program.is_some() && string_table.is_some();

        write!(
            self.inner,
            "STACK WIN {:x} {:x} {:x} {:x} {:x} {:x} {:x} {:x} {:x} {} ",
            frame.ty as u8,
            start.0,
            code_size,
            prolog_size,
            0, // epilog_size
            frame.params_size,
            frame.saved_regs_size,
            frame.locals_size,
            frame.max_stack_size.unwrap_or(0),
            if has_program { 1 } else { 0 },
        )?;

        match frame.program {
            Some(ref prog_ref) => {
                let string_table = match string_table {
                    Some(string_table) => string_table,
                    None => return Ok(writeln!(self.inner)?),
                };

                let program_string = prog_ref.to_string_lossy(string_table)?;

                writeln!(self.inner, "{}", program_string.trim())?;
            }
            None => {
                writeln!(
                    self.inner,
                    "{}",
                    if frame.uses_base_pointer { 1 } else { 0 }
                )?;
            }
        }

        Ok(())
    }

    fn process_pe(&mut self, pe: &PeObject<'_>) -> Result<(), CfiError> {
        let sections = pe.sections();
        let exception_data = match pe.exception_data() {
            Some(data) => data,
            None => return Ok(()),
        };

        let mut cfa_reg = Vec::new();
        let mut saved_regs = Vec::new();
        let mut unwind_codes = Vec::new();

        'functions: for function_result in exception_data.functions() {
            let function =
                function_result.map_err(|e| CfiError::new(CfiErrorKind::BadDebugInfo, e))?;

            // Exception directories can contain zeroed out sections which need to be skipped.
            // Neither their start/end RVA nor the unwind info RVA is valid.
            if function == EMPTY_FUNCTION {
                continue;
            }

            // The minimal stack size is 8 for RIP
            let mut stack_size: u32 = 8;
            // Special handling for machine frames
            let mut machine_frame_offset = 0;

            if function.end_address < function.begin_address {
                continue;
            }

            cfa_reg.clear();
            saved_regs.clear();

            let mut next_function = Some(function);
            while let Some(next) = next_function {
                let unwind_info = exception_data
                    .get_unwind_info(next, sections)
                    .map_err(|e| CfiError::new(CfiErrorKind::BadDebugInfo, e))?;

                unwind_codes.clear();
                for code_result in unwind_info.unwind_codes() {
                    // Due to variable length encoding of operator codes, there is little point in
                    // continuing after this. Other functions in this object file can be valid, so
                    // swallow the error and continue with the next function.
                    let code = match code_result {
                        Ok(code) => code,
                        Err(_) => {
                            continue 'functions;
                        }
                    };
                    unwind_codes.push(code);
                }

                // The unwind codes are saved in reverse order.
                for code in unwind_codes.iter().rev() {
                    match code.operation {
                        UnwindOperation::SaveNonVolatile(reg, offset) => {
                            match offset {
                                // If the Frame Register field in the UNWIND_INFO is zero,
                                // this offset is from RSP.
                                StackFrameOffset::RSP(offset) => {
                                    write!(
                                        &mut saved_regs,
                                        " {}: .cfa {} - ^",
                                        reg.name(),
                                        stack_size.saturating_sub(offset)
                                    )?;
                                }
                                // If the Frame Register field is nonzero, this offset is from where
                                // RSP was located when the FP register was established.
                                // It equals the FP register minus the FP register offset
                                // (16 * the scaled frame register offset in the UNWIND_INFO).
                                StackFrameOffset::FP(offset) => {
                                    write!(
                                        &mut saved_regs,
                                        " {}: {} {} + ^",
                                        reg.name(),
                                        unwind_info.frame_register.name(),
                                        offset.saturating_sub(unwind_info.frame_register_offset)
                                    )?;
                                }
                            };
                        }
                        UnwindOperation::PushNonVolatile(reg) => {
                            // $reg = .cfa - current_offset
                            stack_size += 8;
                            write!(&mut saved_regs, " {}: .cfa {} - ^", reg.name(), stack_size)?;
                        }
                        UnwindOperation::Alloc(size) => {
                            stack_size += size;
                        }
                        UnwindOperation::SetFPRegister => {
                            // Establish the frame pointer register by setting the register to some
                            // offset of the current RSP. The offset is equal to the Frame Register
                            // offset field in the UNWIND_INFO.
                            let offset =
                                stack_size.saturating_sub(unwind_info.frame_register_offset);
                            // Set the `.cfa = $fp + offset`
                            write!(
                                &mut cfa_reg,
                                ".cfa: {} {} +",
                                unwind_info.frame_register.name(),
                                offset
                            )?;
                        }

                        UnwindOperation::PushMachineFrame(is_error) => {
                            let rsp_offset = stack_size + 16;
                            let rip_offset = stack_size + 40;
                            write!(
                                &mut saved_regs,
                                " $rsp: .cfa {} - ^ .ra: .cfa {} - ^",
                                rsp_offset, rip_offset,
                            )?;
                            stack_size += 40;
                            machine_frame_offset = stack_size;
                            stack_size += if is_error { 8 } else { 0 };
                        }
                        _ => {
                            // All other codes do not modify RSP
                        }
                    }
                }

                next_function = unwind_info.chained_info;
            }

            if cfa_reg.is_empty() {
                write!(&mut cfa_reg, ".cfa: $rsp {} +", stack_size)?;
            }
            if machine_frame_offset == 0 {
                write!(&mut saved_regs, " .ra: .cfa 8 - ^")?;
            }

            write!(
                self.inner,
                "STACK CFI INIT {:x} {:x} ",
                function.begin_address,
                function.end_address - function.begin_address,
            )?;
            self.inner.write_all(&cfa_reg)?;
            self.inner.write_all(&saved_regs)?;
            writeln!(self.inner)?;
        }

        Ok(())
    }
}

impl<W: Write + Default> AsciiCfiWriter<W> {
    /// Extracts CFI from the given object and pipes it to a new writer instance.
    pub fn transform(object: &Object<'_>) -> Result<W, CfiError> {
        let mut writer = Default::default();
        AsciiCfiWriter::new(&mut writer).process(object)?;
        Ok(writer)
    }
}

struct CfiCacheV1<'a> {
    byteview: ByteView<'a>,
}

impl<'a> CfiCacheV1<'a> {
    pub fn raw(&self) -> &[u8] {
        &self.byteview
    }
}

enum CfiCacheInner<'a> {
    Unversioned(CfiCacheV1<'a>),
    Versioned(u32, CfiCacheV1<'a>),
}

/// A cache file for call frame information (CFI).
///
/// The default way to use this cache is to construct it from an `Object` and save it to a file.
/// Then, load it from the file and pass it to the minidump processor.
///
/// ```rust,no_run
/// use std::fs::File;
/// use symbolic_common::ByteView;
/// use symbolic_debuginfo::Object;
/// use symbolic_cfi::CfiCache;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let view = ByteView::open("/path/to/object")?;
/// let object = Object::parse(&view)?;
/// let cache = CfiCache::from_object(&object)?;
/// cache.write_to(File::create("my.cficache")?)?;
/// # Ok(())
/// # }
/// ```
///
/// ```rust,no_run
/// use symbolic_common::ByteView;
/// use symbolic_cfi::CfiCache;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let view = ByteView::open("my.cficache")?;
/// let cache = CfiCache::from_bytes(view)?;
/// # Ok(())
/// # }
/// ```
pub struct CfiCache<'a> {
    inner: CfiCacheInner<'a>,
}

impl CfiCache<'static> {
    /// Construct a CFI cache from an `Object`.
    pub fn from_object(object: &Object<'_>) -> Result<Self, CfiError> {
        let mut buffer = vec![];
        write_preamble(&mut buffer, CFICACHE_LATEST_VERSION)?;
        AsciiCfiWriter::new(&mut buffer).process(object)?;

        let byteview = ByteView::from_vec(buffer);
        let inner = CfiCacheInner::Versioned(CFICACHE_LATEST_VERSION, CfiCacheV1 { byteview });
        Ok(CfiCache { inner })
    }
}

fn write_preamble<W: Write>(mut writer: W, version: u32) -> Result<(), io::Error> {
    writer.write_all(&CFICACHE_MAGIC.to_ne_bytes())?;
    writer.write_all(&version.to_ne_bytes())
}

impl<'a> CfiCache<'a> {
    /// Load a symcache from a `ByteView`.
    pub fn from_bytes(byteview: ByteView<'a>) -> Result<Self, CfiError> {
        if byteview.len() == 0 || byteview.starts_with(b"STACK") {
            let inner = CfiCacheInner::Unversioned(CfiCacheV1 { byteview });
            return Ok(CfiCache { inner });
        }

        if let Some(preamble) = byteview.get(0..8) {
            let magic = u32::from_ne_bytes(preamble[0..4].try_into().unwrap());
            if magic == CFICACHE_MAGIC {
                let version = u32::from_ne_bytes(preamble[4..8].try_into().unwrap());
                let inner = CfiCacheInner::Versioned(version, CfiCacheV1 { byteview });
                return Ok(CfiCache { inner });
            }
        }

        Err(CfiErrorKind::BadFileMagic.into())
    }

    /// Returns the cache file format version.
    pub fn version(&self) -> u32 {
        match self.inner {
            CfiCacheInner::Unversioned(_) => 1,
            CfiCacheInner::Versioned(version, _) => version,
        }
    }

    /// Returns whether this cache is up-to-date.
    pub fn is_latest(&self) -> bool {
        self.version() == CFICACHE_LATEST_VERSION
    }

    /// Returns the raw buffer of the cache file.
    pub fn as_slice(&self) -> &[u8] {
        match self.inner {
            CfiCacheInner::Unversioned(ref v1) => v1.raw(),
            CfiCacheInner::Versioned(_, ref v1) => &v1.raw()[8..],
        }
    }

    /// Writes the cache to the given writer.
    pub fn write_to<W: Write>(&self, mut writer: W) -> Result<(), io::Error> {
        if let CfiCacheInner::Versioned(version, _) = self.inner {
            write_preamble(&mut writer, version)?;
        }
        io::copy(&mut self.as_slice(), &mut writer)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cfi_register_name_none() {
        assert_eq!(cfi_register_name(CpuFamily::Arm64, 33), None);
    }
}
