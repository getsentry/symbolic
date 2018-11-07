use std::collections::HashMap;
use std::fmt;
use std::io::{self, Write};

use failure::{Backtrace, Context, Fail, ResultExt};
use gimli::{
    self, BaseAddresses, CfaRule, CieOrFde, DebugFrame, EhFrame, FrameDescriptionEntry, Reader,
    ReaderOffset, RegisterRule, UninitializedUnwindContext, UnwindOffset, UnwindSection,
    UnwindTable,
};

use symbolic_common::byteview::ByteView;
use symbolic_common::types::{Arch, DebugKind, UnknownArchError};
use symbolic_debuginfo::{DwarfData, DwarfSection, Object};

use crate::registers::get_register_name;

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

/// An error returned by `AsciiCfiWriter`.
///
/// This error contains a context with stack traces and an error cause.
#[derive(Debug)]
pub struct CfiError {
    inner: Context<CfiErrorKind>,
}

impl Fail for CfiError {
    fn cause(&self) -> Option<&Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl fmt::Display for CfiError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

impl CfiError {
    pub fn kind(&self) -> CfiErrorKind {
        *self.inner.get_context()
    }
}

impl From<CfiErrorKind> for CfiError {
    fn from(kind: CfiErrorKind) -> CfiError {
        CfiError {
            inner: Context::new(kind),
        }
    }
}

impl From<Context<CfiErrorKind>> for CfiError {
    fn from(inner: Context<CfiErrorKind>) -> CfiError {
        CfiError { inner }
    }
}

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
/// # extern crate symbolic_common;
/// # extern crate symbolic_debuginfo;
/// # extern crate symbolic_minidump;
/// # extern crate failure;
/// use symbolic_common::byteview::ByteView;
/// use symbolic_debuginfo::FatObject;
/// use symbolic_minidump::cfi::AsciiCfiWriter;
///
/// # fn foo() -> Result<(), failure::Error> {
/// let byteview = ByteView::from_path("/path/to/object")?;
/// let fat = FatObject::parse(byteview)?;
/// let object = fat.get_object(0)?.unwrap();
///
/// let mut writer = Vec::new();
/// AsciiCfiWriter::new(&mut writer).process(&object)?;
/// # Ok(())
/// # }
///
/// # fn main() { foo().unwrap() }
/// ```
///
/// For writers that implement `Default`, there is a convenience method that creates an instance and
/// returns it right away:
///
/// ```rust,no_run
/// # extern crate symbolic_common;
/// # extern crate symbolic_debuginfo;
/// # extern crate symbolic_minidump;
/// # extern crate failure;
/// use symbolic_common::byteview::ByteView;
/// use symbolic_debuginfo::FatObject;
/// use symbolic_minidump::cfi::AsciiCfiWriter;
///
/// # fn foo() -> Result<(), failure::Error> {
/// let byteview = ByteView::from_path("/path/to/object")?;
/// let fat = FatObject::parse(byteview)?;
/// let object = fat.get_object(0)?.unwrap();
///
/// let buffer: Vec<u8> = AsciiCfiWriter::transform(&object)?;
/// # Ok(())
/// # }
/// # fn main() { foo().unwrap() }
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
    pub fn process(&mut self, object: &Object) -> Result<(), CfiError> {
        match object.debug_kind() {
            Some(DebugKind::Dwarf) => self.process_dwarf(object),
            Some(DebugKind::Breakpad) => self.process_breakpad(object),

            // clang on darwin moves CFI to the "__eh_frame" section in the
            // exectuable rather than the dSYM. This allows processes to unwind
            // exceptions during runtime. However, we do not detect these files
            // as `DebugKind::Dwarf` to avoid false positives in all other
            // cases. Therefore, simply try find the __eh_frame section and
            // otherwise fail. The file is already loaded and relevant pages
            // are already in the cache, so the overhead is justifiable.
            _ => self
                .process_dwarf(object)
                .map_err(|_| CfiErrorKind::UnsupportedDebugFormat.into()),
        }
    }

    fn process_breakpad(&mut self, object: &Object) -> Result<(), CfiError> {
        for line in object.as_bytes().split(|b| *b == b'\n') {
            if line.starts_with(b"STACK") {
                self.inner
                    .write_all(line)
                    .context(CfiErrorKind::WriteError)?;
                self.inner.write(b"\n").context(CfiErrorKind::WriteError)?;
            }
        }

        Ok(())
    }

    fn process_dwarf(&mut self, object: &Object) -> Result<(), CfiError> {
        let endianness = object.endianness();

        if let Some(section) = object.get_dwarf_section(DwarfSection::EhFrame) {
            let frame = EhFrame::new(section.as_bytes(), endianness);
            let arch = object.arch().map_err(|_| CfiErrorKind::UnsupportedArch)?;
            self.read_cfi(arch, &frame, section.offset())
        } else if let Some(section) = object.get_dwarf_section(DwarfSection::DebugFrame) {
            let frame = DebugFrame::new(section.as_bytes(), endianness);
            let arch = object.arch().map_err(|_| CfiErrorKind::UnsupportedArch)?;
            self.read_cfi(arch, &frame, section.offset())
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
        let bases = BaseAddresses::default().set_cfi(base);

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
                Err(gimli::Error::UnknownCallFrameInstruction(_)) => continue,
                Err(e) => return Err(e.context(CfiErrorKind::BadDebugInfo).into()),
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
        use gimli::CfaRule::*;
        let formatted = match rule {
            RegisterAndOffset { register, offset } => {
                format!("{} {} +", get_register_name(arch, *register)?, *offset)
            }
            Expression(_) => return Ok(false),
        };

        write!(self.inner, " .cfa: {}", formatted).context(CfiErrorKind::WriteError)?;
        Ok(true)
    }

    fn write_register_rule<R: Reader>(
        &mut self,
        arch: Arch,
        register: u8,
        rule: &RegisterRule<R>,
        ra: u64,
    ) -> Result<bool, CfiError> {
        use gimli::RegisterRule::*;
        let formatted = match rule {
            Undefined => return Ok(false),
            SameValue => get_register_name(arch, register)?.into(),
            Offset(offset) => format!(".cfa {} + ^", offset),
            ValOffset(offset) => format!(".cfa {} +", offset),
            Register(register) => get_register_name(arch, *register)?.into(),
            Expression(_) => return Ok(false),
            ValExpression(_) => return Ok(false),
            Architectural => return Ok(false),
        };

        // Breakpad requires an explicit name for the return address register. In all other cases,
        // we use platform specific names for each register as specified by Breakpad.
        let register_name = if u64::from(register) == ra {
            ".ra"
        } else {
            get_register_name(arch, register)?
        };

        write!(self.inner, " {}: {}", register_name, formatted)
            .context(CfiErrorKind::WriteError)?;
        Ok(true)
    }
}

impl<W: Write + Default> AsciiCfiWriter<W> {
    /// Extracts CFI from the given object and pipes it to a new writer instance.
    pub fn transform(object: &Object) -> Result<W, CfiError> {
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
/// # extern crate symbolic_common;
/// # extern crate symbolic_debuginfo;
/// # extern crate symbolic_minidump;
/// # extern crate failure;
/// use std::fs::File;
/// use symbolic_common::byteview::ByteView;
/// use symbolic_debuginfo::FatObject;
/// use symbolic_minidump::cfi::CfiCache;
///
/// # fn foo() -> Result<(), failure::Error> {
/// let byteview = ByteView::from_path("/path/to/object")?;
/// let fat = FatObject::parse(byteview)?;
/// if let Some(object) = fat.get_object(0)? {
///     let cache = CfiCache::from_object(&object)?;
///     cache.write_to(File::create("my.cficache")?)?;
/// }
/// # Ok(())
/// # }
///
/// # fn main() { foo().unwrap() }
/// ```
///
/// ```rust,no_run
/// # extern crate symbolic_common;
/// # extern crate symbolic_debuginfo;
/// # extern crate symbolic_minidump;
/// # extern crate failure;
/// use symbolic_common::byteview::ByteView;
/// use symbolic_debuginfo::FatObject;
/// use symbolic_minidump::cfi::CfiCache;
///
/// # fn foo() -> Result<(), failure::Error> {
/// let byteview = ByteView::from_path("my.cficache")?;
/// let cache = CfiCache::from_bytes(byteview)?;
/// # Ok(())
/// # }
///
/// # fn main() { foo().unwrap() }
/// ```
///
pub struct CfiCache<'a> {
    inner: CfiCacheInner<'a>,
}

impl CfiCache<'static> {
    /// Construct a CFI cache from an `Object`.
    pub fn from_object(object: &Object) -> Result<Self, CfiError> {
        let buffer = AsciiCfiWriter::transform(object)?;
        let byteview = ByteView::from_vec(buffer);
        let inner = CfiCacheInner::V1(CfiCacheV1 { byteview });
        Ok(CfiCache { inner })
    }
}

impl<'a> CfiCache<'a> {
    /// Load a symcache from a `ByteView`.
    pub fn from_bytes(byteview: ByteView<'a>) -> Result<Self, CfiError> {
        if byteview.starts_with(b"STACK") {
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
