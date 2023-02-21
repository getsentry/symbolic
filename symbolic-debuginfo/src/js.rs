//! Utilities specifically for working with JavaScript specific debug info.
//!
//! This for the most part only contains utility functions to parse references
//! out of minified JavaScript files and source maps.  For actually working
//! with source maps this module is insufficient.

use debugid::DebugId;
use serde::Deserialize;

/// Parses a sourceMappingURL comment in a file to discover a sourcemap reference.
pub fn discover_sourcemaps_location(contents: &str) -> Option<&str> {
    for line in contents.lines().rev() {
        if line.starts_with("//# sourceMappingURL=") || line.starts_with("//@ sourceMappingURL=") {
            return Some(line[21..].trim());
        }
    }
    None
}

/// Quickly reads the embedded `debug_id` key from a source map.
pub fn discover_sourcemap_embedded_debug_id(contents: &str) -> Option<DebugId> {
    #[derive(Deserialize)]
    struct DebugIdInSourceMap {
        debug_id: Option<DebugId>,
    }

    serde_json::from_str(contents)
        .ok()
        .and_then(|x: DebugIdInSourceMap| x.debug_id)
}

/// Parses a `debugId` comment in a file to discover a sourcemap's debug ID.
pub fn discover_debug_id(contents: &str) -> Option<DebugId> {
    for line in contents.lines().rev() {
        if let Some(rest) = line.strip_prefix("//# debugId=") {
            return rest.trim().parse().ok();
        }
    }
    None
}
