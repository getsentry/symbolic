//! Utilities specifically for working with JavaScript specific debug info.
//!
//! This for the most part only contains utility functions to parse references
//! out of minified JavaScript files and source maps.  For actually working
//! with source maps this module is insufficient.

use debugid::DebugId;
use memchr::memmem::FinderRev;
use serde::Deserialize;

/// Parses a sourceMappingURL comment in a file to discover a sourcemap reference.
///
/// Any query string or fragments the URL might contain will be stripped away.
pub fn discover_sourcemaps_location(contents: &str) -> Option<&str> {
    let finder = MagicCommentFinder::source_mapping_url().allow_deprecated_at_variant();

    if let Some(url) = finder.find(contents) {
        // The URL might contain a query string or fragment. Strip those away before recording the URL.
        let without_query = url.split_once('?').map(|x| x.0).unwrap_or(url);
        let without_fragment = without_query
            .split_once('#')
            .map(|x| x.0)
            .unwrap_or(without_query);

        return Some(without_fragment);
    }

    None
}

/// Quickly reads the embedded `debug_id` key from a source map.
///
/// Both `debugId` and `debug_id` are supported as field names. If both
/// are set, the latter takes precedence.
pub fn discover_sourcemap_embedded_debug_id(contents: &str) -> Option<DebugId> {
    // Deserialize from `"debugId"` or `"debug_id"`,
    // preferring the latter.
    #[derive(Deserialize)]
    struct DebugIdInSourceMap {
        #[serde(rename = "debugId")]
        debug_id_new: Option<DebugId>,
        #[serde(rename = "debug_id")]
        debug_id_old: Option<DebugId>,
    }

    serde_json::from_str(contents)
        .ok()
        .and_then(|x: DebugIdInSourceMap| x.debug_id_old.or(x.debug_id_new))
}

/// Parses a `debugId` comment in a file to discover a sourcemap's debug ID.
pub fn discover_debug_id(contents: &str) -> Option<DebugId> {
    MagicCommentFinder::debug_id()
        .find(contents)
        .and_then(|s| s.parse().ok())
}

/// A helper utility which allows searching for magic comments in JavaScript sources.
///
/// The finder will optionally consider the `//#` variant as well as the deprecated `//@` variant.
///
/// Generally considered a magic comment is a comment following the pattern: `//[#@]\s<name>=\s*<value>\s*.*`
struct MagicCommentFinder<'a> {
    finder: FinderRev<'a>,
    allow_at: bool,
}

impl<'a> MagicCommentFinder<'a> {
    fn new(pattern: &'a str) -> Self {
        Self {
            finder: FinderRev::new(pattern),
            allow_at: false,
        }
    }

    /// Creates a new [`Self`] for `debugId`s,
    pub fn debug_id() -> Self {
        Self::new("debugId=")
    }

    /// Creates a new [`Self`] for `sourceMappingURL`s,
    pub fn source_mapping_url() -> Self {
        Self::new("sourceMappingURL=")
    }

    /// Also considers magic comments starting with `//@`.
    pub fn allow_deprecated_at_variant(mut self) -> Self {
        self.allow_at = true;
        self
    }

    /// Finds the last occurrence of the magic comment in the supplied string.
    ///
    /// Returns the value of the magic comment.
    pub fn find<'h>(&self, haystack: &'h str) -> Option<&'h str> {
        let haystack = haystack.as_bytes();
        let mut matches = self.finder.rfind_iter(haystack);

        let value = loop {
            let pos = matches.next()?;
            let prefix = haystack.get(pos.checked_sub(4)?..pos)?;

            // The match starts at needle, e.g. `sourceMappingURL=`, check if the characters
            // before, are what we expect.
            let is_match = (prefix == b"//# ") || (self.allow_at && prefix == b"//@ ");
            if is_match {
                break haystack.get(pos + self.finder.needle().len()..)?;
            }
        };

        // Trim whitespaces after the `=`.
        let value = value.trim_ascii_start();
        // Split until the next whitespace:
        let value = value
            .split(u8::is_ascii_whitespace)
            .next()
            // If there is no whitespace, assume until end.
            .unwrap_or(value);

        // This should never fail, the input was a valid string, we only trimmed characters in the
        // ascii range.
        std::str::from_utf8(value).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! test_sourcemaps_location {
        ($name:ident, $input:expr, $expected:expr) => {
            #[test]
            fn $name() {
                assert_eq!(discover_sourcemaps_location($input), $expected);
            }
        };
    }

    test_sourcemaps_location!(
        test_discover_sourcemaps_location_standalone_line,
        "//# sourceMappingURL=foo.js.map\n",
        Some("foo.js.map")
    );
    test_sourcemaps_location!(
        test_discover_sourcemaps_location_no_trailing_newline,
        "//# sourceMappingURL=foo.js.map",
        Some("foo.js.map")
    );
    test_sourcemaps_location!(
        test_discover_sourcemaps_location_after_code,
        "var a=1;\n//# sourceMappingURL=foo.js.map\n",
        Some("foo.js.map")
    );
    test_sourcemaps_location!(
        test_discover_sourcemaps_location_after_code_no_newline,
        "var a=1;\n//# sourceMappingURL=foo.js.map",
        Some("foo.js.map")
    );
    test_sourcemaps_location!(
        test_discover_sourcemaps_location_deprecated_at_variant,
        "//@ sourceMappingURL=foo.js.map\n",
        Some("foo.js.map")
    );
    test_sourcemaps_location!(
        test_discover_sourcemaps_location_query_string_stripped,
        "//# sourceMappingURL=foo.js.map?v=1&t=abc\n",
        Some("foo.js.map")
    );
    test_sourcemaps_location!(
        test_discover_sourcemaps_location_fragment_stripped,
        "//# sourceMappingURL=foo.js.map#hash\n",
        Some("foo.js.map")
    );
    test_sourcemaps_location!(
        test_discover_sourcemaps_location_query_and_fragment_stripped,
        "//# sourceMappingURL=foo.js.map?v=1#hash\n",
        Some("foo.js.map")
    );
    test_sourcemaps_location!(
        test_discover_sourcemaps_location_whitespace_after_eq,
        "//# sourceMappingURL=  foo.js.map\n",
        Some("foo.js.map")
    );
    test_sourcemaps_location!(
        test_discover_sourcemaps_location_trailing_whitespace,
        "//# sourceMappingURL=  foo.js.map  ",
        Some("foo.js.map")
    );
    test_sourcemaps_location!(
        test_discover_sourcemaps_location_trailing_tabs,
        "//# sourceMappingURL=foo.js.map\t \t",
        Some("foo.js.map")
    );
    test_sourcemaps_location!(
        test_discover_sourcemaps_location_space_in_value,
        "//# sourceMappingURL=foo bar.map",
        Some("foo")
    );
    test_sourcemaps_location!(
        test_discover_sourcemaps_location_trailing_junk,
        "//# sourceMappingURL=foo.js.map junk",
        Some("foo.js.map")
    );
    test_sourcemaps_location!(
        test_discover_sourcemaps_location_trailing_comment,
        "//# sourceMappingURL=foo.js.map //comment",
        Some("foo.js.map")
    );
    test_sourcemaps_location!(
        test_discover_sourcemaps_location_last_occurrence_wins,
        "//# sourceMappingURL=first.js.map\n//# sourceMappingURL=second.js.map\n",
        Some("second.js.map")
    );
    test_sourcemaps_location!(
        test_discover_sourcemaps_location_none_when_missing,
        "var a = 1;\n",
        None
    );
    test_sourcemaps_location!(
        test_discover_sourcemaps_location_none_without_comment_prefix,
        "sourceMappingURL=foo.js.map\n",
        None
    );
    test_sourcemaps_location!(
        test_discover_sourcemaps_location_data_url,
        "//# sourceMappingURL=data:application/json;base64,eyJ2ZXJzaW9uIjozfQ==\n",
        Some("data:application/json;base64,eyJ2ZXJzaW9uIjozfQ==")
    );
    test_sourcemaps_location!(
        test_discover_sourcemaps_location_relative_path,
        "//# sourceMappingURL=../maps/foo.js.map\n",
        Some("../maps/foo.js.map")
    );
    test_sourcemaps_location!(
        test_discover_sourcemaps_location_absolute_url,
        "//# sourceMappingURL=https://example.com/foo.js.map\n",
        Some("https://example.com/foo.js.map")
    );
    test_sourcemaps_location!(
        test_discover_sourcemaps_location_absolute_url_query_stripped,
        "//# sourceMappingURL=https://example.com/foo.js.map?token=abc\n",
        Some("https://example.com/foo.js.map")
    );

    macro_rules! test_debug_id {
        ($name:ident, $input:expr, $expected:expr) => {
            #[test]
            fn $name() {
                assert_eq!(discover_debug_id($input), $expected);
            }
        };
    }

    test_debug_id!(
        test_discover_debug_id_standalone_line,
        "//# debugId=00000000-0000-0000-0000-000000000000\n",
        Some(DebugId::default())
    );
    test_debug_id!(
        test_discover_debug_id_no_trailing_newline,
        "//# debugId=00000000-0000-0000-0000-000000000000",
        Some(DebugId::default())
    );
    test_debug_id!(
        test_discover_debug_id_after_code,
        "var a=1;\n//# debugId=00000000-0000-0000-0000-000000000000\n",
        Some(DebugId::default())
    );
    test_debug_id!(
        test_discover_debug_id_after_code_no_newline,
        "var a=1;\n//# debugId=00000000-0000-0000-0000-000000000000",
        Some(DebugId::default())
    );
    test_debug_id!(
        test_discover_debug_id_whitespace_after_eq,
        "//# debugId=  00000000-0000-0000-0000-000000000000\n",
        Some(DebugId::default())
    );
    test_debug_id!(
        test_discover_debug_id_trailing_whitespace,
        "//# debugId=  00000000-0000-0000-0000-000000000000  ",
        Some(DebugId::default())
    );
    test_debug_id!(
        test_discover_debug_id_trailing_tabs,
        "//# debugId=00000000-0000-0000-0000-000000000000\t \t",
        Some(DebugId::default())
    );
    test_debug_id!(
        test_discover_debug_id_trailing_junk,
        "//# debugId=00000000-0000-0000-0000-000000000000 junk",
        Some(DebugId::default())
    );
    test_debug_id!(
        test_discover_debug_id_trailing_comment,
        "//# debugId=00000000-0000-0000-0000-000000000000 //comment",
        Some(DebugId::default())
    );
    test_debug_id!(
        test_discover_debug_id_last_occurrence_wins,
        "//# debugId=00000000-0000-0000-0000-000000000000\n//# debugId=11111111-1111-1111-1111-111111111111\n",
        Some("11111111-1111-1111-1111-111111111111".parse().unwrap())
    );
    test_debug_id!(
        test_discover_debug_id_none_when_missing,
        "var a = 1;\n",
        None
    );
    test_debug_id!(
        test_discover_debug_id_none_without_comment_prefix,
        "debugId=00000000-0000-0000-0000-000000000000\n",
        None
    );
    test_debug_id!(
        test_discover_debug_id_none_invalid_id,
        "//# debugId=not-a-valid-id\n",
        None
    );
    test_debug_id!(
        test_discover_debug_id_at_magic_comment_not_allowed,
        "//@ debugId=00000000-0000-0000-0000-000000000000\n",
        None
    );

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

    #[test]
    fn test_debugid_both() {
        let input = r#"{
         "version":3,
         "sources":["coolstuff.js"],
         "names":["x","alert"],
         "mappings":"AAAA,GAAIA,GAAI,EACR,IAAIA,GAAK,EAAG,CACVC,MAAM",
         "debug_id":"00000000-0000-0000-0000-000000000000",
         "debugId":"11111111-1111-1111-1111-111111111111"
     }"#;

        assert_eq!(
            discover_sourcemap_embedded_debug_id(input),
            Some(DebugId::default())
        );
    }
}
