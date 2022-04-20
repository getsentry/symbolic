//! Parser for the Usym format.
//!
//! This format can map il2cpp instruction addresses to managed file names and line numbers.

use std::borrow::Cow;
use std::error::Error;
use std::str::FromStr;
use std::{fmt, mem, ptr};

use symbolic_common::{Arch, DebugId};
use symbolic_debuginfo::FileInfo;
use thiserror::Error;

/// The error type for [`UsymError`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum UsymErrorKind {
    /// Buffer to usym file is misaligned.
    MisalignedBuffer,
    /// The header to the usym file is missing or undersized.
    BadHeader,
    /// The magic string in the header is missing or malformed.
    BadMagic,
    /// The version string in the usym file's header is missing or malformed.
    BadVersion,
    /// The record count in the header can't be read.
    BadRecordCount,
    /// The size of the usym file is smaller than the amount of data it is supposed to hold
    /// according to its header.
    BufferSmallerThanAdvertised,
    /// The strings section is missing.
    MissingStrings,
    /// A valid slice to the usym's source records could not be created.
    BadRecords,
    /// The assembly ID is missing or can't be read.
    BadId,
    /// The assembly name is missing or can't be read.
    BadName,
    /// The architecture is missing or can't be read.
    BadOperatingSystem,
    /// The architecture is missing or can't be read.
    BadArchitecture,
    /// A part of the file is not encoded in valid UTF-8.
    BadEncoding,
}

impl fmt::Display for UsymErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UsymErrorKind::MisalignedBuffer => write!(f, "misaligned pointer to buffer"),
            UsymErrorKind::BadHeader => write!(f, "missing or undersized header"),
            UsymErrorKind::BadMagic => write!(f, "missing or wrong usym magic bytes"),
            UsymErrorKind::BadVersion => write!(f, "missing or wrong version number"),
            UsymErrorKind::BadRecordCount => write!(f, "unreadable record count"),
            UsymErrorKind::BufferSmallerThanAdvertised => {
                write!(f, "buffer does not contain all data header claims it has")
            }
            UsymErrorKind::MissingStrings => write!(f, "strings section is missing"),
            UsymErrorKind::BadRecords => write!(f, "could not construct list of source records"),
            UsymErrorKind::BadId => write!(f, "assembly ID is missing or unreadable"),
            UsymErrorKind::BadName => write!(f, "assembly name is missing or unreadable"),
            UsymErrorKind::BadOperatingSystem => {
                write!(f, "operating system is missing or unreadable")
            }
            UsymErrorKind::BadArchitecture => write!(f, "architecture is missing or unreadable"),
            UsymErrorKind::BadEncoding => {
                write!(f, "part of the file is not encoded in valid UTF-8")
            }
        }
    }
}

/// An error when dealing with [`UsymSymbols`].
#[derive(Debug, Error)]
#[error("{kind}")]
pub struct UsymError {
    kind: UsymErrorKind,
    #[source]
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl UsymError {
    /// Creates a new [`UsymError`] from a [`UsymErrorKind`] and an arbitrary source error payload.
    fn new<E>(kind: UsymErrorKind, source: E) -> Self
    where
        E: Into<Box<dyn Error + Send + Sync>>,
    {
        let source = Some(source.into());
        Self { kind, source }
    }

    /// Returns the corresponding [`UsymErrorKind`] for this error.
    pub fn kind(&self) -> UsymErrorKind {
        self.kind
    }
}

impl From<UsymErrorKind> for UsymError {
    fn from(kind: UsymErrorKind) -> Self {
        Self { kind, source: None }
    }
}

// TODO: consider introducing newtype for strings section offsets and the strings section itself

/// The raw C structures.
mod raw {

    /// The header of the usym file format.
    #[derive(Debug, Clone)]
    #[repr(C)]
    pub(super) struct Header {
        /// Magic number identifying the file, `b"usym"`.
        pub(super) magic: u32,

        /// Version of the usym file format.
        pub(super) version: u32,

        /// Number of [`UsymRecord`] entries.
        ///
        /// These follow right after the header, and after them is the strings section.
        pub(super) record_count: u32,

        /// UUID of the assembly, as an offset into the strings section.
        pub(super) id: u32,

        /// Name of the "assembly", as an offset into the strings section.
        pub(super) name: u32,

        /// Name of OS, as an offset into the strings section.
        pub(super) os: u32,

        /// Name of architecture, as an offset into the strings section.
        pub(super) arch: u32,
    }

    /// A record mapping an IL2CPP instruction address to a managed code location.
    ///
    /// This is the raw record as it appears in the file, see [`UsymRecord`] for a record with
    /// the names resolved.
    #[derive(Debug, Clone, Copy)]
    #[repr(C, packed)]
    pub(super) struct SourceRecord {
        /// Instruction pointer address, relative to base address of assembly.
        pub(super) address: u64,
        /// Native symbol name, as an offset into the strings section.
        pub(super) native_symbol: u32,
        /// Native source file, as an offset into the strings section.
        pub(super) native_file: u32,
        /// Native line number.
        pub(super) native_line: u32,
        /// Managed code symbol name, as an offset into the strings section.
        ///
        /// Most of the time, this is 0 if the record does not map to managed code. We haven't seen
        /// this happen yet, but it's possible that a nonzero offset may lead to an empty string,
        /// meaning that there is no managed symbol for this record.
        pub(super) managed_symbol: u32,
        /// Managed code file name, as an offset into the strings section.
        ///
        /// Most of the time, this is 0 if code does not map to managed code. We haven't seen this
        /// happen yet, but it's possible that a nonzero offset may lead to an empty string,
        /// meaning that there is no managed file for this record.
        pub(super) managed_file: u32,
        /// Managed code line number. This is 0 if the record does not map to any managed code.
        pub(super) managed_line: u32,
        /// Unknown field. Normally set to FFFFFFFF, but investigations suggest that if this record
        /// is for an inlinee function, this is set to the index of its parent record.
        pub(super) maybe_parent_record_idx: u32,
    }
}

#[derive(Clone, Debug)]
pub struct UnmappedRecord<'a> {
    /// Instruction pointer address, relative to the base of the assembly.
    pub address: u64,
    /// Symbol name of the native code.
    pub native_symbol: Cow<'a, str>,
    /// File name and path of the native code.
    pub native_file: Cow<'a, str>,
    /// Line number of the native code.
    pub native_line: u32,
}

#[derive(Clone, Debug)]
pub struct MappedRecord<'a> {
    /// Instruction pointer address, relative to the base of the assembly.
    pub address: u64,
    /// Symbol name of the native code.
    pub native_symbol: Cow<'a, str>,
    /// File name and path of the native code.
    pub native_file: Cow<'a, str>,
    /// Line number of the native code.
    pub native_line: u32,
    /// Symbol name of the managed code.
    pub managed_symbol: Cow<'a, str>,
    /// File name of the managed code.
    pub managed_file_info: FileInfo<'a>,
    /// Line number of the managed code.
    pub managed_line: u32,
}

/// A record mapping an IL2CPP instruction address to managed code location.
///
/// Records may exist that do not map directly to any managed code. There are two known cases of
/// this:
/// 1. The record describes native-only code, such as code for the Unity engine.
/// 2. The record describes native-only code that runs under the hood for managed code, such as
///    code that registers functions/methods to the runtime.
#[derive(Clone, Debug)]
pub enum UsymSourceRecord<'a> {
    /// An unmapped record. This could be code for the Unity engine, or under-the-hood native code
    /// that doesn't directly map to any specific managed line.
    Unmapped(UnmappedRecord<'a>),
    /// A mapped record. This directly maps IL2CPP-compiled native code to managed code.
    Mapped(MappedRecord<'a>),
}

/// A usym file containing data on how to map native code generated by Unity's IL2CPP back to their
/// C# (i.e. managed) equivalents.
pub struct UsymSymbols<'a> {
    /// File header.
    header: &'a raw::Header,
    /// Instruction address to managed code mapping records.
    records: &'a [raw::SourceRecord],
    /// All the strings.
    ///
    /// This is not a traditional strings table, but rather a large slice of bytes with
    /// length-prefixed strings where the length is a little-endian u16.  The header and records
    /// refer to strings by byte offsets into this slice of bytes, which must fall on the
    /// the length prefixed part of the string.
    strings: &'a [u8],
    /// The ID of the assembly.
    id: &'a str,
    /// The name of the assembly.
    name: &'a str,
    /// The operating system.
    os: &'a str,
    /// The architecture.
    arch: &'a str,
}

impl<'a> UsymSymbols<'a> {
    const MAGIC: &'static [u8] = b"usym";

    /// Parse a usym file.
    ///
    /// # Panics
    ///
    /// If `std::mem::size_of::<usize>()` is smaller than `std::mem::size_of::<u32>()` on
    /// the machine being run on.
    pub fn parse(buf: &'a [u8]) -> Result<UsymSymbols<'a>, UsymError> {
        if buf.as_ptr().align_offset(8) != 0 {
            return Err(UsymErrorKind::MisalignedBuffer.into());
        }
        if buf.len() < mem::size_of::<raw::Header>() {
            return Err(UsymErrorKind::BadHeader.into());
        }
        if buf.get(..Self::MAGIC.len()) != Some(Self::MAGIC) {
            return Err(UsymErrorKind::BadMagic.into());
        }

        // SAFETY: We checked the buffer is large enough above.
        let header = unsafe { &*(buf.as_ptr() as *const raw::Header) };
        if header.version != 2 {
            return Err(UsymErrorKind::BadVersion.into());
        }

        let record_count: usize = header
            .record_count
            .try_into()
            .map_err(|e| UsymError::new(UsymErrorKind::BadRecordCount, e))?;
        // TODO: consider trying to just grab the records and give up on their strings if something
        // is wrong with the strings section
        let strings_offset =
            mem::size_of::<raw::Header>() + record_count * mem::size_of::<raw::SourceRecord>();
        if buf.len() < strings_offset {
            return Err(UsymErrorKind::BufferSmallerThanAdvertised.into());
        }

        // SAFETY: We checked the buffer is at least the size_of::<UsymHeader>() above.
        let first_record_ptr = unsafe { buf.as_ptr().add(mem::size_of::<raw::Header>()) };

        // SAFETY: We checked the buffer has enough space for all the source records above.
        let records = unsafe {
            let first_record_ptr: *const raw::SourceRecord = first_record_ptr.cast();
            let records_ptr = ptr::slice_from_raw_parts(first_record_ptr, record_count);
            records_ptr
                .as_ref()
                .ok_or_else(|| UsymError::from(UsymErrorKind::BadRecords))
        }?;

        let strings = buf
            .get(strings_offset..)
            .ok_or_else(|| UsymError::from(UsymErrorKind::MissingStrings))?;

        let id_offset = header.id.try_into().unwrap();
        let id = match Self::get_string_from_offset(strings, id_offset)
            .ok_or_else(|| UsymError::from(UsymErrorKind::BadId))?
        {
            Cow::Borrowed(id) => id,
            Cow::Owned(_) => return Err(UsymErrorKind::BadEncoding.into()),
        };
        let name_offset = header.name.try_into().unwrap();
        let name = match Self::get_string_from_offset(strings, name_offset)
            .ok_or_else(|| UsymError::from(UsymErrorKind::BadName))?
        {
            Cow::Borrowed(name) => name,
            Cow::Owned(_) => return Err(UsymErrorKind::BadEncoding.into()),
        };

        let os_offset = header.os.try_into().unwrap();
        let os = match Self::get_string_from_offset(strings, os_offset)
            .ok_or_else(|| UsymError::from(UsymErrorKind::BadOperatingSystem))?
        {
            Cow::Borrowed(name) => name,
            Cow::Owned(_) => return Err(UsymErrorKind::BadEncoding.into()),
        };

        let arch_offset = header.arch.try_into().unwrap();
        let arch = match Self::get_string_from_offset(strings, arch_offset)
            .ok_or_else(|| UsymError::from(UsymErrorKind::BadArchitecture))?
        {
            Cow::Borrowed(name) => name,
            Cow::Owned(_) => return Err(UsymErrorKind::BadEncoding.into()),
        };

        // accumulate and store all of the errors that don't completely block parsing
        // - bad encoding
        // - missing sys info fields
        Ok(Self {
            header,
            records,
            strings,
            id,
            name,
            os,
            arch,
        })
    }

    /// Returns the version of the usym file these symbols were read from.
    pub fn version(&self) -> u32 {
        self.header.version
    }

    /// Returns a string from the strings section at the given offset.
    ///
    /// Offsets can be found in [`raw::Header`], [`raw::SourceRecord`], and [`UsymSymbols`] fields.
    fn get_string_bytes_from_offset(data: &'a [u8], offset: usize) -> Option<&'a [u8]> {
        let size_bytes = data.get(offset..offset + 2)?;
        let size: usize = u16::from_le_bytes([size_bytes[0], size_bytes[1]]).into();

        let start_offset = offset + 2;
        let end_offset = start_offset + size;

        data.get(start_offset..end_offset)
    }

    /// Returns the bytes for a string from the strings section at the given offset.
    fn get_string_bytes(&'a self, offset: usize) -> Option<&'a [u8]> {
        Self::get_string_bytes_from_offset(self.strings, offset)
    }

    fn get_string_from_offset(data: &'a [u8], offset: usize) -> Option<Cow<str>> {
        let string_bytes = Self::get_string_bytes_from_offset(data, offset)?;
        Some(String::from_utf8_lossy(string_bytes))
    }

    /// Returns a string from the strings section at the given offset.
    ///
    /// Offsets can be found in [`raw::Header`], [`raw::SourceRecord`], and [`UsymSymbols`] fields.
    fn get_string(&'a self, offset: usize) -> Option<Cow<'a, str>> {
        Self::get_string_from_offset(self.strings, offset)
    }

    /// The ID of the assembly.
    ///
    /// This should match the ID of the debug symbols.
    pub fn id(&self) -> Result<DebugId, UsymError> {
        DebugId::from_str(self.id).map_err(|e| UsymError::new(UsymErrorKind::BadId, e))
    }

    /// The name of the assembly.
    pub fn name(&self) -> &str {
        self.name
    }

    /// The Operating System name.
    pub fn os(&self) -> &str {
        self.os
    }

    /// The architecture.
    pub fn arch(&self) -> Result<Arch, UsymError> {
        Arch::from_str(self.arch).map_err(|e| UsymError::new(UsymErrorKind::BadArchitecture, e))
    }

    /// Returns a [`UsymSourceRecord`] at the given index.
    pub fn get_record(&self, index: usize) -> Option<UsymSourceRecord> {
        let raw = self.records.get(index)?;
        self.resolve_record(raw)
    }

    /// Fills in a [`raw::SourceRecord`] with its referenced strings taken from the strings section.
    ///
    /// Assumes that all of the string fields are in valid UTF-8.
    fn resolve_record(&self, raw: &raw::SourceRecord) -> Option<UsymSourceRecord> {
        // TODO: add some resilience to this so if we some strings can't be fetched from the strings
        // section, just return none, an empty string, or a placeholder.
        let native_symbol = self.get_string(raw.native_symbol.try_into().unwrap())?;
        let native_file = self.get_string(raw.native_file.try_into().unwrap())?;

        let msymbol_offset = raw.managed_symbol.try_into().unwrap();
        let managed_symbol = self.get_string(msymbol_offset)?;
        let managed_symbol = if managed_symbol.is_empty() {
            None
        } else {
            Some(managed_symbol)
        };
        // TODO: Log these as a warning
        // if managed_symbol.is_none() && raw.managed_symbol > 0 {
        //     println!("A managed symbol with a >0 offset into the string table points to an empty string. We normally expect empty strings to have an offset of 0.");
        //     println!("Native entry: {}::{}", native_file, native_symbol);
        // }
        let mfilename_offset = raw.managed_file.try_into().unwrap();
        let managed_file = self.get_string(mfilename_offset)?;
        let managed_file_info = match managed_file.is_empty() {
            true => None,
            false => {
                let file_bytes = self.get_string_bytes(mfilename_offset)?;
                // implementation blatantly stolen from FileInfo::from_path because it's only visible to
                // the debuginfo crate
                let (dir, name) = symbolic_common::split_path_bytes(file_bytes);
                Some(FileInfo {
                    name,
                    dir: dir.unwrap_or_default(),
                })
            }
        };
        // TODO: Log these as a warning
        // if managed_file.is_empty() && raw.managed_file > 0 {
        //     println!("A managed file name with a >0 offset into the string table points to an empty string. We normally expect empty strings to have an offset of 0.");
        //     println!("Native entry: {}::{}", native_file, native_symbol);
        // }
        let managed_line = match raw.managed_line {
            0 => None,
            n => Some(n),
        };

        match (managed_symbol, managed_file_info, managed_line) {
            (Some(managed_symbol), Some(managed_file_info), Some(managed_line)) => {
                Some(UsymSourceRecord::Mapped(MappedRecord {
                    address: raw.address,
                    native_symbol,
                    native_file,
                    native_line: raw.native_line,
                    managed_symbol,
                    managed_file_info,
                    managed_line,
                }))
            }
            _ => Some(UsymSourceRecord::Unmapped(UnmappedRecord {
                address: raw.address,
                native_symbol,
                native_file,
                native_line: raw.native_line,
            })),
        }
    }

    /// Lookup the managed code source location for an IL2CPP instruction pointer.
    pub fn lookup_source_record(&self, ip: u64) -> Option<UsymSourceRecord> {
        // TODO: need to subtract the image base to get relative address
        match self.records.binary_search_by_key(&ip, |r| r.address) {
            Ok(index) => self.get_record(index),
            Err(index) => self.get_record(index - 1),
        }
    }

    // TODO: Fixup the return type, maybe use the strategy employed by
    // BreakpadDebugSession/DwarfDebugSession/PdbDebugSession/etc's methods
    pub fn records(&'a self) -> impl Iterator<Item = UsymSourceRecord<'a>> {
        self.records.iter().filter_map(|r| self.resolve_record(r))
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;

    use symbolic_common::ByteView;
    use symbolic_testutils::fixture;

    use super::*;

    #[test]
    fn test_write_usym() {
        // Not really a test but rather a quick and dirty way to write a small usym file
        // given a large one.  This was used to generate a small enough usym file to use as
        // a test fixture, however this still tests the reader and writer can round-trip.

        // let file = File::open(
        //     "/Users/flub/code/sentry-unity-il2cpp-line-numbers/Builds/iOS/UnityFramework.usym",
        // )
        // .unwrap();
        let file = File::open(fixture("il2cpp/managed.usym")).unwrap();

        let orig_data = ByteView::map_file_ref(&file).unwrap();
        let usyms = UsymSymbols::parse(&orig_data).unwrap();

        // Our strings and helper to build it by pushing new strings.  We keep strings and
        // strings_offsets so we can de-duplicate, raw_strings is the thing we are really
        // building.
        let mut strings: Vec<String> = Vec::new();
        let mut raw_strings: Vec<u8> = Vec::new();
        let mut string_offsets: Vec<u64> = Vec::new();

        let mut push_string = |s: Cow<'_, str>| match strings.iter().position(|i| i == s.as_ref()) {
            Some(pos) => string_offsets[pos],
            None => {
                let offset = raw_strings.len() as u64;
                let len = s.len() as u16;
                raw_strings.extend_from_slice(&len.to_le_bytes());
                raw_strings.extend_from_slice(s.as_bytes());

                strings.push(s.to_string());
                string_offsets.push(offset);

                offset
            }
        };

        // The string table always starts with an entry for the empty string.
        push_string(Cow::Borrowed(""));

        // Construct new header.
        let mut header = usyms.header.clone();
        header.id = push_string(usyms.get_string(header.id as usize).unwrap()) as u32;
        header.name = push_string(usyms.get_string(header.name as usize).unwrap()) as u32;
        header.os = push_string(usyms.get_string(header.os as usize).unwrap()) as u32;
        header.arch = push_string(usyms.get_string(header.arch as usize).unwrap()) as u32;

        // Construct new records. Skims the top 5 records, then grabs the 3 records that have
        // mappings to managed symbols.
        header.record_count = 5 + 3;
        let first_five = usyms.records.iter().take(5);
        let actual_mappings = usyms.records.iter().filter(|r| r.managed_symbol != 0);
        let mut records = Vec::new();
        for mut record in first_five.chain(actual_mappings).cloned() {
            if record.native_symbol > 0 {
                record.native_symbol =
                    push_string(usyms.get_string(record.native_symbol as usize).unwrap()) as u32;
            }
            if record.native_file > 0 {
                record.native_file =
                    push_string(usyms.get_string(record.native_file as usize).unwrap()) as u32;
            }
            if record.managed_symbol > 0 {
                record.managed_symbol =
                    push_string(usyms.get_string(record.managed_symbol as usize).unwrap()) as u32;
            }
            if record.managed_file > 0 {
                record.managed_file =
                    push_string(usyms.get_string(record.managed_file as usize).unwrap()) as u32;
            }
            records.push(record);
        }

        // let mut dest = File::create(fixture("il2cpp/artificial.usym")).unwrap();
        let mut dest = Vec::new();

        // Write the header.
        let data = &[header];
        let ptr = data.as_ptr() as *const u8;
        let len = std::mem::size_of_val(data);
        let buf = unsafe { std::slice::from_raw_parts(ptr, len) };
        dest.write_all(buf).unwrap();

        // Write the records.
        let ptr = records.as_ptr() as *const u8;
        let len = records.len() * std::mem::size_of::<raw::SourceRecord>();
        let buf = unsafe { std::slice::from_raw_parts(ptr, len) };
        dest.write_all(buf).unwrap();

        // Write the strings.
        dest.write_all(&raw_strings).unwrap();

        assert_eq!(orig_data.as_ref(), dest);
    }

    #[test]
    fn test_basic() {
        let file = File::open(fixture("il2cpp/artificial.usym")).unwrap();
        let data = ByteView::map_file_ref(&file).unwrap();
        let usyms = UsymSymbols::parse(&data).unwrap();

        assert_eq!(usyms.version(), 2);
        assert_eq!(
            usyms.id().unwrap(),
            DebugId::from_str("153d10d10db033d6aacda4e1948da97b").unwrap()
        );
        assert_eq!(usyms.name(), "UnityFramework");
        assert_eq!(usyms.os(), "mac");
        assert_eq!(usyms.arch().unwrap(), Arch::Arm64);

        for i in 0..5 {
            assert!(usyms.get_record(i).is_some());
        }
    }

    #[test]
    fn test_header_with_errors() {
        let data = ByteView::open(fixture("il2cpp/artificial-bad-meta.usym")).unwrap();
        // TODO: We could probably just accept non-UTF8 strings because Rust handles them well
        // enough and inserts placeholders
        assert!(UsymSymbols::parse(&data).is_err());

        // assert_eq!(usyms.version(), 2);
        // assert!(usyms.id().is_err());
        // assert_eq!(usyms.name(), "��ityFramework");
        // assert_eq!(usyms.os(), "mac");
        // assert_eq!(usyms.arch().unwrap(), Arch::Arm64);
    }

    #[test]
    fn test_with_managed() {
        let file = File::open(fixture("il2cpp/managed.usym")).unwrap();
        let data = ByteView::map_file_ref(&file).unwrap();
        let usyms = UsymSymbols::parse(&data).unwrap();

        assert_eq!(usyms.version(), 2);
        assert_eq!(
            usyms.id().unwrap(),
            DebugId::from_str("153d10d10db033d6aacda4e1948da97b").unwrap()
        );
        assert_eq!(usyms.name(), "UnityFramework");
        assert_eq!(usyms.os(), "mac");
        assert_eq!(usyms.arch().unwrap(), Arch::Arm64);

        let mut mapping = match usyms.lookup_source_record(8253832).unwrap() {
            UsymSourceRecord::Mapped(mapping) => mapping,
            UsymSourceRecord::Unmapped(_) => panic!("could not find mapping at addr 8253832"),
        };
        assert_eq!(mapping.managed_symbol, "NewBehaviourScript.Start()");
        assert_eq!(
            mapping.managed_file_info.path_str(),
            "/Users/bitfox/_Workspace/IL2CPP/Assets/NewBehaviourScript.cs"
        );
        assert_eq!(mapping.managed_line, 10);

        mapping = match usyms.lookup_source_record(8253836).unwrap() {
            UsymSourceRecord::Mapped(mapping) => mapping,
            UsymSourceRecord::Unmapped(_) => panic!("could not find mapping at addr 8253836"),
        };
        assert_eq!(mapping.managed_symbol, "NewBehaviourScript.Start()");
        assert_eq!(
            mapping.managed_file_info.path_str(),
            "/Users/bitfox/_Workspace/IL2CPP/Assets/NewBehaviourScript.cs"
        );
        assert_eq!(mapping.managed_line, 10);

        mapping = match usyms.lookup_source_record(8253840).unwrap() {
            UsymSourceRecord::Mapped(mapping) => mapping,
            UsymSourceRecord::Unmapped(_) => panic!("could not find mapping at addr 8253840"),
        };
        assert_eq!(mapping.managed_symbol, "NewBehaviourScript.Update()");
        assert_eq!(
            mapping.managed_file_info.path_str(),
            "/Users/bitfox/_Workspace/IL2CPP/Assets/NewBehaviourScript.cs"
        );
        assert_eq!(mapping.managed_line, 17);
    }

    #[test]
    fn test_sorted_addresses() {
        let file = File::open(fixture("il2cpp/artificial.usym")).unwrap();
        let data = ByteView::map_file_ref(&file).unwrap();
        let usyms = UsymSymbols::parse(&data).unwrap();

        let mut last_address = usyms.records[0].address;
        for i in 1..usyms.header.record_count as usize {
            // The addresses should be weakly monotonic
            assert!(usyms.records[i].address >= last_address);
            last_address = usyms.records[i].address;
        }
    }
}
