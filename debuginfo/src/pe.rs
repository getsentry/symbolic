//! Support for Portable Executables, an extension of COFF used on Windows.

use std::borrow::Cow;
use std::fmt;
use std::io::Cursor;
use std::marker::PhantomData;

use failure::Fail;
use goblin::{error::Error as GoblinError, pe};

use symbolic_common::{Arch, AsSelf, CodeId, DebugId, Uuid};

use crate::base::*;
use crate::private::Parse;

pub use goblin::pe::exception::*;
pub use goblin::pe::section_table::SectionTable;

/// An error when dealing with [`PeObject`](struct.PeObject.html).
#[derive(Debug, Fail)]
pub enum PeError {
    /// The data in the PE file could not be parsed.
    #[fail(display = "invalid PE file")]
    BadObject(#[fail(cause)] GoblinError),
}

/// Portable Executable, an extension of COFF used on Windows.
///
/// This file format is used to carry program code. Debug information is usually moved to a separate
/// container, [`PdbObject`]. The PE file contains a reference to the PDB and vice versa to verify
/// that the files belong together.
///
/// While in rare instances, PE files might contain debug information, this case is not supported.
///
/// [`PdbObject`]: ../pdb/struct.PdbObject.html
pub struct PeObject<'d> {
    pe: pe::PE<'d>,
    data: &'d [u8],
}

impl<'d> PeObject<'d> {
    /// Tests whether the buffer could contain an PE object.
    pub fn test(data: &[u8]) -> bool {
        match goblin::peek(&mut Cursor::new(data)) {
            Ok(goblin::Hint::PE) => true,
            _ => false,
        }
    }

    /// Tries to parse a PE object from the given slice.
    pub fn parse(data: &'d [u8]) -> Result<Self, PeError> {
        pe::PE::parse(data)
            .map(|pe| PeObject { pe, data })
            .map_err(PeError::BadObject)
    }

    /// The container file format, which is always `FileFormat::Pe`.
    pub fn file_format(&self) -> FileFormat {
        FileFormat::Pe
    }

    /// The code identifier of this object.
    ///
    /// The code identifier consists of the `time_date_stamp` field id the COFF header, followed by
    /// the `size_of_image` field in the optional header. If the optional PE header is not present,
    /// this identifier is `None`.
    pub fn code_id(&self) -> Option<CodeId> {
        let header = &self.pe.header;
        let optional_header = header.optional_header.as_ref()?;

        let timestamp = header.coff_header.time_date_stamp;
        let size_of_image = optional_header.windows_fields.size_of_image;
        let string = format!("{:08x}{:x}", timestamp, size_of_image);
        Some(CodeId::new(string))
    }

    /// The debug information identifier of this PE.
    ///
    /// Since debug information is stored in an external [`PdbObject`], this identifier actually
    /// refers to the PDB. While strictly the filename of the PDB would also be necessary fully
    /// resolve it, in most instances the GUID and age contained in this identifier are sufficient.
    pub fn debug_id(&self) -> DebugId {
        self.pe
            .debug_data
            .as_ref()
            .and_then(|debug_data| debug_data.codeview_pdb70_debug_info.as_ref())
            .and_then(|debug_info| {
                // PE always stores the signature with little endian UUID fields.
                // Convert to network byte order (big endian) to match the
                // Breakpad processor's expectations.
                let mut data = debug_info.signature;
                data[0..4].reverse(); // uuid field 1
                data[4..6].reverse(); // uuid field 2
                data[6..8].reverse(); // uuid field 3

                let uuid = Uuid::from_slice(&data).ok()?;
                Some(DebugId::from_parts(uuid, debug_info.age))
            })
            .unwrap_or_default()
    }

    /// The name of the referenced PDB file.
    pub fn debug_file_name(&self) -> Option<Cow<'_, str>> {
        self.pe
            .debug_data
            .as_ref()
            .and_then(|debug_data| debug_data.codeview_pdb70_debug_info.as_ref())
            .map(|debug_info| {
                String::from_utf8_lossy(&debug_info.filename[..debug_info.filename.len() - 1])
            })
    }

    /// The CPU architecture of this object, as specified in the COFF header.
    pub fn arch(&self) -> Arch {
        let machine = self.pe.header.coff_header.machine;
        crate::pdb::arch_from_machine(machine.into())
    }

    /// The kind of this object, as specified in the PE header.
    pub fn kind(&self) -> ObjectKind {
        if self.pe.is_lib {
            ObjectKind::Library
        } else {
            ObjectKind::Executable
        }
    }

    /// The address at which the image prefers to be loaded into memory.
    ///
    /// ELF files store all internal addresses as if it was loaded at that address. When the image
    /// is actually loaded, that spot might already be taken by other images and so it must be
    /// relocated to a new address. During load time, the loader rewrites all addresses in the
    /// program code to match the new load address so that there is no runtime overhead when
    /// executing the code.
    ///
    /// Addresses used in `symbols` or `debug_session` have already been rebased relative to that
    /// load address, so that the caller only has to deal with addresses relative to the actual
    /// start of the image.
    pub fn load_address(&self) -> u64 {
        self.pe.image_base as u64
    }

    /// Determines whether this object exposes a public symbol table.
    pub fn has_symbols(&self) -> bool {
        !self.pe.exports.is_empty()
    }

    /// Returns an iterator over symbols in the public symbol table.
    pub fn symbols(&self) -> PeSymbolIterator<'d, '_> {
        PeSymbolIterator {
            exports: self.pe.exports.iter(),
        }
    }

    /// Returns an ordered map of symbols in the symbol table.
    pub fn symbol_map(&self) -> SymbolMap<'d> {
        self.symbols().collect()
    }

    /// Determines whether this object contains debug information.
    ///
    /// This is always `false`, as debug information is not supported for PE files.
    pub fn has_debug_info(&self) -> bool {
        false
    }

    /// Determines whether this object contains embedded source.
    pub fn has_source(&self) -> bool {
        false
    }

    /// Constructs a no-op debugging session.
    pub fn debug_session(&self) -> Result<PeDebugSession<'d>, PeError> {
        Ok(PeDebugSession { _ph: PhantomData })
    }

    /// Determines whether this object contains stack unwinding information.
    pub fn has_unwind_info(&self) -> bool {
        self.exception_data().map_or(false, |e| !e.is_empty())
    }

    /// Returns the raw data of the PE file.
    pub fn data(&self) -> &'d [u8] {
        self.data
    }

    /// A list of the sections in this PE binary, used to resolve virtual addresses.
    pub fn sections(&self) -> &[SectionTable] {
        &self.pe.sections
    }

    /// Returns exception data containing unwind information.
    pub fn exception_data(&self) -> Option<&ExceptionData> {
        self.pe.exception_data.as_ref()
    }
}

impl fmt::Debug for PeObject<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("PeObject")
            .field("code_id", &self.code_id())
            .field("debug_id", &self.debug_id())
            .field("debug_file_name", &self.debug_file_name())
            .field("arch", &self.arch())
            .field("kind", &self.kind())
            .field("load_address", &format_args!("{:#x}", self.load_address()))
            .field("has_symbols", &self.has_symbols())
            .field("has_debug_info", &self.has_debug_info())
            .field("has_unwind_info", &self.has_unwind_info())
            .finish()
    }
}

impl<'slf, 'd: 'slf> AsSelf<'slf> for PeObject<'d> {
    type Ref = PeObject<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
    }
}

impl<'d> Parse<'d> for PeObject<'d> {
    type Error = PeError;

    fn test(data: &[u8]) -> bool {
        Self::test(data)
    }

    fn parse(data: &'d [u8]) -> Result<Self, PeError> {
        Self::parse(data)
    }
}

impl<'d> ObjectLike for PeObject<'d> {
    type Error = PeError;
    type Session = PeDebugSession<'d>;

    fn file_format(&self) -> FileFormat {
        self.file_format()
    }

    fn code_id(&self) -> Option<CodeId> {
        self.code_id()
    }

    fn debug_id(&self) -> DebugId {
        self.debug_id()
    }

    fn debug_file_name(&self) -> Option<Cow<'_, str>> {
        self.debug_file_name()
    }

    fn arch(&self) -> Arch {
        self.arch()
    }

    fn kind(&self) -> ObjectKind {
        self.kind()
    }

    fn load_address(&self) -> u64 {
        self.load_address()
    }

    fn has_symbols(&self) -> bool {
        self.has_symbols()
    }

    fn symbols(&self) -> DynIterator<'_, Symbol<'_>> {
        Box::new(self.symbols())
    }

    fn symbol_map(&self) -> SymbolMap<'_> {
        self.symbol_map()
    }

    fn has_debug_info(&self) -> bool {
        self.has_debug_info()
    }

    fn debug_session(&self) -> Result<Self::Session, Self::Error> {
        self.debug_session()
    }

    fn has_unwind_info(&self) -> bool {
        self.has_unwind_info()
    }

    fn has_source(&self) -> bool {
        self.has_source()
    }
}

/// An iterator over symbols in the PE file.
///
/// Returned by [`PeObject::symbols`](struct.PeObject.html#method.symbols).
pub struct PeSymbolIterator<'d, 'o> {
    exports: std::slice::Iter<'o, pe::export::Export<'d>>,
}

impl<'d, 'o> Iterator for PeSymbolIterator<'d, 'o> {
    type Item = Symbol<'d>;

    fn next(&mut self) -> Option<Self::Item> {
        self.exports.next().map(|export| Symbol {
            name: export.name.map(Cow::Borrowed),
            address: export.rva as u64,
            size: export.size as u64,
        })
    }
}

/// Debug session for PE objects.
///
/// Since debug information in PE containers is not supported, this session consists of NoOps and
/// always returns empty results.
#[derive(Debug)]
pub struct PeDebugSession<'d> {
    _ph: PhantomData<&'d ()>,
}

impl<'d> PeDebugSession<'d> {
    /// Returns an iterator over all functions in this debug file.
    pub fn functions(&self) -> PeFunctionIterator<'_> {
        std::iter::empty()
    }

    /// Looks up a file's source contents by its full canonicalized path.
    ///
    /// The given path must be canonicalized.
    pub fn source_by_path(&self, _path: &str) -> Option<String> {
        None
    }
}

impl DebugSession for PeDebugSession<'_> {
    type Error = PeError;

    fn functions(&self) -> DynIterator<'_, Result<Function<'_>, Self::Error>> {
        Box::new(std::iter::empty())
    }

    fn source_by_path(&self, path: &str) -> Option<String> {
        self.source_by_path(path)
    }
}

/// An iterator over functions in a PE file.
pub type PeFunctionIterator<'s> = std::iter::Empty<Result<Function<'s>, PeError>>;
