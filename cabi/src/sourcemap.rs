use std::os::raw::c_char;
use std::ptr;
use std::slice;

use symbolic::sourcemap::{SourceMapView, SourceView, TokenMatch};

use crate::core::SymbolicStr;
use crate::utils::ForeignObject;

/// Represents a source view.
pub struct SymbolicSourceView;

impl ForeignObject for SymbolicSourceView {
    type RustObject = SourceView<'static>;
}

/// Represents a sourcemap view.
pub struct SymbolicSourceMapView;

impl ForeignObject for SymbolicSourceMapView {
    type RustObject = SourceMapView;
}

/// Represents a single token after lookup.
#[repr(C)]
pub struct SymbolicTokenMatch {
    pub src_line: u32,
    pub src_col: u32,
    pub dst_line: u32,
    pub dst_col: u32,
    pub src_id: u32,
    pub name: SymbolicStr,
    pub src: SymbolicStr,
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
        let view = SourceView::from_slice(slice);
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
        Ok(SymbolicSourceView::as_rust(view).as_str().into())
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
        let view = SourceMapView::from_json_slice(slice)?;
        Ok(SymbolicSourceMapView::from_rust(view))
    }
}

ffi_fn! {
    /// Frees a source map view.
    unsafe fn symbolic_sourcemapview_free(source_map: *mut SymbolicSourceMapView) {
        SymbolicSourceMapView::drop(source_map);
    }
}

fn convert_token_match(token: Option<TokenMatch<'_>>) -> *mut SymbolicTokenMatch {
    token
        .map(|token| {
            Box::into_raw(Box::new(SymbolicTokenMatch {
                src_line: token.src_line,
                src_col: token.src_col,
                dst_line: token.dst_line,
                dst_col: token.dst_col,
                src_id: token.src_id,
                name: SymbolicStr::new(token.name.unwrap_or("")),
                src: SymbolicStr::new(token.src.unwrap_or("")),
                function_name: token
                    .function_name
                    .map(SymbolicStr::from_string)
                    .unwrap_or_default(),
            }))
        })
        .unwrap_or(ptr::null_mut())
}

ffi_fn! {
    /// Looks up a token.
    unsafe fn symbolic_sourcemapview_lookup_token(
        source_map: *const SymbolicSourceMapView,
        line: u32,
        col: u32,
    ) -> Result<*mut SymbolicTokenMatch> {
        let token = SymbolicSourceMapView::as_rust(source_map).lookup_token(line, col);
        Ok(convert_token_match(token))
    }
}

ffi_fn! {
    /// Looks up a token.
    unsafe fn symbolic_sourcemapview_lookup_token_with_function_name(
        source_map: *const SymbolicSourceMapView,
        line: u32, col: u32,
        minified_name: *const SymbolicStr,
        view: *const SymbolicSourceView,
    ) -> Result<*mut SymbolicTokenMatch> {
        let source_view = SymbolicSourceView::as_rust(view);
        let token = SymbolicSourceMapView::as_rust(source_map)
            .lookup_token_with_function_name(line, col, (*minified_name).as_str(), source_view);
        Ok(convert_token_match(token))
    }
}

ffi_fn! {
    /// Return the sourceview for a given source.
    unsafe fn symbolic_sourcemapview_get_sourceview(
        source_map: *const SymbolicSourceMapView,
        index: u32
    ) -> Result<*const SymbolicSourceView> {
        Ok(match SymbolicSourceMapView::as_rust(source_map).get_source_view(index) {
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
        let name_opt = view.get_source_name(index);
        Ok(name_opt.unwrap_or("").into())
    }
}

ffi_fn! {
    /// Return the number of sources.
    unsafe fn symbolic_sourcemapview_get_source_count(
        source_map: *const SymbolicSourceMapView
    ) -> Result<u32> {
        Ok(SymbolicSourceMapView::as_rust(source_map).get_source_count())
    }
}

ffi_fn! {
    /// Returns a specific token.
    unsafe fn symbolic_sourcemapview_get_token(
        source_map: *const SymbolicSourceMapView,
        index: u32
    ) -> Result<*mut SymbolicTokenMatch> {
        let token = SymbolicSourceMapView::as_rust(source_map).get_token(index);
        Ok(convert_token_match(token))
    }
}

ffi_fn! {
    /// Returns the number of tokens.
    unsafe fn symbolic_sourcemapview_get_tokens(source_map: *const SymbolicSourceMapView) -> Result<u32> {
        Ok(SymbolicSourceMapView::as_rust(source_map).get_token_count())
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
