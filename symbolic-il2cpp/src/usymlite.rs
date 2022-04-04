//! Experimental parser for the UsymLite format.
//!
//! This format can map il2cpp instruction addresses to managed file names and line numbers.
//!
//! Current state: This can parse the UsymLite format, but a method to get the source location
//! based on the native instruction pointer does not yet exist.

use std::borrow::Cow;
use std::error::Error;
use std::ffi::CStr;
use std::fmt;
use std::mem;
use std::os::raw::c_char;
use std::ptr;

use thiserror::Error;

/// The error type for [`UsymLiteError`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum UsymLiteErrorKind {
    /// Buffer to usym file is misaligned.
    MisalignedBuffer,
    /// The header to the usym file is missing or undersized.
    BadHeader,
    /// The magic string in the header is missing or malformed.
    BadMagic,
    /// The version string in the usym file's header is missing or malformed.
    BadVersion,
    /// The line count in the header can't be read.
    BadLineCount,
    /// The size of the usym file is smaller than the amount of data it is supposed to hold
    /// according to its header.
    BufferSmallerThanAdvertised,
    /// The string table is missing.
    MissingStringTable,
    /// The string table is not terminated by a NULL byte.
    UnterminatedStringTable,
    /// A valid slice to the usym's source records could not be created.
    BadLines,
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

impl fmt::Display for UsymLiteErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UsymLiteErrorKind::MisalignedBuffer => write!(f, "misaligned pointer to buffer"),
            UsymLiteErrorKind::BadHeader => write!(f, "missing or undersized header"),
            UsymLiteErrorKind::BadMagic => write!(f, "missing breakpad symbol header"),
            UsymLiteErrorKind::BadVersion => write!(f, "missing or wrong version number"),
            UsymLiteErrorKind::BadLineCount => write!(f, "unreadable record count"),
            UsymLiteErrorKind::BufferSmallerThanAdvertised => {
                write!(f, "buffer does not contain all data header claims it has")
            }
            UsymLiteErrorKind::MissingStringTable => write!(f, "string table is missing"),
            UsymLiteErrorKind::UnterminatedStringTable => {
                write!(f, "string table does not end with a NULL byte")
            }
            UsymLiteErrorKind::BadLines => {
                write!(f, "could not construct list of source records")
            }
            UsymLiteErrorKind::BadId => write!(f, "assembly ID is missing or unreadable"),
            UsymLiteErrorKind::BadName => write!(f, "assembly name is missing or unreadable"),
            UsymLiteErrorKind::BadOperatingSystem => {
                write!(f, "operating system is missing or unreadable")
            }
            UsymLiteErrorKind::BadArchitecture => {
                write!(f, "architecture is missing or unreadable")
            }
            UsymLiteErrorKind::BadEncoding => {
                write!(f, "part of the file is not encoded in valid UTF-8")
            }
        }
    }
}

/// An error when dealing with [`BreakpadObject`](struct.BreakpadObject.html).
#[derive(Debug, Error)]
#[error("{kind}")]
pub struct UsymLiteError {
    kind: UsymLiteErrorKind,
    #[source]
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl UsymLiteError {
    /// Creates a new Breakpad error from a known kind of error as well as an arbitrary error
    /// payload.
    fn new<E>(kind: UsymLiteErrorKind, source: E) -> Self
    where
        E: Into<Box<dyn Error + Send + Sync>>,
    {
        let source = Some(source.into());
        Self { kind, source }
    }

    /// Returns the corresponding [`UsymLiteErrorKind`] for this error.
    pub fn kind(&self) -> UsymLiteErrorKind {
        self.kind
    }
}

impl From<UsymLiteErrorKind> for UsymLiteError {
    fn from(kind: UsymLiteErrorKind) -> Self {
        Self { kind, source: None }
    }
}

// TODO: Follow the same structure as usyms and introduce a raw module and other
// types to distinguish between raw and parsed reps?
#[derive(Debug, Clone)]
#[repr(C)]
struct UsymLiteHeader {
    /// Magic number identifying the file, `b"sym-"`.
    magic: u32,
    /// Version of the usym file format.
    version: u32,
    /// Number of UsymLiteLine records.
    line_count: u32,
    /// Executable's id, offset in the string table.
    ///
    /// This is a hex-formatted UUID.
    id: u32,
    /// Executable's os, offset in the string table.
    os: u32,
    /// Executable's arch, offset in the string table.
    arch: u32,
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct UsymLiteLine {
    address: u64,
    /// Reference to the managed source file name in the string table.
    filename: u32,
    /// Managed line number.
    line: u32,
}

pub struct UsymLiteSymbols<'a> {
    header: &'a UsymLiteHeader,
    lines: &'a [UsymLiteLine],
    /// The string table.
    ///
    /// This is a large slice of bytes with null-terminated C strings.
    string_table: &'a [u8],
}

impl<'a> UsymLiteSymbols<'a> {
    const MAGIC: &'static [u8] = b"sym-";

    pub fn parse(buf: &'a [u8]) -> Result<UsymLiteSymbols<'a>, UsymLiteError> {
        if buf.as_ptr().align_offset(8) != 0 {
            // Alignment only really matters for performance really.
            return Err(UsymLiteError::from(UsymLiteErrorKind::MisalignedBuffer));
        }
        if buf.len() < mem::size_of::<UsymLiteHeader>() {
            return Err(UsymLiteError::from(UsymLiteErrorKind::BadHeader));
        }
        if buf.get(..Self::MAGIC.len()) != Some(Self::MAGIC) {
            return Err(UsymLiteError::from(UsymLiteErrorKind::BadMagic));
        }

        // SAFETY: We checked the buffer is large enough above.
        let header = unsafe { &*(buf.as_ptr() as *const UsymLiteHeader) };
        if header.version != 2 {
            return Err(UsymLiteError::from(UsymLiteErrorKind::BadVersion));
        }
        let line_count: usize = header
            .line_count
            .try_into()
            .map_err(|e| UsymLiteError::new(UsymLiteErrorKind::BadLineCount, e))?;
        let stringtable_offset =
            mem::size_of::<UsymLiteHeader>() + line_count * mem::size_of::<UsymLiteLine>();
        if buf.len() < stringtable_offset {
            return Err(UsymLiteError::from(
                UsymLiteErrorKind::BufferSmallerThanAdvertised,
            ));
        }

        // SAFETY: We checked the buffer is at least the size_of::<UsymLiteHeader>() above.
        let lines_ptr = unsafe { buf.as_ptr().add(mem::size_of::<UsymLiteHeader>()) };

        // SAFETY: We checked the buffer has enough space for all the line records above.
        let lines = unsafe {
            let lines_ptr: *const UsymLiteLine = lines_ptr.cast();
            let lines_ptr = ptr::slice_from_raw_parts(lines_ptr, line_count);
            lines_ptr
                .as_ref()
                .ok_or_else(|| UsymLiteError::from(UsymLiteErrorKind::BadLines))
        }?;

        let stringtable = buf
            .get(stringtable_offset..)
            .ok_or_else(|| UsymLiteError::from(UsymLiteErrorKind::MissingStringTable))?;
        if stringtable.last() != Some(&0u8) {
            return Err(UsymLiteError::from(
                UsymLiteErrorKind::UnterminatedStringTable,
            ));
        }

        Ok(Self {
            header,
            lines,
            string_table: stringtable,
        })
    }

    /// Returns a string from the string table at given offset.
    ///
    /// Offsets are as provided by some [`UsymLiteHeader`] and [`UsymLiteLine`] fields.
    fn get_string(&self, offset: u32) -> Option<&'a CStr> {
        // Panic if size_of::<usize>() < size_of::<u32>().
        let offset: usize = offset.try_into().unwrap();
        if offset >= self.string_table.len() {
            return None;
        }

        let table_ptr = self.string_table.as_ptr();

        // SAFETY: We checked offset is inside the stringtable.
        let string_ptr = unsafe { table_ptr.add(offset) as *const c_char };

        // SAFETY: the stringtable is guaranteed to end in a NULL byte by
        // [`UsymSymbols::parse`].
        let string = unsafe { CStr::from_ptr(string_ptr) };
        Some(string)
    }

    pub fn id(&self) -> Result<Cow<'a, str>, UsymLiteError> {
        self.get_string(self.header.id)
            .map(|s| s.to_string_lossy())
            .ok_or_else(|| UsymLiteError::from(UsymLiteErrorKind::BadId))
    }

    pub fn os(&self) -> Result<Cow<'a, str>, UsymLiteError> {
        self.get_string(self.header.os)
            .map(|s| s.to_string_lossy())
            .ok_or_else(|| UsymLiteError::from(UsymLiteErrorKind::BadOperatingSystem))
    }

    pub fn arch(&self) -> Result<Cow<'a, str>, UsymLiteError> {
        self.get_string(self.header.arch)
            .map(|s| s.to_string_lossy())
            .ok_or_else(|| UsymLiteError::from(UsymLiteErrorKind::BadArchitecture))
    }

    pub fn get_record(&self, index: usize) -> Option<&UsymLiteLine> {
        self.lines.get(index)
    }
}

#[cfg(test)]
mod tests {
    use std::{fs::File, io};

    use symbolic_common::ByteView;
    use symbolic_testutils::fixture;

    use super::*;

    fn empty_usymlite() -> Result<ByteView<'static>, io::Error> {
        let file = File::open(fixture("il2cpp/empty.usymlite"))?;
        ByteView::map_file_ref(&file)
    }

    #[test]
    fn test_parse_header() {
        let data = empty_usymlite().unwrap();
        let info = UsymLiteSymbols::parse(&data).unwrap();

        assert_eq!(
            info.header.magic,
            u32::from_le_bytes([b's', b'y', b'm', b'-'])
        );
        assert_eq!(info.header.version, 2);
        assert_eq!(info.header.line_count, 0);

        assert_eq!(info.id().unwrap(), "153d10d10db033d6aacda4e1948da97b");
        assert_eq!(info.os().unwrap(), "mac");
        assert_eq!(info.arch().unwrap(), "arm64");
    }
}
