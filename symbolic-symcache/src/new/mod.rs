//! The SymCache binary format.
//!
//!
use std::{mem, ptr};

mod error;
//mod lookup;
pub(crate) mod raw;

pub use error::Error;
//pub use lookup::*;
use raw::align_to_eight;

type Result<T, E = Error> = std::result::Result<T, E>;

/// The serialized SymCache binary format.
///
/// This can be parsed from a binary buffer via [`Format::parse`], and lookups on it can be performed
/// via the [`Format::lookup`] method.
#[derive(Debug, PartialEq, Eq)]
pub struct SymCache<'data> {
    header: &'data raw::Header,
    strings: &'data [raw::String],
    files: &'data [raw::File],
    functions: &'data [raw::Function],
    source_locations: &'data [raw::SourceLocation],
    ranges: &'data [raw::Range],
    string_bytes: &'data [u8],
}

impl<'data> SymCache<'data> {
    /// Parse the SymCache binary format into a convenient type that allows safe access and allows
    /// fast lookups.
    ///
    /// See the [raw module](raw) for an explanation of the binary format.
    pub fn parse(buf: &'data [u8]) -> Result<Self> {
        if align_to_eight(buf.as_ptr() as usize) != 0 {
            return Err(Error::BufferNotAligned);
        }

        let mut header_size = mem::size_of::<raw::Header>();
        header_size += align_to_eight(header_size);

        if buf.len() < header_size {
            return Err(Error::HeaderTooSmall);
        }
        // SAFETY: we checked that the buffer is well aligned and large enough to fit a `raw::Header`.
        let header = unsafe { &*(buf.as_ptr() as *const raw::Header) };
        if header.magic == raw::SYMCACHE_MAGIC_FLIPPED {
            return Err(Error::WrongEndianness);
        }
        if header.magic != raw::SYMCACHE_MAGIC {
            return Err(Error::WrongFormat);
        }
        if header.version != raw::SYMCACHE_VERSION {
            return Err(Error::WrongVersion);
        }

        let mut strings_size = mem::size_of::<raw::String>() * header.num_strings as usize;
        strings_size += align_to_eight(strings_size);

        let mut files_size = mem::size_of::<raw::File>() * header.num_files as usize;
        files_size += align_to_eight(files_size);

        let mut functions_size = mem::size_of::<raw::Function>() * header.num_functions as usize;
        functions_size += align_to_eight(functions_size);

        let mut source_locations_size =
            mem::size_of::<raw::SourceLocation>() * header.num_source_locations as usize;
        source_locations_size += align_to_eight(source_locations_size);

        let mut ranges_size = mem::size_of::<raw::Range>() * header.num_ranges as usize;
        ranges_size += align_to_eight(ranges_size);

        let expected_buf_size = header_size
            + strings_size
            + files_size
            + functions_size
            + source_locations_size
            + ranges_size
            + header.string_bytes as usize;

        if buf.len() != expected_buf_size || source_locations_size < ranges_size {
            return Err(Error::BadFormatLength);
        }

        // SAFETY: we just made sure that all the pointers we are constructing via pointer
        // arithmetic are within `buf`
        let strings_start = unsafe { buf.as_ptr().add(header_size) };
        let files_start = unsafe { strings_start.add(strings_size) };
        let functions_start = unsafe { files_start.add(files_size) };
        let source_locations_start = unsafe { functions_start.add(functions_size) };
        let ranges_start = unsafe { source_locations_start.add(source_locations_size) };
        let string_bytes_start = unsafe { ranges_start.add(ranges_size) };

        // SAFETY: the above buffer size check also made sure we are not going out of bounds
        // here
        let strings = unsafe {
            &*(ptr::slice_from_raw_parts(strings_start, header.num_strings as usize)
                as *const [raw::String])
        };
        let files = unsafe {
            &*(ptr::slice_from_raw_parts(files_start, header.num_files as usize)
                as *const [raw::File])
        };
        let functions = unsafe {
            &*(ptr::slice_from_raw_parts(functions_start, header.num_functions as usize)
                as *const [raw::Function])
        };
        let source_locations = unsafe {
            &*(ptr::slice_from_raw_parts(
                source_locations_start,
                header.num_source_locations as usize,
            ) as *const [raw::SourceLocation])
        };
        let ranges = unsafe {
            &*(ptr::slice_from_raw_parts(ranges_start, header.num_ranges as usize)
                as *const [raw::Range])
        };
        let string_bytes = unsafe {
            &*(ptr::slice_from_raw_parts(string_bytes_start, header.string_bytes as usize)
                as *const [u8])
        };

        Ok(SymCache {
            header,
            strings,
            files,
            functions,
            source_locations,
            ranges,
            string_bytes,
        })
    }

    /// Resolves a string reference to the pointed-to `&str` data.
    fn get_string(&self, string_idx: u32) -> Option<&'data str> {
        if string_idx == u32::MAX {
            return None;
        }
        let string = self.strings.get(string_idx as usize)?;

        let start_offset = string.string_offset as usize;
        let end_offset = start_offset + string.string_len as usize;
        let bytes = self.string_bytes.get(start_offset..end_offset)?;

        std::str::from_utf8(bytes).ok()
    }

    /// The version of the SymCache file format.
    pub fn version(&self) -> u32 {
        self.header.version
    }
}
