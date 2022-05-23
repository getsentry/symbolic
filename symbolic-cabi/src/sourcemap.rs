use std::borrow::Cow;
use std::ops::Deref;
use std::os::raw::c_char;
use std::ptr;
use std::slice;

use crate::core::SymbolicStr;
use crate::utils::ForeignObject;

/// Represents a source view.
pub struct SymbolicSourceView;

impl ForeignObject for SymbolicSourceView {
    type RustObject = sourcemap::SourceView<'static>;
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

pub struct SourceMapView {
    inner: SourceMapType,
}

/// Represents a sourcemap view.
pub struct SymbolicSourceMapView;

impl ForeignObject for SymbolicSourceMapView {
    type RustObject = SourceMapView;
}

/// Represents a single token after lookup.
#[repr(C)]
pub struct SymbolicTokenMatch {
    /// The line number in the original source file.
    pub src_line: u32,
    /// The column number in the original source file.
    pub src_col: u32,
    /// The line number in the minifid source file.
    pub dst_line: u32,
    /// The column number in the minified source file.
    pub dst_col: u32,
    /// The source ID of the token.
    pub src_id: u32,
    /// The token name, if present.
    pub name: SymbolicStr,
    /// The source.
    pub src: SymbolicStr,
    /// The name of the function containing the token.
    pub function_name: SymbolicStr,
}

ffi_fn! {
    /// Creates a source view from a given path.
    ///
    /// This shares the underlying memory and does not copy it if that is
    /// possible.  Will ignore utf-8 decoding errors.
    unsafe fn symbolic_sourceview_from_bytes(
        bytes: *const c_char,
        len: usize,
    ) -> Result<*mut SymbolicSourceView> {
        let slice = slice::from_raw_parts(bytes as *const _, len);
        let view = match String::from_utf8_lossy(slice) {
            Cow::Owned(s) => sourcemap::SourceView::from_string(s),
            Cow::Borrowed(s) => sourcemap::SourceView::new(s),
        };
        Ok(SymbolicSourceView::from_rust(view))
    }
}

ffi_fn! {
    /// Frees a source view.
    unsafe fn symbolic_sourceview_free(view: *mut SymbolicSourceView) {
        SymbolicSourceView::drop(view);
    }
}

ffi_fn! {
    /// Returns the underlying source (borrowed).
    unsafe fn symbolic_sourceview_as_str(view: *const SymbolicSourceView) -> Result<SymbolicStr> {
        Ok(SymbolicSourceView::as_rust(view).source().into())
    }
}

ffi_fn! {
    /// Returns a specific line.
    unsafe fn symbolic_sourceview_get_line(
        view: *const SymbolicSourceView,
        index: u32
    ) -> Result<SymbolicStr> {
        Ok(SymbolicSourceView::as_rust(view).get_line(index).unwrap_or("").into())
    }
}

ffi_fn! {
    /// Returns the number of lines.
    unsafe fn symbolic_sourceview_get_line_count(
        source_map: *const SymbolicSourceView
    ) -> Result<u32> {
        Ok(SymbolicSourceView::as_rust(source_map).line_count() as u32)
    }
}

ffi_fn! {
    /// Loads a sourcemap from a JSON byte slice.
    unsafe fn symbolic_sourcemapview_from_json_slice(
        data: *const c_char,
        len: usize
    ) -> Result<*mut SymbolicSourceMapView> {
        let slice = slice::from_raw_parts(data as *const _, len);
        let inner = match sourcemap::decode_slice(slice)? {
                sourcemap::DecodedMap::Regular(sm) => SourceMapType::Regular(sm),
                sourcemap::DecodedMap::Index(smi) => SourceMapType::Regular(smi.flatten()?),
                sourcemap::DecodedMap::Hermes(smh) => SourceMapType::Hermes(smh),
        };
        let view = SourceMapView {inner};
        Ok(SymbolicSourceMapView::from_rust(view))
    }
}

ffi_fn! {
    /// Frees a source map view.
    unsafe fn symbolic_sourcemapview_free(source_map: *mut SymbolicSourceMapView) {
        SymbolicSourceMapView::drop(source_map);
    }
}

fn make_token_match(token: sourcemap::Token<'_>) -> *mut SymbolicTokenMatch {
    Box::into_raw(Box::new(SymbolicTokenMatch {
        src_line: token.get_src_line(),
        src_col: token.get_src_col(),
        dst_line: token.get_dst_line(),
        dst_col: token.get_dst_col(),
        src_id: token.get_src_id(),
        name: SymbolicStr::new(token.get_name().unwrap_or_default()),
        src: SymbolicStr::new(token.get_source().unwrap_or_default()),
        function_name: SymbolicStr::default(),
    }))
}

ffi_fn! {
    /// Looks up a token.
    unsafe fn symbolic_sourcemapview_lookup_token(
        source_map: *const SymbolicSourceMapView,
        line: u32,
        col: u32,
    ) -> Result<*mut SymbolicTokenMatch> {
        let token_match = SymbolicSourceMapView::as_rust(source_map)
            .inner
            .lookup_token(line, col)
            .map(make_token_match)
            .unwrap_or_else(ptr::null_mut);
        Ok(token_match)
    }
}

ffi_fn! {
    /// Looks up a token and the original function name.
    ///
    /// This is similar to `lookup_token` but if a minified function name and
    /// the sourceview to the minified source is available this function will
    /// also resolve the original function name.  This is used to fully
    /// resolve tracebacks.
    unsafe fn symbolic_sourcemapview_lookup_token_with_function_name(
        source_map: *const SymbolicSourceMapView,
        line: u32, col: u32,
        minified_name: *const SymbolicStr,
        source_view: *const SymbolicSourceView,
    ) -> Result<*mut SymbolicTokenMatch> {
        let source_map = SymbolicSourceMapView::as_rust(source_map) ;
        let source_view = SymbolicSourceView::as_rust(source_view);
        let token_match = match &source_map.inner {
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
                    let mut rv = make_token_match(token);
                    if let Some(name) = smh.get_original_function_name(col + 1).map(str::to_owned) {
                        (*rv).function_name = SymbolicStr::from_string(name);
                    }
                    rv
                })
            }
            _ => source_map.inner.lookup_token(line, col).map(|token| {
                let mut rv = make_token_match(token);
                if let Some(name) = source_view
                    .get_original_function_name(token, (*minified_name).as_str())
                    .map(str::to_owned) {
                        (*rv).function_name = SymbolicStr::from_string(name);
                }
                rv
            }),

        };

        Ok(token_match.unwrap_or_else(ptr::null_mut))
    }
}

ffi_fn! {
    /// Return the sourceview for a given source.
    unsafe fn symbolic_sourcemapview_get_sourceview(
        source_map: *const SymbolicSourceMapView,
        index: u32
    ) -> Result<*const SymbolicSourceView> {
        Ok(match SymbolicSourceMapView::as_rust(source_map).inner.get_source_view(index) {
            Some(view) => SymbolicSourceView::from_ref(view),
            None => ptr::null(),
        })
    }
}

ffi_fn! {
    /// Return the source name for an index.
    unsafe fn symbolic_sourcemapview_get_source_name(
        source_map: *const SymbolicSourceMapView,
        index: u32
    ) -> Result<SymbolicStr> {
        let view = SymbolicSourceMapView::as_rust(source_map);
        let name_opt = view.inner.get_source(index);
        Ok(name_opt.unwrap_or("").into())
    }
}

ffi_fn! {
    /// Return the number of sources.
    unsafe fn symbolic_sourcemapview_get_source_count(
        source_map: *const SymbolicSourceMapView
    ) -> Result<u32> {
        Ok(SymbolicSourceMapView::as_rust(source_map).inner.get_source_count())
    }
}

ffi_fn! {
    /// Returns a specific token.
    unsafe fn symbolic_sourcemapview_get_token(
        source_map: *const SymbolicSourceMapView,
        index: u32
    ) -> Result<*mut SymbolicTokenMatch> {
        let token = SymbolicSourceMapView::as_rust(source_map).inner.get_token(index);
        Ok(token.map(make_token_match).unwrap_or_else(ptr::null_mut))
    }
}

ffi_fn! {
    /// Returns the number of tokens.
    unsafe fn symbolic_sourcemapview_get_tokens(source_map: *const SymbolicSourceMapView) -> Result<u32> {
        Ok(SymbolicSourceMapView::as_rust(source_map).inner.get_token_count())
    }
}

ffi_fn! {
    /// Free a token match.
    unsafe fn symbolic_token_match_free(token_match: *mut SymbolicTokenMatch) {
        if !token_match.is_null() {
            Box::from_raw(token_match);
        }
    }
}
