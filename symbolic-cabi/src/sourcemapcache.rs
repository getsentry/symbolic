use std::ops::Deref;
use std::os::raw::c_char;
use std::ptr;
use std::slice;

use crate::utils::ForeignObject;
use crate::SymbolicStr;

use symbolic::common::AsSelf;
use symbolic::common::ByteView;
use symbolic::common::SelfCell;
use symbolic::sourcemapcache::SourceLocation;
use symbolic::sourcemapcache::{
    ScopeLookupResult, SourceMapCache, SourceMapCacheWriter, SourcePosition,
};

struct Inner<'a> {
    cache: SourceMapCache<'a>,
}

impl<'slf, 'a: 'slf> AsSelf<'slf> for Inner<'a> {
    type Ref = Inner<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
    }
}

pub struct OwnedSourceMapCache<'a> {
    inner: SelfCell<ByteView<'a>, Inner<'a>>,
}

/// Represents an sourcemapcache.
pub struct SymbolicSourceMapCache;

impl ForeignObject for SymbolicSourceMapCache {
    type RustObject = OwnedSourceMapCache<'static>;
}

#[repr(C)]
pub struct SymbolicStrVec {
    pub strs: *mut SymbolicStr,
    pub len: usize,
}

impl SymbolicStrVec {
    pub fn from_vec(vec: Vec<&str>) -> Self {
        let mut strs: Vec<_> = vec.into_iter().map(SymbolicStr::new).collect();
        strs.shrink_to_fit();
        let rv = SymbolicStrVec {
            strs: strs.as_mut_ptr(),
            len: strs.len(),
        };
        std::mem::forget(strs);
        rv
    }
}

/// Represents a single token after lookup.
#[repr(C)]
pub struct SymbolicSmTokenMatch {
    /// The line number in the original source file.
    pub line: u32,
    /// The column number in the original source file.
    pub col: u32,
    /// The path to the original source.
    pub src: SymbolicStr,
    /// The name of the source location as it is defined in the SourceMap.
    pub name: SymbolicStr,
    /// The name of the function containing the token.
    pub function_name: SymbolicStr,

    pub pre_context: SymbolicStrVec,
    pub context_line: SymbolicStr,
    pub post_context: SymbolicStrVec,
}

ffi_fn! {
    /// Creates an sourcemapcache from a given minified source and sourcemap contents.
    ///
    /// This shares the underlying memory and does not copy it if that is
    /// possible.  Will ignore utf-8 decoding errors.
    unsafe fn symbolic_sourcemapcache_from_bytes(
        source_content: *const c_char,
        source_len: usize,
        sourcemap_content: *const c_char,
        sourcemap_len: usize,
    ) -> Result<*mut SymbolicSourceMapCache> {
        let source_slice = slice::from_raw_parts(source_content as *const _, source_len);
        let sourcemap_slice = slice::from_raw_parts(sourcemap_content as *const _, sourcemap_len);

        let writer = SourceMapCacheWriter::new(
            String::from_utf8_lossy(source_slice).deref(),
            String::from_utf8_lossy(sourcemap_slice).deref()
        )?;
        let mut buffer = Vec::new();
        writer.serialize(&mut buffer)?;

        let byteview = ByteView::from_vec(buffer);
        let inner = SelfCell::try_new::<symbolic::sourcemapcache::SourceMapCacheError, _>(byteview, |data| {
            let cache = SourceMapCache::parse(&*data)?;
            Ok(Inner { cache })
        })?;

        let cache = OwnedSourceMapCache { inner };
        Ok(SymbolicSourceMapCache::from_rust(cache))
    }
}

ffi_fn! {
    /// Frees an SourceMapCache.
    unsafe fn symbolic_sourcemapcache_free(view: *mut SymbolicSourceMapCache) {
        SymbolicSourceMapCache::drop(view);
    }
}

fn make_token_match(token: SourceLocation, context_lines: u32) -> *mut SymbolicSmTokenMatch {
    let function_name = match token.scope() {
        ScopeLookupResult::NamedScope(name) => name,
        ScopeLookupResult::AnonymousScope => "<anonymous>",
        ScopeLookupResult::Unknown => "<unknown>",
    };

    let context_line = token.line_contents().unwrap_or_default();
    let context_line = SymbolicStr::new(context_line);

    let (pre_context, post_context) = if let Some(file) = token.file() {
        let current_line = token.line();

        let pre_line = current_line.saturating_sub(context_lines);
        let pre_context: Vec<_> = (pre_line..current_line)
            .filter_map(|line| file.line(line as usize))
            .collect();
        let pre_context = SymbolicStrVec::from_vec(pre_context);

        let post_line = current_line.saturating_add(context_lines);
        let post_context: Vec<_> = (current_line + 1..=post_line)
            .filter_map(|line| file.line(line as usize))
            .collect();

        let post_context = SymbolicStrVec::from_vec(post_context);

        (pre_context, post_context)
    } else {
        (
            SymbolicStrVec {
                strs: ptr::null_mut(),
                len: 0,
            },
            SymbolicStrVec {
                strs: ptr::null_mut(),
                len: 0,
            },
        )
    };

    Box::into_raw(Box::new(SymbolicSmTokenMatch {
        line: token.line() + 1,
        col: token.column() + 1,
        src: SymbolicStr::new(token.file_name().unwrap_or_default()),
        name: SymbolicStr::new(token.name().unwrap_or_default()),
        function_name: SymbolicStr::new(function_name),

        pre_context,
        context_line,
        post_context,
    }))
}

ffi_fn! {
    /// Looks up a token.
    unsafe fn symbolic_sourcemapcache_lookup_token(
        source_map: *const SymbolicSourceMapCache,
        line: u32,
        col: u32,
        context_lines: u32,
    ) -> Result<*mut SymbolicSmTokenMatch> {
        // Sentry JS events are 1-indexed, where SourcePosition is using 0-indexed locations
        let token_match = SymbolicSourceMapCache::as_rust(source_map)
            .inner
            .get()
            .cache
            .lookup(SourcePosition::new(line - 1, col - 1))
            .map(|sp| make_token_match(sp, context_lines))
            .unwrap_or_else(ptr::null_mut);
        Ok(token_match)
    }
}

ffi_fn! {
    /// Free a token match.
    unsafe fn symbolic_sourcemapcache_token_match_free(token_match: *mut SymbolicSmTokenMatch) {
        if !token_match.is_null() {
            let boxed_match = Box::from_raw(token_match);

            Vec::from_raw_parts(boxed_match.pre_context.strs, boxed_match.pre_context.len, boxed_match.pre_context.len);
            Vec::from_raw_parts(boxed_match.post_context.strs, boxed_match.post_context.len, boxed_match.post_context.len);
        }
    }
}
