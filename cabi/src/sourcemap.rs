use std::ptr;
use std::mem;
use std::slice;
use std::os::raw::c_char;

use symbolic_common::Result;
use symbolic_sourcemap::{SourceMapView, SourceView, TokenMatch};

use core::SymbolicStr;

/// Represents a source view
pub struct SymbolicSourceView;

/// Represents a sourcemap view
pub struct SymbolicSourceMapView;

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
        let sv = SourceView::from_bytes(slice::from_raw_parts(bytes as *const _, len));
        Ok(Box::into_raw(Box::new(sv)) as *mut SymbolicSourceView)
    }
}

ffi_fn! {
    /// Frees a source view.
    unsafe fn symbolic_sourceview_free(ssv: *mut SymbolicSourceView) {
        if !ssv.is_null() {
            let sv = ssv as *mut SourceView<'static>;
            Box::from_raw(sv);
        }
    }
}

ffi_fn! {
    /// Returns the underlying source (borrowed).
    unsafe fn symbolic_sourceview_as_str(ssv: *const SymbolicSourceView) -> Result<SymbolicStr> {
        let sv = ssv as *mut SourceView<'static>;
        Ok(SymbolicStr::new((*sv).as_str()))
    }
}

ffi_fn! {
    /// Returns a specific line.
    unsafe fn symbolic_sourceview_get_line(
        ssv: *const SymbolicSourceView,
        idx: u32
    ) -> Result<SymbolicStr> {
        let sv = ssv as *mut SourceView<'static>;
        let line = (*sv).get_line(idx).unwrap_or("");
        Ok(SymbolicStr::new(line))
    }
}

ffi_fn! {
    /// Returns the number of lines.
    unsafe fn symbolic_sourceview_get_line_count(ssv: *const SymbolicSourceView) -> Result<u32> {
        let sv = ssv as *mut SourceView<'static>;
        Ok((*sv).line_count() as u32)
    }
}

ffi_fn! {
    /// Loads a sourcemap from a JSON byte slice.
    unsafe fn symbolic_sourcemapview_from_json_slice(
        data: *const c_char,
        len: usize
    ) -> Result<*mut SymbolicSourceMapView> {
        let bytes = slice::from_raw_parts(data as *const _, len);
        let sm = SourceMapView::from_json_slice(bytes)?;
        Ok(Box::into_raw(Box::new(sm)) as *mut SymbolicSourceMapView)
    }
}

ffi_fn! {
    /// Frees a source map view
    unsafe fn symbolic_sourcemapview_free(smv: *const SymbolicSourceMapView) {
        if !smv.is_null() {
            let sm = smv as *mut SourceMapView;
            Box::from_raw(sm);
        }
    }
}

fn convert_token_match(token: Option<TokenMatch>) -> Result<*mut SymbolicTokenMatch> {
    Ok(token
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
                    .map(|name| SymbolicStr::from_string(name))
                    .unwrap_or(Default::default()),
            }))
        })
        .unwrap_or(ptr::null_mut()))
}

ffi_fn! {
    /// Looks up a token.
    unsafe fn symbolic_sourcemapview_lookup_token(
        ssm: *const SymbolicSourceMapView,
        line: u32,
        col: u32,
    ) -> Result<*mut SymbolicTokenMatch> {
        let sm = ssm as *const SourceMapView;
        convert_token_match((*sm).lookup_token(line, col))
    }
}

ffi_fn! {
    /// Looks up a token.
    unsafe fn symbolic_sourcemapview_lookup_token_with_function_name(
        ssm: *const SymbolicSourceMapView,
        line: u32, col: u32,
        minified_name: *const SymbolicStr,
        ssv: *const SymbolicSourceView,
    ) -> Result<*mut SymbolicTokenMatch> {
        let sm = ssm as *const SourceMapView;
        let sv = ssv as *const SourceView<'static>;
        convert_token_match((*sm).lookup_token_with_function_name(
            line, col, (*minified_name).as_str(), mem::transmute(sv)))
    }
}

ffi_fn! {
    /// Return the sourceview for a given source.
    unsafe fn symbolic_sourcemapview_get_sourceview(
        ssm: *const SymbolicSourceMapView,
        index: u32
    ) -> Result<*const SymbolicSourceView> {
        let sm = ssm as *const SourceMapView;
        Ok((*sm)
           .get_source_view(index)
           .map(|x| mem::transmute(x))
           .unwrap_or(ptr::null()))
    }
}

ffi_fn! {
    /// Return the source name for an index.
    unsafe fn symbolic_sourcemapview_get_source_name(
        ssm: *const SymbolicSourceMapView,
        index: u32
    ) -> Result<SymbolicStr> {
        let sm = ssm as *const SourceMapView;
        Ok(SymbolicStr::new((*sm)
           .get_source_name(index)
           .unwrap_or("")))
    }
}

ffi_fn! {
    /// Return the number of sources.
    unsafe fn symbolic_sourcemapview_get_source_count(
        ssm: *const SymbolicSourceMapView
    ) -> Result<u32> {
        let sm = ssm as *const SourceMapView;
        Ok((*sm).get_source_count())
    }
}

ffi_fn! {
    /// Returns a specific token.
    unsafe fn symbolic_sourcemapview_get_token(
        ssm: *const SymbolicSourceMapView,
        idx: u32
    ) -> Result<*mut SymbolicTokenMatch> {
        let sm = ssm as *const SourceMapView;
        convert_token_match((*sm).get_token(idx))
    }
}

ffi_fn! {
    /// Returns the number of tokens.
    unsafe fn symbolic_sourcemapview_get_tokens(ssm: *const SymbolicSourceMapView) -> Result<u32> {
        let sm = ssm as *const SourceMapView;
        Ok((*sm).get_token_count())
    }
}

ffi_fn! {
    /// Free a token match
    unsafe fn symbolic_token_match_free(stm: *mut SymbolicTokenMatch) {
        if !stm.is_null() {
            let tm = stm as *mut SymbolicTokenMatch;
            (*tm).name.free();
            (*tm).src.free();
            (*tm).function_name.free();
            Box::from_raw(tm);
        }
    }
}
