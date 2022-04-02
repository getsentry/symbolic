//! Parser for the Usym format.
//!
//! This format can map il2cpp instruction addresses to managed file names and line numbers.

use std::borrow::Cow;
use std::error::Error;
use std::fmt;
use std::mem;
use std::ptr;

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
    InvalidVersion,
    /// The record count in the header can't be read.
    BadRecordCount,
    /// The size of the usym file is smaller than the amount of data it is supposed to hold
    /// according to its header.
    BufferSmallerThanAdvertised,
    /// The string table is missing.
    MissingStringTable,
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
            UsymErrorKind::BadMagic => write!(f, "missing breakpad symbol header"),
            UsymErrorKind::InvalidVersion => write!(f, "invalid version number"),
            UsymErrorKind::BadRecordCount => write!(f, "unreadable record count"),
            UsymErrorKind::BufferSmallerThanAdvertised => {
                write!(f, "buffer does not contain all data header claims it has")
            }
            UsymErrorKind::MissingStringTable => write!(f, "string table is missing"),
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

/// An error when dealing with [`BreakpadObject`](struct.BreakpadObject.html).
#[derive(Debug, Error)]
#[error("{kind}")]
pub struct UsymError {
    kind: UsymErrorKind,
    #[source]
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl UsymError {
    /// Creates a new Breakpad error from a known kind of error as well as an arbitrary error
    /// payload.
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

// TODO: consider introducing newtype for string table offsets and the string table itself

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
        /// These follow right after the header, and after them is the string table.
        pub(super) record_count: u32,

        /// UUID of the assembly, as an offset into string table.
        pub(super) id: u32,

        /// Name of the "assembly", as an offset into string table.
        pub(super) name: u32,

        /// Name of OS, as an offset into string table.
        pub(super) os: u32,

        /// Name of architecture, as an offset into string table.
        pub(super) arch: u32,
    }

    /// A record mapping an IL2CPP instruction address to managed code location.
    ///
    /// This is the raw record as it appears in the file, see [`UsymRecord`] for a record with
    /// the names resolved.
    #[derive(Debug, Clone, Copy)]
    #[repr(C, packed)]
    pub(super) struct SourceRecord {
        /// Instruction pointer address, relative to base address of assembly.
        pub(super) address: u64,
        /// Managed symbol name, as an offset into the string table.
        pub(super) symbol: u32,
        /// Managed source file, as an offset into the string table.
        pub(super) file: u32,
        /// Managed line number.
        pub(super) line: u32,
        // These might not even be u64, it's just 128 bits we don't know.
        _unknown0: u64,
        _unknown1: u64,
    }
}

/// A record mapping an IL2CPP instruction address to managed code location.
#[derive(Debug, Clone)]
pub struct UsymSourceRecord<'a> {
    /// Instruction pointer address, relative to the base of the assembly.
    pub address: u64,
    /// Symbol name of the managed code.
    pub symbol: Cow<'a, str>,
    /// File name of the managed code.
    pub file: Cow<'a, str>,
    /// Line number of the managed code.
    pub line: u32,
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
    /// This is not a traditional string table but rather a large slice of bytes with
    /// length-prefixed strings, the length is a little-endian u16.  The header and records
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
            return Err(UsymError::from(UsymErrorKind::MisalignedBuffer));
        }
        if buf.len() < mem::size_of::<raw::Header>() {
            return Err(UsymError::from(UsymErrorKind::BadHeader));
        }
        if buf.get(..Self::MAGIC.len()) != Some(Self::MAGIC) {
            return Err(UsymError::from(UsymErrorKind::BadMagic));
        }

        // SAFETY: We checked the buffer is large enough above.
        let header = unsafe { &*(buf.as_ptr() as *const raw::Header) };
        if header.version != 2 {
            return Err(UsymError::from(UsymErrorKind::InvalidVersion));
        }

        let record_count: usize = header
            .record_count
            .try_into()
            .map_err(|e| UsymError::new(UsymErrorKind::BadRecordCount, e))?;
        // TODO: consider trying to just grab the records and give up on their strings if something
        // is wrong with the string table
        let strings_offset =
            mem::size_of::<raw::Header>() + record_count * mem::size_of::<raw::SourceRecord>();
        if buf.len() < strings_offset {
            return Err(UsymError::from(UsymErrorKind::BufferSmallerThanAdvertised));
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
            .ok_or_else(|| UsymError::from(UsymErrorKind::MissingStringTable))?;
        // TODO: null byte checking at the end of the string table?

        let id_offset = header.id.try_into().unwrap();
        let id = match Self::get_string_from_offset(strings, id_offset)
            .ok_or_else(|| UsymError::from(UsymErrorKind::BadId))?
        {
            Cow::Borrowed(id) => id,
            Cow::Owned(_) => return Err(UsymError::from(UsymErrorKind::BadEncoding)),
        };
        let name_offset = header.name.try_into().unwrap();
        let name = match Self::get_string_from_offset(strings, name_offset)
            .ok_or_else(|| UsymError::from(UsymErrorKind::BadName))?
        {
            Cow::Borrowed(name) => name,
            Cow::Owned(_) => return Err(UsymError::from(UsymErrorKind::BadEncoding)),
        };

        let os_offset = header.os.try_into().unwrap();
        let os = match Self::get_string_from_offset(strings, os_offset)
            .ok_or_else(|| UsymError::from(UsymErrorKind::BadOperatingSystem))?
        {
            Cow::Borrowed(name) => name,
            Cow::Owned(_) => return Err(UsymError::from(UsymErrorKind::BadEncoding)),
        };

        let arch_offset = header.arch.try_into().unwrap();
        let arch = match Self::get_string_from_offset(strings, arch_offset)
            .ok_or_else(|| UsymError::from(UsymErrorKind::BadArchitecture))?
        {
            Cow::Borrowed(name) => name,
            Cow::Owned(_) => return Err(UsymError::from(UsymErrorKind::BadEncoding)),
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

    fn get_string_from_offset(data: &[u8], offset: usize) -> Option<Cow<str>> {
        let size_bytes = data.get(offset..offset + 2)?;
        let size: usize = u16::from_le_bytes([size_bytes[0], size_bytes[1]]).into();

        let start_offset = offset + 2;
        let end_offset = start_offset + size;

        let string_bytes = data.get(start_offset..end_offset)?;
        Some(String::from_utf8_lossy(string_bytes))
    }

    /// Returns a string from the string table at given offset.
    ///
    /// Offsets are as provided by some [`UsymLiteHeader`] and [`UsymLiteLine`] fields.
    fn get_string(&self, offset: usize) -> Option<Cow<'a, str>> {
        Self::get_string_from_offset(self.strings, offset)
    }

    /// The ID of the assembly.
    ///
    /// This should match the ID of the debug symbols.
    // TODO: Consider making this return debugid::DebugId
    pub fn id(&self) -> &str {
        self.id
    }

    /// The name of the assembly.
    pub fn name(&self) -> &str {
        self.name
    }

    /// The Operating System name.
    pub fn os(&self) -> &str {
        self.os
    }

    /// The architecture name.
    pub fn arch(&self) -> &str {
        self.arch
    }

    /// Returns a [`UsymSourceRecord`] at the given index it was stored.
    ///
    /// Not that useful, you have no idea what index you want.
    pub fn get_record(&self, index: usize) -> Option<UsymSourceRecord> {
        let raw = self.records.get(index)?;
        Some(UsymSourceRecord {
            address: raw.address,
            symbol: self.get_string(raw.symbol.try_into().unwrap())?,
            file: self.get_string(raw.file.try_into().unwrap())?,
            line: raw.line,
        })
    }

    /// Lookup the managed code source location for an IL2CPP instruction pointer.
    pub fn lookup_source_record(&self, ip: u64) -> Option<UsymSourceRecord> {
        // TODO: need to subtract the image base to get relative address
        match self.records.binary_search_by_key(&ip, |r| r.address) {
            Ok(index) => self.get_record(index),
            Err(index) => self.get_record(index - 1),
        }
    }

    // TODO: Add iterator over records?
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
        let file = File::open(fixture("il2cpp/artificial.usym")).unwrap();

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

        // Construct new header.
        let mut header = usyms.header.clone();
        header.id = push_string(usyms.get_string(header.id as usize).unwrap()) as u32;
        header.name = push_string(usyms.get_string(header.name as usize).unwrap()) as u32;
        header.os = push_string(usyms.get_string(header.os as usize).unwrap()) as u32;
        header.arch = push_string(usyms.get_string(header.arch as usize).unwrap()) as u32;

        // Construct new records.
        header.record_count = 5;
        let mut records = Vec::new();
        for mut record in usyms.records.iter().cloned().take(5) {
            record.symbol = push_string(usyms.get_string(record.symbol as usize).unwrap()) as u32;
            record.file = push_string(usyms.get_string(record.file as usize).unwrap()) as u32;
            records.push(record);
        }

        // let mut dest = File::create(
        //     "/Users/flub/code/symbolic/symbolic-testutils/fixtures/il2cpp/artificial.usym",
        // )
        // .unwrap();
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
        assert_eq!(usyms.id(), "153d10d10db033d6aacda4e1948da97b");
        assert_eq!(usyms.name(), "UnityFramework");
        assert_eq!(usyms.os(), "mac");
        assert_eq!(usyms.arch(), "arm64");
    }

    #[test]
    fn test_sorted_addresses() {
        let file = File::open(fixture("il2cpp/artificial.usym")).unwrap();
        let data = ByteView::map_file_ref(&file).unwrap();
        let usyms = UsymSymbols::parse(&data).unwrap();

        let mut last_address = usyms.records[0].address;
        for i in 1..usyms.header.record_count as usize {
            assert!(usyms.records[i].address > last_address);
            last_address = usyms.records[i].address;
        }
    }
}
