//! Utilities specifically for working with JavaScript specific debug info.
//!
//! This for the most part only contains utility functions to parse references
//! out of minified JavaScript files and source maps.  For actually working
//! with source maps this module is insufficient.

use debugid::DebugId;
use serde::Deserialize;

/// Parses a sourceMappingURL comment in a file to discover a sourcemap reference.
///
/// Any query string or fragments the URL might contain will be stripped away.
pub fn discover_sourcemaps_location(contents: &str) -> Option<&str> {
    for line in contents.lines().rev() {
        if line.starts_with("//# sourceMappingURL=") || line.starts_with("//@ sourceMappingURL=") {
            let url = line[21..].trim();

            // The URL might contain a query string or fragment. Strip those away before recording the URL.
            let without_query = url.split_once('?').map(|x| x.0).unwrap_or(url);
            let without_fragment = without_query
                .split_once('#')
                .map(|x| x.0)
                .unwrap_or(without_query);

            return Some(without_fragment);
        }
    }
    None
}

/// Quickly reads the embedded `debug_id` key from a source map.
///
/// Both `debug_id` and `debugId` are supported as field names.
pub fn discover_sourcemap_embedded_debug_id(contents: &str) -> Option<DebugId> {
    #[derive(Deserialize)]
    struct DebugIdInSourceMap {
        #[serde(alias = "debugId")]
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

#[cfg(test)]
mod tests {
    use debugid::DebugId;

    use crate::js::discover_sourcemap_embedded_debug_id;

    #[test]
    fn test_debugid_snake_case() {
        let input = r#"{
         "version":3,
         "sources":["coolstuff.js"],
         "names":["x","alert"],
         "mappings":"AAAA,GAAIA,GAAI,EACR,IAAIA,GAAK,EAAG,CACVC,MAAM",
         "debug_id":"00000000-0000-0000-0000-000000000000"
     }"#;

        assert_eq!(
            discover_sourcemap_embedded_debug_id(input),
            Some(DebugId::default())
        );
    }

    #[test]
    fn test_debugid_camel_case() {
        let input = r#"{
         "version":3,
         "sources":["coolstuff.js"],
         "names":["x","alert"],
         "mappings":"AAAA,GAAIA,GAAI,EACR,IAAIA,GAAK,EAAG,CACVC,MAAM",
         "debugId":"00000000-0000-0000-0000-000000000000"
     }"#;

        assert_eq!(
            discover_sourcemap_embedded_debug_id(input),
            Some(DebugId::default())
        );
    }
}
