//! API to process Unreal Engine 4 crashes.
#![warn(missing_docs)]

use std::fmt;
use std::io::{self, Cursor, Read};

use anylog::LogEntry;
use bytes::{Buf, Bytes};
use chrono::{DateTime, TimeZone, Utc};
use compress::zlib;
use failure::Fail;
use lazy_static::lazy_static;
use regex::Regex;

#[cfg(feature = "with-serde")]
use serde::Serialize;

use crate::context::Unreal4Context;

lazy_static! {
    /// https://github.com/EpicGames/UnrealEngine/blob/f509bb2d6c62806882d9a10476f3654cf1ee0634/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformTime.cpp#L79-L93
    /// Note: Date is always in US format (dd/MM/yyyy) and time is local
    /// Example: Log file open, 12/13/18 15:54:53
    static ref LOG_FIRST_LINE: Regex = Regex::new(r"Log file open, (?P<month>\d\d)/(?P<day>\d\d)/(?P<year>\d\d) (?P<hour>\d\d):(?P<minute>\d\d):(?P<second>\d\d)$").unwrap();
}

mod context;

/// Errors related to parsing an UE4 crash file.
#[derive(Fail, Debug)]
pub enum Unreal4Error {
    /// Expected UnrealEngine4 crash (zlib compressed).
    #[fail(display = "unknown bytes format")]
    UnknownBytesFormat,

    /// Empty data blob received.
    #[fail(display = "empty crash")]
    Empty,

    /// Value out of bounds.
    #[fail(display = "out of bounds")]
    OutOfBounds,

    /// Invalid compressed data.
    #[fail(display = "bad compression")]
    BadCompression(io::Error),

    /// Can't process log entry.
    #[fail(display = "invalid log entry")]
    InvalidLogEntry(std::str::Utf8Error),

    /// Invalid XML
    #[fail(display = "invalid xml")]
    InvalidXml(elementtree::Error),
}

struct Unreal4Header {
    pub directory_name: String,
    pub file_name: String,
    pub uncompressed_size: i32,
    pub file_count: i32,
}

impl Unreal4Header {
    fn read(cursor: &mut Cursor<&[u8]>) -> Self {
        Unreal4Header {
            directory_name: read_ansi_string(cursor),
            file_name: read_ansi_string(cursor),
            uncompressed_size: cursor.get_i32_le(),
            file_count: cursor.get_i32_le(),
        }
    }
}

/// The type of the file within the UE4 crash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unreal4FileType {
    /// Minidump.
    Minidump,
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

/// The type of native crash report contained in the unreal 4 crash
#[derive(Debug, Clone)]
pub enum NativeCrash<'a> {
    /// A crash report that is a minidump
    Minidump(&'a [u8]),
    /// A crash report that is an apple text file crash report
    AppleCrashReport(&'a str),
}

/// Meta-data about a file within a UE4 crash file.
#[derive(Clone, Debug)]
pub struct Unreal4FileMeta {
    /// The original index within the UE4 crash file.
    pub index: usize,
    /// File name.
    pub file_name: String,
    /// Start of the file within crash dumb.
    pub offset: usize,
    /// Length of bytes from offset.
    pub len: usize,
}

impl Unreal4FileMeta {
    fn read(cursor: &mut Cursor<&[u8]>) -> Self {
        let meta = Unreal4FileMeta {
            index: cursor.get_i32_le() as usize,
            file_name: read_ansi_string(cursor),
            len: cursor.get_i32_le() as usize,
            offset: cursor.position() as usize,
        };

        cursor.advance(meta.len);
        meta
    }

    /// Returns the file type.
    pub fn ty(&self) -> Unreal4FileType {
        match self.file_name.as_str() {
            // https://github.com/EpicGames/UnrealEngine/blob/5e997dc7b5a4efb7f1be22fa8c4875c9c0034394/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp#L60
            "UE4Minidump.dmp" => Unreal4FileType::Minidump,
            // https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/Mac/MacPlatformMisc.cpp#L1636
            "minidump.dmp" => Unreal4FileType::Minidump,
            "CrashReportClient.ini" => Unreal4FileType::Config,
            "CrashContext.runtime-xml" => Unreal4FileType::Context,
            name => {
                if name.ends_with(".log") {
                    Unreal4FileType::Log
                } else {
                    Unreal4FileType::Unknown
                }
            }
        }
    }
}

/// A log entry from an Unreal Engine 4 crash.
#[cfg_attr(feature = "with-serde", derive(Serialize))]
pub struct Unreal4LogEntry {
    /// The timestamp of the message, when available.
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub timestamp: Option<DateTime<Utc>>,

    /// The component that issued the log, when available.
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub component: Option<String>,

    /// The log message.
    pub message: String,
}

/// Unreal Engine 4 crash file.
#[derive(Debug)]
pub struct Unreal4Crash {
    bytes: Bytes,
    files: Vec<Unreal4FileMeta>,
}

impl Unreal4Crash {
    /// Creates an instance of `Unreal4Crash` from the original, compressed bytes.
    pub fn parse(bytes: &[u8]) -> Result<Unreal4Crash, Unreal4Error> {
        if bytes.is_empty() {
            return Err(Unreal4Error::Empty);
        }

        let mut zlib_decoder = zlib::Decoder::new(bytes);

        let mut decompressed = Vec::new();
        zlib_decoder
            .read_to_end(&mut decompressed)
            .map_err(Unreal4Error::BadCompression)?;

        let decompressed = Bytes::from(decompressed);
        let files = get_files_from_slice(&decompressed)?;

        Ok(Unreal4Crash {
            bytes: decompressed,
            files,
        })
    }

    /// Files within the UE4 crash dump.
    pub fn files(&self) -> impl Iterator<Item = &Unreal4FileMeta> {
        self.files.iter()
    }

    /// Count of files within the UE4 crash dump.
    pub fn file_count(&self) -> usize {
        self.files.len() as usize
    }

    /// Get a `Unreal4FileMeta` by its index.
    pub fn file_by_index(&self, index: usize) -> Option<&Unreal4FileMeta> {
        self.files().find(|f| f.index == index)
    }

    /// Get the contents of a file by its index.
    pub fn file_contents_by_index(&self, index: usize) -> Result<Option<&[u8]>, Unreal4Error> {
        match self.file_by_index(index) {
            Some(f) => Ok(Some(self.get_file_contents(f)?)),
            None => Ok(None),
        }
    }

    /// Returns the native crash report contained.
    pub fn get_native_crash(&self) -> Result<Option<NativeCrash<'_>>, Unreal4Error> {
        Ok(self
            .get_file_slice(Unreal4FileType::Minidump)?
            .and_then(|bytes| {
                if bytes.get(..4) == Some(b"MDMP") {
                    return Some(NativeCrash::Minidump(bytes));
                }
                if bytes.get(..20) == Some(b"Incident Identifier:") {
                    if let Ok(s) = std::str::from_utf8(bytes) {
                        return Some(NativeCrash::AppleCrashReport(s));
                    }
                }
                None
            }))
    }

    /// Get the Minidump file bytes.
    pub fn get_minidump_slice(&self) -> Result<Option<&[u8]>, Unreal4Error> {
        Ok(self.get_native_crash()?.and_then(|ft| {
            if let NativeCrash::Minidump(md) = ft {
                Some(md)
            } else {
                None
            }
        }))
    }

    /// Gets the native apple crash report as str.
    pub fn get_apple_crash_report(&self) -> Result<Option<&str>, Unreal4Error> {
        Ok(self.get_native_crash()?.and_then(|ft| {
            if let NativeCrash::AppleCrashReport(s) = ft {
                Some(s)
            } else {
                None
            }
        }))
    }

    /// Get the file contents by its file type.
    pub fn get_file_slice(
        &self,
        file_type: Unreal4FileType,
    ) -> Result<Option<&[u8]>, Unreal4Error> {
        let file = match self.files().find(|f| f.ty() == file_type) {
            Some(m) => m,
            None => return Ok(None),
        };

        Ok(Some(self.get_file_contents(file)?))
    }

    /// Get file content.
    pub fn get_file_contents(&self, file_meta: &Unreal4FileMeta) -> Result<&[u8], Unreal4Error> {
        let end = file_meta
            .offset
            .checked_add(file_meta.len)
            .ok_or(Unreal4Error::OutOfBounds)?;
        self.bytes
            .get(file_meta.offset..end)
            .ok_or(Unreal4Error::OutOfBounds)
    }

    /// Get the `Unreal4Context` of this crash.
    /// This is achieved by reading the context (xml) file
    /// If the file doesn't exist in the crash, `None` is returned.
    pub fn get_context(&self) -> Result<Option<Unreal4Context>, Unreal4Error> {
        Unreal4Context::from_crash(self)
    }

    /// Get up to `limit` log entries of this crash.
    pub fn get_logs(&self, limit: usize) -> Result<Vec<Unreal4LogEntry>, Unreal4Error> {
        match self.get_file_slice(Unreal4FileType::Log)? {
            Some(f) => parse_log_from_slice(f, limit),
            None => Ok(Vec::new()),
        }
    }
}

fn parse_log_from_slice(
    log_slice: &[u8],
    limit: usize,
) -> Result<Vec<Unreal4LogEntry>, Unreal4Error> {
    let mut fallback_timestamp = None;
    let logs_utf8 = std::str::from_utf8(log_slice).map_err(Unreal4Error::InvalidLogEntry)?;

    if let Some(first_line) = logs_utf8.lines().next() {
        // First line includes the timestamp of the following 100 and some lines until
        // log entries actually include timestamps
        if let Some(captures) = LOG_FIRST_LINE.captures(&first_line) {
            fallback_timestamp = Some(
                // Using UTC but this entry is local time. Unfortunately there's no way to find the offset.
                Utc.ymd(
                    // https://github.com/EpicGames/UnrealEngine/blob/f7626ddd147fe20a6144b521a26739c863546f4a/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformTime.cpp#L46
                    captures["year"].parse::<i32>().unwrap() + 2000,
                    captures["month"].parse::<u32>().unwrap(),
                    captures["day"].parse::<u32>().unwrap(),
                )
                .and_hms(
                    captures["hour"].parse::<u32>().unwrap(),
                    captures["minute"].parse::<u32>().unwrap(),
                    captures["second"].parse::<u32>().unwrap(),
                ),
            );
        }
    }

    let mut logs: Vec<_> = logs_utf8
        .lines()
        .rev()
        .take(limit)
        .map(|line| {
            let entry = LogEntry::parse(line.as_bytes());
            let (component, message) = entry.component_and_message();
            // Reads in reverse where logs include timestamp. If it never reached the point of adding
            // timestamp to log entries, the first record's timestamp (local time, above) will be used
            // on all records.
            fallback_timestamp = entry.utc_timestamp().or(fallback_timestamp);

            Unreal4LogEntry {
                timestamp: fallback_timestamp,
                component: component.map(Into::into),
                message: message.into(),
            }
        })
        .collect();

    logs.reverse();
    Ok(logs)
}

fn read_ansi_string(buffer: &mut Cursor<&[u8]>) -> String {
    let size = buffer.get_u32_le() as usize;
    let dir_name = String::from_utf8_lossy(&Buf::bytes(&buffer)[..size]).into_owned();
    buffer.advance(size);
    dir_name.trim_end_matches('\0').into()
}

fn get_files_from_slice(bytes: &[u8]) -> Result<Vec<Unreal4FileMeta>, Unreal4Error> {
    let mut rv = vec![];

    let file_count = Cursor::new(
        &bytes
            .get(bytes.len() - 4..)
            .ok_or(Unreal4Error::OutOfBounds)?,
    )
    .get_i32_le();

    let mut cursor = Cursor::new(&bytes[..]);
    Unreal4Header::read(&mut cursor);

    for _ in 0..file_count {
        rv.push(Unreal4FileMeta::read(&mut cursor));
    }

    Ok(rv)
}

#[test]
fn test_from_slice_empty_buffer() {
    let crash = &[];

    let result = Unreal4Crash::from_slice(crash);

    assert!(match result.expect_err("empty crash") {
        Unreal4Error::Empty => true,
        _ => false,
    })
}

#[test]
fn test_from_slice_invalid_input() {
    let crash = &[0u8; 1];

    let result = Unreal4Crash::from_slice(crash);

    let err = match result.expect_err("empty crash") {
        Unreal4Error::BadCompression(b) => b.to_string(),
        _ => panic!(),
    };

    assert_eq!("unexpected EOF", err)
}

#[test]
fn test_parse_log_from_slice_no_entries_with_timestamp() {
    let log_bytes = br"Log file open, 12/13/18 15:54:53
LogWindows: Failed to load 'aqProf.dll' (GetLastError=126)
LogWindows: File 'aqProf.dll' does not exist";

    let logs = parse_log_from_slice(log_bytes, 1000).expect("logs");

    assert_eq!(logs.len(), 3);
    assert_eq!(logs[2].component.as_ref().expect("component"), "LogWindows");
    assert_eq!(
        logs[2].timestamp.expect("timestamp").to_rfc3339(),
        "2018-12-13T15:54:53+00:00"
    );
    assert_eq!(logs[2].message, "File 'aqProf.dll' does not exist");
}
