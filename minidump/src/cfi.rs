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

use failure::{Fail, ResultExt};

use symbolic_common::{derive_failure, Arch, ByteView, UnknownArchError};
use symbolic_debuginfo::breakpad::{BreakpadObject, BreakpadStackRecord};
use symbolic_debuginfo::dwarf::gimli::{
    BaseAddresses, CfaRule, CieOrFde, DebugFrame, EhFrame, Error, FrameDescriptionEntry, Reader,
    ReaderOffset, Register, RegisterRule, UninitializedUnwindContext, UnwindOffset, UnwindSection,
    UnwindTable,
};
use symbolic_debuginfo::dwarf::Dwarf;
use symbolic_debuginfo::Object;

/// The latest version of the file format.
pub const CFICACHE_LATEST_VERSION: u32 = 1;

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
            Object::MachO(o) => self.process_dwarf(o.arch(), o),
            Object::Elf(o) => self.process_dwarf(o.arch(), o),
            _ => Err(CfiErrorKind::UnsupportedDebugFormat.into()),
        }
    }

    /// Returns the wrapped writer from this instance.
    pub fn into_inner(self) -> W {
        self.inner
    }

    fn process_breakpad(&mut self, object: &BreakpadObject<'_>) -> Result<(), CfiError> {
        for record in object.stack_records() {
            match record.context(CfiErrorKind::BadDebugInfo)? {
                BreakpadStackRecord::Cfi(r) => write!(self.inner, "STACK CFI {}\n", r.text),
                BreakpadStackRecord::Win(r) => write!(self.inner, "STACK WIN {}\n", r.text),
            }
            .context(CfiErrorKind::WriteError)?
        }

        Ok(())
    }

    fn process_dwarf<'o, O>(&mut self, arch: Arch, object: &O) -> Result<(), CfiError>
    where
        O: Dwarf<'o>,
    {
        let endian = object.endianity();

        if let Some((offset, data)) = object.section_data("eh_frame") {
            let mut frame = EhFrame::new(&data, endian);
            if let Some(pointer_size) = arch.pointer_size() {
                frame.set_address_size(pointer_size as u8);
            }
            self.read_cfi(arch, &frame, offset)
        } else if let Some((offset, data)) = object.section_data("debug_frame") {
            let mut frame = DebugFrame::new(&data, endian);
            if let Some(pointer_size) = arch.pointer_size() {
                frame.set_address_size(pointer_size as u8);
            }
            self.read_cfi(arch, &frame, offset)
        } else {
            Err(CfiErrorKind::MissingDebugInfo.into())
        }
    }

    fn read_cfi<U, R>(&mut self, arch: Arch, frame: &U, base: u64) -> Result<(), CfiError>
    where
        R: Reader + Eq,
        U: UnwindSection<R>,
    {
        // CFI information can have relative offsets to the base address of thir respective debug
        // section (either `.eh_frame` or `.debug_frame`). We need to supply this offset to the
        // entries iterator before starting to interpret instructions.
        let bases = BaseAddresses::default().set_eh_frame(base);

        let mut entries = frame.entries(&bases);
        while let Some(entry) = entries.next().context(CfiErrorKind::BadDebugInfo)? {
            // We skip all Common Information Entries and only process Frame Description Items here.
            // The iterator yields partial FDEs which need their associated CIE passed in via a
            // callback. This function is provided by the UnwindSection (frame), which then parses
            // the CIE and returns it for the FDE.
            if let CieOrFde::Fde(partial_fde) = entry {
                if let Ok(fde) = partial_fde.parse(|off| frame.cie_from_offset(&bases, off)) {
                    self.process_fde(arch, &fde)?
                }
            }
        }

        Ok(())
    }

    fn process_fde<S, R, O>(
        &mut self,
        arch: Arch,
        fde: &FrameDescriptionEntry<S, R, O>,
    ) -> Result<(), CfiError>
    where
        R: Reader<Offset = O> + Eq,
        O: ReaderOffset,
        S: UnwindSection<R>,
        S::Offset: UnwindOffset<R::Offset>,
    {
        // Retrieves the register that specifies the return address. We need to assign a special
        // format to this register for Breakpad.
        let ra = fde.cie().return_address_register();

        // Interpret all DWARF instructions of this Frame Description Entry. This gives us an unwind
        // table that contains rules for retrieving registers at every instruction address. These
        // rules can directly be transcribed to breakpad STACK CFI records.
        let ctx = UninitializedUnwindContext::new();
        let mut ctx = ctx
            .initialize(fde.cie())
            .map_err(|(e, _)| e)
            .context(CfiErrorKind::BadDebugInfo)?;
        let mut table = UnwindTable::new(&mut ctx, &fde);

        // Collect all rows first, as we need to know the final end address in order to write the
        // CFI INIT record describing the extent of the whole unwind table.
        let mut rows = Vec::new();
        loop {
            match table.next_row() {
                Ok(None) => break,
                Ok(Some(row)) => rows.push(row.clone()),
                Err(Error::UnknownCallFrameInstruction(_)) => {
                    continue;
                }
                Err(e) => {
                    return Err(e.context(CfiErrorKind::BadDebugInfo).into());
                }
            }
        }

        if let Some(first_row) = rows.first() {
            // Calculate the start address and total range covered by the CFI INIT record and its
            // subsequent CFI records. This information will be written into the CFI INIT record.
            let start = first_row.start_address();
            let length = rows.last().unwrap().end_address() - start;

            // Every register rule in the table will be cached so that it can be compared with
            // subsequent occurrences. Only registers with changed rules will be written.
            let mut rule_cache = HashMap::new();

            // Write records for every entry in the unwind table.
            for row in &rows {
                // Depending on whether this is the first row or any subsequent row, print a INIT or
                // normal STACK CFI record.
                if row.start_address() == start {
                    write!(self.inner, "STACK CFI INIT {:x} {:x}", start, length)
                        .context(CfiErrorKind::WriteError)?;
                } else {
                    write!(self.inner, "STACK CFI {:x}", row.start_address())
                        .context(CfiErrorKind::WriteError)?;
                }

                // Write the mandatory CFA rule for this row, followed by optional register rules.
                // The actual formatting of the rules depends on their rule type.
                self.write_cfa_rule(arch, row.cfa())?;

                // Print only registers that have changed rules to their previous occurrence to
                // reduce the number of rules per row. Then, cache the new occurrence for the next
                // row.
                for &(register, ref rule) in row.registers() {
                    if !rule_cache.get(&register).map_or(false, |c| c == &rule) {
                        rule_cache.insert(register, rule);
                        self.write_register_rule(arch, register, rule, ra)?;
                    }
                }

                writeln!(self.inner).context(CfiErrorKind::WriteError)?;
            }
        }

        Ok(())
    }

    fn write_cfa_rule<R: Reader>(
        &mut self,
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

        write!(self.inner, " .cfa: {}", formatted).context(CfiErrorKind::WriteError)?;
        Ok(true)
    }

    fn write_register_rule<R: Reader>(
        &mut self,
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

        write!(self.inner, " {}: {}", register_name, formatted)
            .context(CfiErrorKind::WriteError)?;
        Ok(true)
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
