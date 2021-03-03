//! Provides sourcemap support.

#![warn(missing_docs)]

use std::borrow::Cow;
use std::fmt;
use std::ops::Deref;

#[cfg(test)]
use similar_asserts::assert_eq;

/// An error returned when parsing source maps.
#[derive(Debug)]
pub struct ParseSourceMapError(sourcemap::Error);

impl fmt::Display for ParseSourceMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            sourcemap::Error::Io(..) => write!(f, "sourcemap parsing failed with io error"),
            sourcemap::Error::Utf8(..) => write!(f, "sourcemap parsing failed due to bad utf-8"),
            sourcemap::Error::BadJson(..) => write!(f, "invalid json data on sourcemap parsing"),
            ref other => write!(f, "{}", other),
        }
    }
}

impl std::error::Error for ParseSourceMapError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self.0 {
            sourcemap::Error::Io(ref err) => err,
            sourcemap::Error::Utf8(ref err) => err,
            sourcemap::Error::BadJson(ref err) => err,
            _ => return None,
        })
    }
}

impl From<sourcemap::Error> for ParseSourceMapError {
    fn from(error: sourcemap::Error) -> ParseSourceMapError {
        ParseSourceMapError(error)
    }
}

/// Represents JS source code.
pub struct SourceView<'a> {
    sv: sourcemap::SourceView<'a>,
}

enum SourceMapType {
    Regular(sourcemap::SourceMap),
    Hermes(sourcemap::SourceMapHermes),
}

impl Deref for SourceMapType {
    type Target = sourcemap::SourceMap;

    fn deref(&self) -> &Self::Target {
        match self {
            SourceMapType::Regular(sm) => sm,
            SourceMapType::Hermes(smh) => smh,
        }
    }
}

/// Represents a source map.
pub struct SourceMapView {
    sm: SourceMapType,
}

/// A matched token.
#[derive(Debug, Default, PartialEq)]
pub struct TokenMatch<'a> {
    /// The line number in the original source file.
    pub src_line: u32,
    /// The column number in the original source file.
    pub src_col: u32,
    /// The column number in the minifid source file.
    pub dst_line: u32,
    /// The column number in the minified source file.
    pub dst_col: u32,
    /// The source ID of the token.
    pub src_id: u32,
    /// The token name, if present.
    pub name: Option<&'a str>,
    /// The source.
    pub src: Option<&'a str>,
    /// The name of the function containing the token.
    pub function_name: Option<String>,
}

impl<'a> SourceView<'a> {
    /// Creates a view from a string.
    pub fn new(source: &'a str) -> Self {
        SourceView {
            sv: sourcemap::SourceView::new(source),
        }
    }

    /// Creates a view from a string.
    pub fn from_string(source: String) -> Self {
        SourceView {
            sv: sourcemap::SourceView::from_string(source),
        }
    }

    /// Creates a soruce view from bytes ignoring utf-8 errors.
    pub fn from_slice(source: &'a [u8]) -> Self {
        match String::from_utf8_lossy(source) {
            Cow::Owned(s) => SourceView::from_string(s),
            Cow::Borrowed(s) => SourceView::new(s),
        }
    }

    /// Returns the embedded source a string.
    pub fn as_str(&self) -> &str {
        self.sv.source()
    }

    /// Returns a specific line.
    pub fn get_line(&self, idx: u32) -> Option<&str> {
        self.sv.get_line(idx)
    }

    /// Returns the number of lines.
    pub fn line_count(&self) -> usize {
        self.sv.line_count()
    }
}

impl SourceMapView {
    /// Constructs a sourcemap from a slice.
    ///
    /// If the sourcemap is an index it is being flattened.  If flattening
    /// is not possible then an error is raised.
    pub fn from_json_slice(buffer: &[u8]) -> Result<Self, ParseSourceMapError> {
        Ok(SourceMapView {
            sm: match sourcemap::decode_slice(buffer)? {
                sourcemap::DecodedMap::Regular(sm) => SourceMapType::Regular(sm),
                sourcemap::DecodedMap::Index(smi) => SourceMapType::Regular(smi.flatten()?),
                sourcemap::DecodedMap::Hermes(smh) => SourceMapType::Hermes(smh),
            },
        })
    }

    /// Looks up a token and returns it.
    pub fn lookup_token(&self, line: u32, col: u32) -> Option<TokenMatch<'_>> {
        self.sm
            .lookup_token(line, col)
            .map(|tok| self.make_token_match(tok))
    }

    /// Returns a token for a specific index.
    pub fn get_token(&self, idx: u32) -> Option<TokenMatch<'_>> {
        self.sm.get_token(idx).map(|tok| self.make_token_match(tok))
    }

    /// Returns the number of tokens.
    pub fn get_token_count(&self) -> u32 {
        self.sm.get_token_count()
    }

    /// Returns a source view for the given source.
    pub fn get_source_view(&self, idx: u32) -> Option<&SourceView<'_>> {
        self.sm
            .get_source_view(idx)
            .map(|s| unsafe { &*(s as *const _ as *const SourceView<'_>) })
    }

    /// Returns the source name for an index.
    pub fn get_source_name(&self, idx: u32) -> Option<&str> {
        self.sm.get_source(idx)
    }

    /// Returns the number of sources.
    pub fn get_source_count(&self) -> u32 {
        self.sm.get_source_count()
    }

    /// Looks up a token and the original function name.
    ///
    /// This is similar to `lookup_token` but if a minified function name and
    /// the sourceview to the minified source is available this function will
    /// also resolve the original function name.  This is used to fully
    /// resolve tracebacks.
    pub fn lookup_token_with_function_name<'a, 'b>(
        &'a self,
        line: u32,
        col: u32,
        minified_name: &str,
        source: &SourceView<'b>,
    ) -> Option<TokenMatch<'a>> {
        match &self.sm {
            // Instead of regular line/column pairs, Hermes uses bytecode offsets, which always
            // have `line == 0`.
            // However, a `SourceMapHermes` is defined by having `x_facebook_sources` scope
            // information, which can actually be used without Hermes.
            // So if our stack frame has `line > 0` (0-based), it is extremely likely we don’t run
            // on hermes at all, in which case just fall back to regular sourcemap logic.
            // Luckily, `metro` puts a prelude on line 0,
            // so regular non-hermes user code should always have `line > 0`.
            SourceMapType::Hermes(smh) if line == 0 => {
                // we use `col + 1` here, since hermes uses bytecode offsets which are 0-based,
                // and the upstream python code does a `- 1` here:
                // https://github.com/getsentry/sentry/blob/fdabccac7576c80674c2fed556d4c5407657dc4c/src/sentry/lang/javascript/processor.py#L584-L586
                smh.lookup_token(line, col + 1).map(|token| {
                    let mut rv = self.make_token_match(token);
                    rv.function_name = smh.get_original_function_name(col + 1).map(str::to_owned);
                    rv
                })
            }
            _ => self.sm.lookup_token(line, col).map(|token| {
                let mut rv = self.make_token_match(token);
                rv.function_name = source
                    .sv
                    .get_original_function_name(token, minified_name)
                    .map(str::to_owned);
                rv
            }),
        }
    }

    fn make_token_match<'a>(&'a self, tok: sourcemap::Token<'a>) -> TokenMatch<'a> {
        TokenMatch {
            src_line: tok.get_src_line(),
            src_col: tok.get_src_col(),
            dst_line: tok.get_dst_line(),
            dst_col: tok.get_dst_col(),
            src_id: tok.get_src_id(),
            name: tok.get_name(),
            src: tok.get_source(),
            function_name: None,
        }
    }
}

#[test]
fn test_react_native_hermes() {
    let bytes = include_bytes!("../tests/fixtures/react-native-hermes.map");
    let smv = SourceMapView::from_json_slice(bytes).unwrap();
    let sv = SourceView::new("");

    //    at foo (address at unknown:1:11939)
    assert_eq!(
        smv.lookup_token_with_function_name(0, 11939, "", &sv),
        Some(TokenMatch {
            src_line: 1,
            src_col: 10,
            dst_line: 0,
            dst_col: 11939,
            src_id: 5,
            name: None,
            src: Some("module.js"),
            function_name: Some("foo".into())
        })
    );

    //    at anonymous (address at unknown:1:11857)
    assert_eq!(
        smv.lookup_token_with_function_name(0, 11857, "", &sv),
        Some(TokenMatch {
            src_line: 2,
            src_col: 0,
            dst_line: 0,
            dst_col: 11857,
            src_id: 4,
            name: None,
            src: Some("input.js"),
            function_name: Some("<global>".into())
        })
    );
}

#[test]
fn test_react_native_metro() {
    let source = include_str!("../tests/fixtures/react-native-metro.js");
    let bytes = include_bytes!("../tests/fixtures/react-native-metro.js.map");
    let smv = SourceMapView::from_json_slice(bytes).unwrap();
    let sv = SourceView::new(source);

    //    e.foo (react-native-metro.js:7:101)
    assert_eq!(
        smv.lookup_token_with_function_name(6, 100, "e.foo", &sv),
        Some(TokenMatch {
            src_line: 1,
            src_col: 10,
            dst_line: 6,
            dst_col: 100,
            src_id: 6,
            name: None,
            src: Some("module.js"),
            function_name: None,
        })
    );

    //    at react-native-metro.js:6:44
    assert_eq!(
        smv.lookup_token_with_function_name(5, 43, "", &sv),
        Some(TokenMatch {
            src_line: 2,
            src_col: 0,
            dst_line: 5,
            dst_col: 39,
            src_id: 5,
            name: Some("foo"),
            src: Some("input.js"),
            function_name: None,
        })
    );

    // in case we have a `metro` bundle, but a `hermes` bytecode offset (something out of range),
    // we can’t resolve this.
    assert_eq!(smv.lookup_token_with_function_name(0, 11857, "", &sv), None);
}
