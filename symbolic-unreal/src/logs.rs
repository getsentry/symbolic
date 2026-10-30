use std::sync::LazyLock;

use anylog::LogEntry;
use chrono::{DateTime, Utc};
use regex::Regex;
#[cfg(any(test, feature = "serde"))]
use time::format_description::well_known::Rfc3339;

use crate::Unreal4ErrorKind;
use crate::error::Unreal4Error;

#[cfg(test)]
use similar_asserts::assert_eq;

/// <https://github.com/EpicGames/UnrealEngine/blob/f509bb2d6c62806882d9a10476f3654cf1ee0634/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformTime.cpp#L79-L93>
///
/// Note: Date is always in US format (dd/MM/yyyy) and time is local
/// Example: Log file open, 12/13/18 15:54:53
static LOG_FIRST_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Log file open, (?P<month>\d\d)/(?P<day>\d\d)/(?P<year>\d\d) (?P<hour>\d\d):(?P<minute>\d\d):(?P<second>\d\d)$").unwrap()
});

/// The `[time][frame]` prefix that starts a log entry, in any of the formats selected by the
/// `log.Timestamp` console variable.
///
/// Examples: `[2018.10.29-16.56.37:542][  0]`, `[0130.29][420]`
static LOG_ENTRY_TIME_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[[^\[\]]*\]\[\s*\d+\]").unwrap());

/// The `Category: ` prefix that starts a log entry written before timestamps are enabled.
static LOG_ENTRY_CATEGORY_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*: ").unwrap());

/// Returns whether the line starts a new log entry.
///
/// Unreal only prefixes the first line of a log message, so lines that don't start an entry are
/// continuations of a multi-line message, such as an Aftermath GPU crash dump.
fn is_entry_start(line: &str) -> bool {
    LOG_ENTRY_TIME_PREFIX.is_match(line) || LOG_ENTRY_CATEGORY_PREFIX.is_match(line)
}

#[cfg(feature = "serde")]
fn serialize_timestamp<S: serde::Serializer>(
    timestamp: &Option<time::OffsetDateTime>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::Error;
    match timestamp {
        Some(timestamp) => serializer.serialize_str(&match timestamp.format(&Rfc3339) {
            Ok(s) => s,
            Err(_) => return Err(S::Error::custom("failed formatting `OffsetDateTime`")),
        }),
        None => serializer.serialize_none(),
    }
}

/// A log entry from an Unreal Engine 4 crash.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Unreal4LogEntry {
    /// The timestamp of the message, when available.
    #[cfg_attr(
        feature = "serde",
        serde(
            skip_serializing_if = "Option::is_none",
            serialize_with = "serialize_timestamp"
        )
    )]
    pub timestamp: Option<time::OffsetDateTime>,

    /// The component that issued the log, when available.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub component: Option<String>,

    /// The log message.
    pub message: String,
}

fn parse_log_datetime(text: &str) -> Option<time::OffsetDateTime> {
    let captures = LOG_FIRST_LINE.captures(text)?;

    // https://github.com/EpicGames/UnrealEngine/blob/f7626ddd147fe20a6144b521a26739c863546f4a/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformTime.cpp#L46
    let month = time::Month::try_from(captures["month"].parse::<u8>().ok()?).ok()?;
    let date = time::Date::from_calendar_date(
        captures["year"].parse::<i32>().ok()? + 2000,
        month,
        captures["day"].parse::<u8>().ok()?,
    )
    .ok()?;

    let datetime = date
        .with_hms(
            captures["hour"].parse::<u8>().ok()?,
            captures["minute"].parse::<u8>().ok()?,
            captures["second"].parse::<u8>().ok()?,
        )
        .ok()?;

    // Using UTC but this entry is local time. Unfortunately there's no way to find the offset.
    Some(datetime.assume_utc())
}

fn convert_chrono(dt: DateTime<Utc>) -> Option<time::OffsetDateTime> {
    time::OffsetDateTime::from_unix_timestamp(dt.timestamp()).ok()
}

impl Unreal4LogEntry {
    /// Tries to parse a blob normally coming from a logs file inside an Unreal4Crash into
    /// a vector of Unreal4LogEntry.
    ///
    /// Returns up to `limit` entries from the end of the log. The lines of a multi-line log
    /// message are joined into a single entry.
    pub fn parse(log_slice: &[u8], limit: usize) -> Result<Vec<Self>, Unreal4Error> {
        let logs_utf8 = std::str::from_utf8(log_slice)
            .map_err(|e| Unreal4Error::new(Unreal4ErrorKind::InvalidLogEntry, e))?;

        let mut lines = logs_utf8.lines();

        // First line is the file header. It includes the timestamp of the following 100 and some
        // lines until log entries actually include timestamps
        let mut fallback_timestamp = lines.next().and_then(parse_log_datetime);

        let mut logs = Vec::new();
        // Continuation lines of the entry being read, from the bottom up.
        let mut continuation = Vec::new();

        for line in lines.rev() {
            if logs.len() == limit {
                break;
            }

            if is_entry_start(line) {
                logs.push(Self::from_lines(
                    line,
                    &continuation,
                    &mut fallback_timestamp,
                ));
                continuation.clear();
            } else {
                continuation.push(line);
            }
        }

        // Lines above the first entry start can't be grouped, so each becomes its own entry.
        for line in continuation {
            if logs.len() == limit {
                break;
            }

            logs.push(Self::from_lines(line, &[], &mut fallback_timestamp));
        }

        logs.reverse();
        Ok(logs)
    }

    /// Creates an entry from its first line and its continuation lines, given from the bottom up.
    fn from_lines(
        line: &str,
        continuation: &[&str],
        fallback_timestamp: &mut Option<time::OffsetDateTime>,
    ) -> Self {
        let entry = LogEntry::parse(line.as_bytes());
        let (component, message) = entry.component_and_message();
        // Reads in reverse where logs include timestamp. If it never reached the point of
        // adding timestamp to log entries, the first record's timestamp (local time, above)
        // will be used on all records.
        *fallback_timestamp = entry
            .utc_timestamp()
            .and_then(convert_chrono)
            .or(*fallback_timestamp);

        // Blank lines at the end of a message only separate it from the next entry.
        let trailing_blank = continuation
            .iter()
            .take_while(|line| line.trim().is_empty())
            .count();

        let mut message = message.to_owned();
        for line in continuation[trailing_blank..].iter().rev() {
            message.push('\n');
            message.push_str(line);
        }

        Unreal4LogEntry {
            timestamp: *fallback_timestamp,
            component: component.map(Into::into),
            message,
        }
    }
}

#[test]
fn test_parse_logs_no_entries_with_timestamp() {
    let log_bytes = br"Log file open, 12/13/18 15:54:53
LogWindows: Failed to load 'aqProf.dll' (GetLastError=126)
LogWindows: File 'aqProf.dll' does not exist";

    let logs = Unreal4LogEntry::parse(log_bytes, 1000).expect("logs");

    assert_eq!(logs.len(), 2);
    assert_eq!(logs[1].component.as_ref().expect("component"), "LogWindows");
    assert_eq!(
        logs[1]
            .timestamp
            .expect("timestamp")
            .format(&Rfc3339)
            .unwrap(),
        "2018-12-13T15:54:53Z"
    );
    assert_eq!(logs[1].message, "File 'aqProf.dll' does not exist");
}

#[test]
fn test_parse_logs_invalid_time() {
    let log_bytes = br"Log file open, 12/13/18 99:99:99
LogWindows: Failed to load 'aqProf.dll' (GetLastError=126)
LogWindows: File 'aqProf.dll' does not exist";

    let logs = Unreal4LogEntry::parse(log_bytes, 1000).expect("logs");

    assert_eq!(logs.len(), 2);
    assert_eq!(logs[1].timestamp, None);
}

#[test]
fn test_parse_logs_multiline_entries() {
    let log_bytes = br#"Log file open, 10/30/26 13:02:53
LogInit: Session CrashGUID >=====
         Session CrashGUID >   UECC-Windows-0000
         Session CrashGUID >=====
LogConfig: No local boot hotfix file found
[2026.10.30-11.03.11:626][561]LogD3D12RHI: Error: hr failed
 at D:\build\WindowsD3D12PipelineState.cpp:856
 with error DXGI_ERROR_DEVICE_REMOVED

[2026.10.30-11.03.11:645][561]LogNvidiaAftermath: Error:

Writing Aftermath dump to: D3D12.nv-gpudmp
[
  {
    "ApplicationName": "UE-SentryTower"
  }
]


[2026.10.30-11.03.11:704][561]LogD3D12RHI: Error: DRED: No PageFault data."#;

    let logs = Unreal4LogEntry::parse(log_bytes, 1000).expect("logs");

    assert_eq!(logs.len(), 5);

    assert_eq!(logs[0].component.as_deref(), Some("LogInit"));
    assert_eq!(
        logs[0].message,
        "Session CrashGUID >=====\n         Session CrashGUID >   UECC-Windows-0000\n         Session CrashGUID >====="
    );

    assert_eq!(logs[1].component.as_deref(), Some("LogConfig"));
    assert_eq!(logs[1].message, "No local boot hotfix file found");

    assert_eq!(logs[2].component.as_deref(), Some("LogD3D12RHI"));
    assert_eq!(
        logs[2].message,
        "Error: hr failed\n at D:\\build\\WindowsD3D12PipelineState.cpp:856\n with error DXGI_ERROR_DEVICE_REMOVED"
    );

    assert_eq!(logs[3].component.as_deref(), Some("LogNvidiaAftermath"));
    assert_eq!(
        logs[3].message,
        "Error:\n\nWriting Aftermath dump to: D3D12.nv-gpudmp\n[\n  {\n    \"ApplicationName\": \"UE-SentryTower\"\n  }\n]"
    );

    assert_eq!(logs[4].component.as_deref(), Some("LogD3D12RHI"));
    assert_eq!(logs[4].message, "Error: DRED: No PageFault data.");
}

#[test]
fn test_parse_logs_other_timestamp_formats() {
    // `log.Timestamp=2` writes the seconds since the engine started.
    let log_bytes = br"Log file open, 10/30/26 13:02:53
[0130.29][420]LogD3D12RHI: Error: hr failed
 at D:\build\WindowsD3D12PipelineState.cpp:856
[0130.30][421]LogD3D12RHI: Error: DRED: No PageFault data.";

    let logs = Unreal4LogEntry::parse(log_bytes, 1000).expect("logs");

    assert_eq!(logs.len(), 2);
    assert!(
        logs[0]
            .message
            .ends_with("hr failed\n at D:\\build\\WindowsD3D12PipelineState.cpp:856")
    );
    assert!(logs[1].message.ends_with("DRED: No PageFault data."));
}

#[test]
fn test_parse_logs_no_entry_starts() {
    // Without timestamps and categories, lines can't be grouped.
    let log_bytes = br"Log file open, 10/30/26 13:02:53
Failed to load 'aqProf.dll' (GetLastError=126)
  File 'aqProf.dll' does not exist";

    let logs = Unreal4LogEntry::parse(log_bytes, 1000).expect("logs");

    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0].component, None);
    assert_eq!(
        logs[0].message,
        "Failed to load 'aqProf.dll' (GetLastError=126)"
    );
    assert_eq!(logs[1].component, None);
    assert_eq!(logs[1].message, "  File 'aqProf.dll' does not exist");
}
