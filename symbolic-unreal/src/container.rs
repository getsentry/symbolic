//! API to process Unreal Engine 4 crashes.
#![warn(missing_docs)]

use std::fmt;
use std::io::Read;
use std::iter::FusedIterator;
use std::ops::Deref;

use bytes::Bytes;
use flate2::bufread::ZlibDecoder;
use scroll::{ctx::TryFromCtx, Endian, Pread};

use crate::context::Unreal4Context;
use crate::error::{Unreal4Error, Unreal4ErrorKind};
use crate::logs::Unreal4LogEntry;

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct AnsiString(String);

impl AnsiString {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for AnsiString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Deref for AnsiString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for AnsiString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl TryFromCtx<'_, Endian> for AnsiString {
    type Error = scroll::Error;

    fn try_from_ctx(data: &[u8], context: Endian) -> Result<(Self, usize), Self::Error> {
        let mut offset = 0;

        // Read the length and data of this string
        let len = data.gread_with::<u32>(&mut offset, context)?;
        let bytes = data.gread_with::<&[u8]>(&mut offset, len as usize)?;

        // Convert into UTF-8 and truncate the trailing zeros
        let mut string = String::from_utf8_lossy(bytes).into_owned();
        let actual_len = string.trim_end_matches('\0').len();
        string.truncate(actual_len);

        Ok((Self(string), offset))
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, Pread)]
struct Unreal4Header {
    pub directory_name: AnsiString,
    pub file_name: AnsiString,
    pub uncompressed_size: i32,
    pub file_count: i32,
}

/// Meta-data about a file within a UE4 crash file.
#[derive(Clone, Debug)]
struct Unreal4FileMeta {
    /// The original index within the UE4 crash file.
    index: usize,
    /// File name.
    file_name: AnsiString,
    /// Start of the file within crash dump.
    offset: usize,
    /// Length of bytes from offset.
    len: usize,
}

impl TryFromCtx<'_, usize> for Unreal4FileMeta {
    type Error = scroll::Error;

    fn try_from_ctx(data: &[u8], file_offset: usize) -> Result<(Self, usize), Self::Error> {
        let mut offset = 0;

        let index = data.gread_with::<i32>(&mut offset, scroll::LE)? as usize;
        let file_name = data.gread_with(&mut offset, scroll::LE)?;
        let len = data.gread_with::<i32>(&mut offset, scroll::LE)? as usize;

        let file_meta = Unreal4FileMeta {
            index,
            file_name,
            offset: file_offset + offset,
            len,
        };

        // Ensure that the buffer contains enough data
        if len > 0 {
            data.gread_with::<&[u8]>(&mut offset, len)?;
        }

        Ok((file_meta, offset))
    }
}

fn gread_files(
    bytes: &[u8],
    count: usize,
    offset: &mut usize,
) -> Result<Vec<Unreal4FileMeta>, Unreal4Error> {
    // a `Unreal4FileMeta` is at least 3 * 4 bytes
    if count > bytes.len() / 12 {
        return Err(Unreal4ErrorKind::BadData.into());
    }
    let mut files = Vec::with_capacity(count);
    for _ in 0..count {
        let file_offset = *offset;
        files.push(bytes.gread_with(offset, file_offset)?);
    }
    Ok(files)
}

/// Unreal Engine 4 crash file.
#[derive(Debug)]
pub struct Unreal4Crash {
    bytes: Bytes,
    header: Unreal4Header,
    files: Vec<Unreal4FileMeta>,
}

impl Unreal4Crash {
    fn from_bytes(bytes: Bytes) -> Result<Self, Unreal4Error> {
        let mut offset = 0;

        let (header, files) = if bytes.starts_with(b"CR1") {
            // https://github.com/EpicGames/UnrealEngine/commit/a0471b76577a64e5c4dad89a38dfe7d9611a65ef
            // The 'CR1' marker marks a new version of the file format. There is a single correct
            // header at the start of the file. Start parsing after the 3 byte marker.
            offset = 3;

            let header = bytes.gread_with::<Unreal4Header>(&mut offset, scroll::LE)?;
            let files = gread_files(&bytes, header.file_count as usize, &mut offset)?;

            (header, files)
        } else {
            // The header is repeated at the beginning and the end of the file. The first one is
            // merely a placeholder, the second contains actual information. However, it's not
            // possible to parse it right away, so we only read the file count and parse the rest
            // progressively.
            let file_count = bytes.pread_with::<i32>(bytes.len() - 4, scroll::LE)? as usize;

            // Ignore the initial header and use the one at the end of the file instead.
            bytes.gread_with::<Unreal4Header>(&mut offset, scroll::LE)?;

            let files = gread_files(&bytes, file_count, &mut offset)?;
            let header = bytes.gread_with(&mut offset, scroll::LE)?;

            (header, files)
        };

        if offset != bytes.len() {
            return Err(Unreal4ErrorKind::TrailingData.into());
        }

        Ok(Unreal4Crash {
            bytes,
            header,
            files,
        })
    }

    /// Parses a UE4 crash dump from the original, compressed data.
    ///
    /// To prevent unbounded decompression, consider using
    /// [`parse_with_limit`](Self::parse_with_limit) with an explicit limit, instead.
    pub fn parse(slice: &[u8]) -> Result<Self, Unreal4Error> {
        Self::parse_with_limit(slice, usize::MAX)
    }

    /// Parses a UE4 crash dump from the original, compressed data up to a maximum size limit.
    ///
    /// If files contained within the UE4 crash exceed the given size `limit`, this function returns
    /// `Err` with [`Unreal4ErrorKind::TooLarge`].
    pub fn parse_with_limit(slice: &[u8], limit: usize) -> Result<Self, Unreal4Error> {
        if slice.is_empty() {
            return Err(Unreal4ErrorKind::Empty.into());
        }

        let mut decompressed = Vec::new();
        let decoder = &mut ZlibDecoder::new(slice);

        decoder
            .take(limit as u64)
            .read_to_end(&mut decompressed)
            .map_err(|e| Unreal4Error::new(Unreal4ErrorKind::BadCompression, e))?;

        // The decoder was not exhausted if there's still a byte to read. Given that we're decoding
        // from a slice, it should be safe to ignore `ErrorKind::Interrupted` and the likes.
        if !matches!(decoder.read(&mut [0; 1]), Ok(0)) {
            return Err(Unreal4ErrorKind::TooLarge.into());
        }

        Self::from_bytes(decompressed.into())
    }

    /// Returns the file name of this UE4 crash.
    pub fn name(&self) -> &str {
        &self.header.file_name
    }

    /// Returns the directory path of this UE4 crash.
    pub fn directory_name(&self) -> &str {
        &self.header.directory_name
    }

    /// Returns an iterator over all files within this UE4 crash dump.
    pub fn files(&self) -> Unreal4FileIterator<'_> {
        Unreal4FileIterator {
            inner: self.files.iter(),
            bytes: &self.bytes,
        }
    }

    /// Count of files within the UE4 crash dump.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Returns the file at the given index.
    pub fn file_by_index(&self, index: usize) -> Option<Unreal4File> {
        self.files().nth(index)
    }

    /// Returns a file by its type.
    ///
    /// If there are multiple files matching the given type, the first match is returned.
    pub fn file_by_type(&self, ty: Unreal4FileType) -> Option<Unreal4File> {
        self.files().find(|f| f.ty() == ty)
    }

    /// Returns the native crash report contained.
    pub fn native_crash(&self) -> Option<Unreal4File> {
        self.files().find(|f| {
            f.ty() == Unreal4FileType::Minidump || f.ty() == Unreal4FileType::AppleCrashReport
        })
    }

    /// Get the `Unreal4Context` of this crash.
    ///
    /// This is achieved by reading the context (xml) file
    /// If the file doesn't exist in the crash, `None` is returned.
    pub fn context(&self) -> Result<Option<Unreal4Context>, Unreal4Error> {
        match self.file_by_type(Unreal4FileType::Context) {
            Some(file) => Unreal4Context::parse(file.data()).map(Some),
            None => Ok(None),
        }
    }

    /// Get up to `limit` log entries of this crash.
    pub fn logs(&self, limit: usize) -> Result<Vec<Unreal4LogEntry>, Unreal4Error> {
        match self.file_by_type(Unreal4FileType::Log) {
            Some(file) => Unreal4LogEntry::parse(file.data(), limit),
            None => Ok(Vec::new()),
        }
    }
}

/// The type of the file within the UE4 crash.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Unreal4FileType {
    /// Microsoft or Breakpad Minidump.
    Minidump,
    /// Apple crash report text file.
    AppleCrashReport,
    /// Log file.
    Log,
    /// The .ini config file.
    Config,
    /// The XML context file.
    Context,
    /// Unknown file type.
    Unknown,
}

impl Unreal4FileType {
    /// Returns the display name of this file type.
    pub fn name(self) -> &'static str {
        match self {
            Unreal4FileType::Minidump => "minidump",
            Unreal4FileType::AppleCrashReport => "applecrashreport",
            Unreal4FileType::Log => "log",
            Unreal4FileType::Config => "config",
            Unreal4FileType::Context => "context",
            Unreal4FileType::Unknown => "unknown",
        }
    }
}

impl fmt::Display for Unreal4FileType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// A file withing an `Unreal4Crash`.
///
/// The file internally holds a reference to the entire unreal 4 crash data.
#[derive(Debug)]
pub struct Unreal4File {
    /// The original index within the UE4 crash file.
    index: usize,
    /// The file name.
    file_name: String,
    /// A handle to the data of this file.
    bytes: Bytes,
}

impl Unreal4File {
    /// Creates an instance from the header and data.
    fn from_meta(meta: &Unreal4FileMeta, bytes: &Bytes) -> Self {
        Unreal4File {
            index: meta.index,
            file_name: meta.file_name.as_str().to_owned(),
            bytes: bytes.slice(meta.offset..meta.offset + meta.len),
        }
    }

    /// Returns the original index of this file in the unreal crash.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Returns the file name of this file (without path).
    pub fn name(&self) -> &str {
        &self.file_name
    }

    /// Returns the raw contents of this file.
    pub fn data(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the file type.
    pub fn ty(&self) -> Unreal4FileType {
        if self.name() == "CrashReportClient.ini" {
            Unreal4FileType::Config
        } else if self.name() == "CrashContext.runtime-xml" {
            Unreal4FileType::Context
        } else if self.name().ends_with(".log") {
            Unreal4FileType::Log
        } else if self.data().starts_with(b"MDMP") {
            Unreal4FileType::Minidump
        } else if self.data().starts_with(b"Incident Identifier:") {
            Unreal4FileType::AppleCrashReport
        } else {
            Unreal4FileType::Unknown
        }
    }
}

/// An iterator over `Unreal4File`.
pub struct Unreal4FileIterator<'a> {
    inner: std::slice::Iter<'a, Unreal4FileMeta>,
    bytes: &'a Bytes,
}

impl Iterator for Unreal4FileIterator<'_> {
    type Item = Unreal4File;

    fn next(&mut self) -> Option<Self::Item> {
        let meta = self.inner.next()?;
        Some(Unreal4File::from_meta(meta, self.bytes))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }

    fn count(self) -> usize {
        self.inner.count()
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        let meta = self.inner.nth(n)?;
        Some(Unreal4File::from_meta(meta, self.bytes))
    }
}

impl DoubleEndedIterator for Unreal4FileIterator<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let meta = self.inner.next_back()?;
        Some(Unreal4File::from_meta(meta, self.bytes))
    }
}

impl FusedIterator for Unreal4FileIterator<'_> {}

impl ExactSizeIterator for Unreal4FileIterator<'_> {}

#[cfg(test)]
mod tests {
    use std::{error::Error, fs::File};
    use symbolic_testutils::fixture;

    use super::*;

    #[test]
    fn test_parse_empty_buffer() {
        let crash = &[];

        let result = Unreal4Crash::parse(crash);

        assert!(matches!(
            result.expect_err("empty crash").kind(),
            Unreal4ErrorKind::Empty
        ));
    }

    #[test]
    fn test_parse_invalid_input() {
        let crash = &[0u8; 1];

        let result = Unreal4Crash::parse(crash);
        let error = result.expect_err("empty crash");
        assert_eq!(error.kind(), Unreal4ErrorKind::BadCompression);

        let source = error.source().expect("error source");
        assert_eq!(source.to_string(), "corrupt deflate stream");
    }

    // The size of the unreal_crash fixture when decompressed.
    const DECOMPRESSED_SIZE: usize = 440752;

    #[test]
    fn test_parse_too_large() {
        let mut file = File::open(fixture("unreal/unreal_crash")).expect("example file opens");
        let mut file_content = Vec::new();
        file.read_to_end(&mut file_content).expect("fixture file");

        let result = Unreal4Crash::parse_with_limit(&file_content, DECOMPRESSED_SIZE - 1);
        let error = result.expect_err("too large");
        assert_eq!(error.kind(), Unreal4ErrorKind::TooLarge);
    }

    #[test]
    fn test_parse_fits_exact() {
        let mut file = File::open(fixture("unreal/unreal_crash")).expect("example file opens");
        let mut file_content = Vec::new();
        file.read_to_end(&mut file_content).expect("fixture file");

        Unreal4Crash::parse_with_limit(&file_content, DECOMPRESSED_SIZE)
            .expect("file fits decompression buffer");
    }
}
