//! Experimental parser for the UsymLite format.
//!
//! This format can map il2cpp instruction addresses to managed file names and line numbers.
//!
//! Current state: This can parse the UsymLite format, but a method to get the source location
//! based on the native instruction pointer does not yet exist.

use std::borrow::Cow;
use std::ffi::CStr;
use std::mem;
use std::os::raw::c_char;
use std::ptr;

use anyhow::{Error, Result};

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

    pub fn parse(buf: &'a [u8]) -> Result<UsymLiteSymbols<'a>> {
        if buf.as_ptr().align_offset(8) != 0 {
            // Alignment only really matters for performance really.
            return Err(Error::msg("Data buffer not aligned to 8 bytes"));
        }
        if buf.len() < mem::size_of::<UsymLiteHeader>() {
            return Err(Error::msg("Data smaller than UsymLiteHeader"));
        }
        if buf.get(..4) != Some(Self::MAGIC) {
            return Err(Error::msg("Wrong magic number"));
        }

        // SAFETY: We checked the buffer is large enough above.
        let header = unsafe { &*(buf.as_ptr() as *const UsymLiteHeader) };
        if header.version != 2 {
            return Err(Error::msg("Unknown version"));
        }
        let line_count: usize = header.line_count.try_into()?;
        let stringtable_offset =
            mem::size_of::<UsymLiteHeader>() + line_count * mem::size_of::<UsymLiteLine>();
        if buf.len() < stringtable_offset {
            return Err(Error::msg("Data smaller than number of line records"));
        }

        // SAFETY: We checked the buffer is at least the size_of::<UsymLiteHeader>() above.
        let lines_ptr = unsafe { buf.as_ptr().add(mem::size_of::<UsymLiteHeader>()) };

        // SAFETY: We checked the buffer has enough space for all the line records above.
        let lines = unsafe {
            let lines_ptr: *const UsymLiteLine = lines_ptr.cast();
            let lines_ptr = ptr::slice_from_raw_parts(lines_ptr, line_count);
            lines_ptr
                .as_ref()
                .ok_or_else(|| Error::msg("lines_ptr was null pointer!"))
        }?;

        let stringtable = buf
            .get(stringtable_offset..)
            .ok_or_else(|| Error::msg("No string table found"))?;
        if stringtable.last() != Some(&0u8) {
            return Err(Error::msg("String table does not end in NULL byte"));
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

    pub fn id(&self) -> Result<Cow<'a, str>> {
        let s = self
            .get_string(self.header.id)
            .ok_or_else(|| Error::msg("bad offset or stringtable"))?;
        Ok(s.to_string_lossy())
    }

    pub fn os(&self) -> Result<Cow<'a, str>> {
        let s = self
            .get_string(self.header.os)
            .ok_or_else(|| Error::msg("bad offset or stringtable"))?;
        Ok(s.to_string_lossy())
    }

    pub fn arch(&self) -> Result<Cow<'a, str>> {
        let s = self
            .get_string(self.header.arch)
            .ok_or_else(|| Error::msg("bad offset or string table"))?;
        Ok(s.to_string_lossy())
    }

    pub fn get_record(&self, index: usize) -> Option<&UsymLiteLine> {
        self.lines.get(index)
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use symbolic_common::ByteView;
    use symbolic_testutils::fixture;

    use super::*;

    fn empty_usymlite() -> Result<ByteView<'static>> {
        let file = File::open(fixture("il2cpp/empty.usymlite"))?;
        ByteView::map_file_ref(&file).map_err(Into::into)
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
