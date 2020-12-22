use anylog::LogEntry;
use chrono::{DateTime, TimeZone, Utc};
use lazy_static::lazy_static;
use regex::Regex;

use crate::error::Unreal4Error;
use crate::Unreal4ErrorKind;

lazy_static! {
    /// https://github.com/EpicGames/UnrealEngine/blob/f509bb2d6c62806882d9a10476f3654cf1ee0634/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformTime.cpp#L79-L93
    /// Note: Date is always in US format (dd/MM/yyyy) and time is local
    /// Example: Log file open, 12/13/18 15:54:53
    static ref LOG_FIRST_LINE: Regex = Regex::new(r"Log file open, (?P<month>\d\d)/(?P<day>\d\d)/(?P<year>\d\d) (?P<hour>\d\d):(?P<minute>\d\d):(?P<second>\d\d)$").unwrap();
}

/// A log entry from an Unreal Engine 4 crash.
#[cfg_attr(feature = "serde", derive(serde_::Serialize))]
#[cfg_attr(feature = "serde", serde(crate = "serde_"))]
pub struct Unreal4LogEntry {
    /// The timestamp of the message, when available.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub timestamp: Option<DateTime<Utc>>,

    /// The component that issued the log, when available.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub component: Option<String>,

    /// The log message.
    pub message: String,
}

impl Unreal4LogEntry {
    /// Tries to parse a blob normally coming from a logs file inside an Unreal4Crash into
    /// a vector of Unreal4LogEntry.
    pub fn parse(log_slice: &[u8], limit: usize) -> Result<Vec<Self>, Unreal4Error> {
        let mut fallback_timestamp = None;
        let logs_utf8 = std::str::from_utf8(log_slice)
            .map_err(|e| Unreal4Error::new(Unreal4ErrorKind::InvalidLogEntry, e))?;

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
            .take(limit + 1) // read one more that we need, will be dropped at the end
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

        // drops either the first line in the file, which is the file header and therefore
        // not a valid log, or the (limit+1)-th entry from the bottom which we are not
        // interested in (since we only want (limit) entries).
        logs.pop();
        logs.reverse();
        Ok(logs)
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
        logs[1].timestamp.expect("timestamp").to_rfc3339(),
        "2018-12-13T15:54:53+00:00"
    );
    assert_eq!(logs[1].message, "File 'aqProf.dll' does not exist");
}
