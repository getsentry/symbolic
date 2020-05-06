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
use std::io::{self, Write};
use std::ops::Range;

use failure::{Fail, ResultExt};

use symbolic_common::{derive_failure, Arch, ByteView, UnknownArchError};
use symbolic_debuginfo::breakpad::{BreakpadObject, BreakpadStackRecord};
use symbolic_debuginfo::dwarf::gimli::{
    BaseAddresses, CfaRule, CieOrFde, DebugFrame, EhFrame, Error, FrameDescriptionEntry, Reader,
    Register, RegisterRule, UninitializedUnwindContext, UnwindSection,
};
use symbolic_debuginfo::dwarf::Dwarf;
use symbolic_debuginfo::pdb::pdb::{self, FallibleIterator, FrameData, Rva, StringTable};
use symbolic_debuginfo::pdb::PdbObject;
use symbolic_debuginfo::pe::{PeObject, RuntimeFunction, UnwindOperation};
use symbolic_debuginfo::{Object, ObjectLike};

/// The latest version of the file format.
pub const CFICACHE_LATEST_VERSION: u32 = 1;

/// Used to detect empty runtime function entries in PEs.
const EMPTY_FUNCTION: RuntimeFunction = RuntimeFunction {
    begin_address: 0,
    end_address: 0,
    unwind_info_address: 0,
};

/// Possible error kinds of `CfiError`.
#[derive(Debug, Fail, Copy, Clone)]
pub enum CfiErrorKind {
    /// Required debug sections are missing in the `Object` file.
    #[fail(display = "missing cfi debug sections")]
    MissingDebugInfo,

    /// The debug information in the `Object` file is not supported.
    #[fail(display = "unsupported debug format")]
    UnsupportedDebugFormat,

    /// The debug information in the `Object` file is invalid.
    #[fail(display = "bad debug information")]
    BadDebugInfo,

    /// The `Object`s architecture is not supported by symbolic.
    #[fail(display = "unsupported architecture")]
    UnsupportedArch,

    /// CFI for an invalid address outside the mapped range was encountered.
    #[fail(display = "invalid cfi address")]
    InvalidAddress,

    /// Generic error when writing CFI information, likely IO.
    #[fail(display = "failed to write cfi")]
    WriteError,

    /// Invalid magic bytes in the cfi cache header.
    #[fail(display = "bad cfi cache magic")]
    BadFileMagic,
}

derive_failure!(
    CfiError,
    CfiErrorKind,
    doc = "An error returned by [`AsciiCfiWriter`](struct.AsciiCfiWriter.html)."
);

impl From<UnknownArchError> for CfiError {
    fn from(_: UnknownArchError) -> CfiError {
        CfiErrorKind::UnsupportedArch.into()
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
    pub fn new<O, R>(object: &O, addr: u64, mut section: U) -> Self
    where
        O: ObjectLike,
        R: Reader,
        U: UnwindSectionExt<R>,
    {
        let arch = object.arch();
        let load_address = object.load_address();

        // CFI information can have relative offsets to the virtual address of thir respective debug
        // section (either `.eh_frame` or `.debug_frame`). We need to supply this offset to the
        // entries iterator before starting to interpret instructions. The other base addresses are
        // not needed for CFI.
        let bases = BaseAddresses::default().set_eh_frame(addr);

        // Based on the architecture, pointers inside eh_frame and debug_frame have different sizes.
        // Configure the section to read them appropriately.
        if let Some(pointer_size) = arch.pointer_size() {
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

/// A service that converts call frame information (CFI) from an object file to Breakpad ASCII
/// format and writes it to the given writer.
///
/// The default way to use this writer is to create a writer, pass it to the `AsciiCfiWriter` and
/// then process an object:
///
/// ```rust,no_run
/// use symbolic_common::ByteView;
/// use symbolic_debuginfo::Object;
/// use symbolic_minidump::cfi::AsciiCfiWriter;
///
/// # fn main() -> Result<(), failure::Error> {
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
/// use symbolic_minidump::cfi::AsciiCfiWriter;
///
/// # fn main() -> Result<(), failure::Error> {
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
            Object::MachO(o) => self.process_dwarf(o),
            Object::Elf(o) => self.process_dwarf(o),
            Object::Pdb(o) => self.process_pdb(o),
            Object::Pe(o) => self.process_pe(o),
            Object::SourceBundle(_) => Ok(()),
        }
    }

    /// Returns the wrapped writer from this instance.
    pub fn into_inner(self) -> W {
        self.inner
    }

    fn process_breakpad(&mut self, object: &BreakpadObject<'_>) -> Result<(), CfiError> {
        for record in object.stack_records() {
            match record.context(CfiErrorKind::BadDebugInfo)? {
                BreakpadStackRecord::Cfi(r) => writeln!(self.inner, "STACK CFI {}", r.text),
                BreakpadStackRecord::Win(r) => writeln!(self.inner, "STACK WIN {}", r.text),
            }
            .context(CfiErrorKind::WriteError)?
        }

        Ok(())
    }

    fn process_dwarf<'o, O>(&mut self, object: &O) -> Result<(), CfiError>
    where
        O: ObjectLike + Dwarf<'o>,
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

        // Indepdendently, Linux C++ exception handling information can also provide unwind info.
        if let Some(section) = object.section("eh_frame") {
            let frame = EhFrame::new(&section.data, endian);
            let info = UnwindInfo::new(object, section.address, frame);
            self.read_cfi(&info)?;
        }

        debug_frame_result
    }

    fn read_cfi<U, R>(&mut self, info: &UnwindInfo<U>) -> Result<(), CfiError>
    where
        R: Reader + Eq,
        U: UnwindSection<R>,
    {
        // Initialize an unwind context once and reuse it for the entire section.
        let mut ctx = UninitializedUnwindContext::new();

        let mut entries = info.section.entries(&info.bases);
        while let Some(entry) = entries.next().context(CfiErrorKind::BadDebugInfo)? {
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
        ctx: &mut UninitializedUnwindContext<R>,
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
        let mut table = fde
            .rows(&info.section, &info.bases, ctx)
            .context(CfiErrorKind::BadDebugInfo)?;

        // Collect all rows first, as we need to know the final end address in order to write the
        // CFI INIT record describing the extent of the whole unwind table.
        let mut rows = Vec::new();
        loop {
            match table.next_row() {
                Ok(None) => break,
                Ok(Some(row)) => rows.push(row.clone()),
                Err(Error::UnknownCallFrameInstruction(_)) => continue,
                // NOTE: Temporary workaround for https://github.com/gimli-rs/gimli/pull/487
                Err(Error::TooManyRegisterRules) => continue,
                Err(e) => return Err(e.context(CfiErrorKind::BadDebugInfo).into()),
            }
        }

        if let Some(first_row) = rows.first() {
            // Calculate the start address and total range covered by the CFI INIT record and its
            // subsequent CFI records. This information will be written into the CFI INIT record.
            let start = first_row.start_address();
            let length = rows.last().unwrap().end_address() - start;

            // Verify that the CFI entry is in range of the mapped module. Zero values are a special
            // case and seem to indicate that the entry is no longer valid. All other cases are
            // considered erroneous CFI.
            if start < info.load_address {
                return match start {
                    0 => Ok(()),
                    _ => Err(CfiErrorKind::InvalidAddress.into()),
                };
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
                    write!(line, "STACK CFI INIT {:x} {:x}", start_addr, length)
                        .context(CfiErrorKind::WriteError)?;
                } else {
                    let start_addr = row.start_address() - info.load_address;
                    write!(line, "STACK CFI {:x}", start_addr).context(CfiErrorKind::WriteError)?;
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
                for &(register, ref rule) in row.registers() {
                    if !rule_cache.get(&register).map_or(false, |c| c == &rule) {
                        rule_cache.insert(register, rule);
                        written |=
                            Self::write_register_rule(&mut line, info.arch, register, rule, ra)?;
                    }
                }

                if written {
                    self.inner
                        .write_all(&line)
                        .and_then(|_| writeln!(self.inner))
                        .context(CfiErrorKind::WriteError)?;
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
                match arch.register_name(register.0) {
                    Some(register) => format!("{} {} +", register, *offset),
                    None => return Ok(false),
                }
            }
            CfaRule::Expression(_) => return Ok(false),
        };

        write!(target, " .cfa: {}", formatted).context(CfiErrorKind::WriteError)?;
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
            RegisterRule::SameValue => match arch.register_name(register.0) {
                Some(reg) => reg.into(),
                None => return Ok(false),
            },
            RegisterRule::Offset(offset) => format!(".cfa {} + ^", offset),
            RegisterRule::ValOffset(offset) => format!(".cfa {} +", offset),
            RegisterRule::Register(register) => match arch.register_name(register.0) {
                Some(reg) => reg.into(),
                None => return Ok(false),
            },
            RegisterRule::Expression(_) => return Ok(false),
            RegisterRule::ValExpression(_) => return Ok(false),
            RegisterRule::Architectural => return Ok(false),
        };

        // Breakpad requires an explicit name for the return address register. In all other cases,
        // we use platform specific names for each register as specified by Breakpad.
        let register_name = if register == ra {
            ".ra"
        } else {
            match arch.register_name(register.0) {
                Some(reg) => reg,
                None => return Ok(false),
            }
        };

        write!(target, " {}: {}", register_name, formatted).context(CfiErrorKind::WriteError)?;
        Ok(true)
    }

    fn process_pdb(&mut self, pdb: &PdbObject<'_>) -> Result<(), CfiError> {
        let mut pdb = pdb.inner().write();
        let frame_table = pdb.frame_table().context(CfiErrorKind::BadDebugInfo)?;
        let address_map = pdb.address_map().context(CfiErrorKind::BadDebugInfo)?;

        // See `PdbDebugSession::build`.
        let string_table = match pdb.string_table() {
            Ok(string_table) => Some(string_table),
            Err(pdb::Error::StreamNameNotFound) => None,
            Err(e) => Err(e).context(CfiErrorKind::BadDebugInfo)?,
        };

        let mut frames = frame_table.iter();
        let mut last_frame: Option<FrameData> = None;

        while let Some(frame) = frames.next().context(CfiErrorKind::BadDebugInfo)? {
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
        let program_or_bp =
            frame.program.is_some() && string_table.is_some() || frame.uses_base_pointer;

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
            if program_or_bp { 1 } else { 0 },
        )
        .context(CfiErrorKind::WriteError)?;

        match frame.program {
            Some(ref prog_ref) => {
                let string_table = match string_table {
                    Some(string_table) => string_table,
                    None => return Ok(writeln!(self.inner).context(CfiErrorKind::WriteError)?),
                };

                let program_string = prog_ref
                    .to_string_lossy(&string_table)
                    .context(CfiErrorKind::BadDebugInfo)?;

                writeln!(self.inner, "{}", program_string.trim())
                    .context(CfiErrorKind::WriteError)?;
            }
            None => {
                writeln!(self.inner, "{}", if program_or_bp { 1 } else { 0 })
                    .context(CfiErrorKind::WriteError)?;
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

        for function_result in exception_data {
            let function = function_result.context(CfiErrorKind::BadDebugInfo)?;

            // Exception directories can contain zeroed out sections which need to be skipped.
            // Neither their start/end RVA nor the unwind info RVA is valid.
            if function == EMPTY_FUNCTION {
                continue;
            }

            // The minimal stack size is 8 for RIP
            let mut stack_size = 8;
            // Special handling for machine frames
            let mut machine_frame_offset = 0;

            if function.end_address < function.begin_address {
                continue;
            }

            let mut next_function = Some(function);
            while let Some(next) = next_function {
                let unwind_info = exception_data
                    .get_unwind_info(next, sections)
                    .context(CfiErrorKind::BadDebugInfo)?;

                for code_result in &unwind_info {
                    let code = code_result.context(CfiErrorKind::BadDebugInfo)?;
                    match code.operation {
                        UnwindOperation::PushNonVolatile(_) => {
                            stack_size += 8;
                        }
                        UnwindOperation::Alloc(size) => {
                            stack_size += size;
                        }
                        UnwindOperation::PushMachineFrame(is_error) => {
                            stack_size += if is_error { 48 } else { 40 };
                            machine_frame_offset = stack_size;
                        }
                        _ => {
                            // All other codes do not modify RSP
                        }
                    }
                }

                next_function = unwind_info.chained_info;
            }

            writeln!(
                self.inner,
                "STACK CFI INIT {:x} {:x} .cfa: $rsp 8 + .ra: .cfa 8 - ^",
                function.begin_address,
                function.end_address - function.begin_address,
            )
            .context(CfiErrorKind::WriteError)?;

            if machine_frame_offset > 0 {
                writeln!(
                    self.inner,
                    "STACK CFI {:x} .cfa: $rsp {} + $rsp: .cfa {} - ^ .ra: .cfa {} - ^",
                    function.begin_address,
                    stack_size,
                    stack_size - machine_frame_offset + 24, // old RSP offset
                    stack_size - machine_frame_offset + 48, // entire frame offset
                )
                .context(CfiErrorKind::WriteError)?
            } else {
                writeln!(
                    self.inner,
                    "STACK CFI {:x} .cfa: $rsp {} +",
                    function.begin_address, stack_size,
                )
                .context(CfiErrorKind::WriteError)?
            }
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
    V1(CfiCacheV1<'a>),
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
/// use symbolic_minidump::cfi::CfiCache;
///
/// # fn main() -> Result<(), failure::Error> {
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
/// use symbolic_minidump::cfi::CfiCache;
///
/// # fn main() -> Result<(), failure::Error> {
/// let view = ByteView::open("my.cficache")?;
/// let cache = CfiCache::from_bytes(view)?;
/// # Ok(())
/// # }
/// ```
///
pub struct CfiCache<'a> {
    inner: CfiCacheInner<'a>,
}

impl CfiCache<'static> {
    /// Construct a CFI cache from an `Object`.
    pub fn from_object(object: &Object<'_>) -> Result<Self, CfiError> {
        let buffer = AsciiCfiWriter::transform(object)?;
        let byteview = ByteView::from_vec(buffer);
        let inner = CfiCacheInner::V1(CfiCacheV1 { byteview });
        Ok(CfiCache { inner })
    }
}

impl<'a> CfiCache<'a> {
    /// Load a symcache from a `ByteView`.
    pub fn from_bytes(byteview: ByteView<'a>) -> Result<Self, CfiError> {
        if byteview.len() == 0 || byteview.starts_with(b"STACK") {
            let inner = CfiCacheInner::V1(CfiCacheV1 { byteview });
            return Ok(CfiCache { inner });
        }

        Err(CfiErrorKind::BadFileMagic.into())
    }

    /// Returns the cache file format version.
    pub fn version(&self) -> u32 {
        match self.inner {
            CfiCacheInner::V1(_) => 1,
        }
    }

    /// Returns whether this cache is up-to-date.
    pub fn is_latest(&self) -> bool {
        self.version() == CFICACHE_LATEST_VERSION
    }

    /// Returns the raw buffer of the cache file.
    pub fn as_slice(&self) -> &[u8] {
        match self.inner {
            CfiCacheInner::V1(ref v1) => v1.raw(),
        }
    }

    /// Writes the cache to the given writer.
    pub fn write_to<W: Write>(&self, mut writer: W) -> Result<(), io::Error> {
        io::copy(&mut self.as_slice(), &mut writer)?;
        Ok(())
    }
}
