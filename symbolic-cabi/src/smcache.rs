use std::ops::Deref;
use std::os::raw::c_char;
use std::ptr;
use std::slice;

use crate::utils::ForeignObject;
use crate::SymbolicStr;

use symbolic::common::AsSelf;
use symbolic::common::ByteView;
use symbolic::common::SelfCell;
use symbolic::smcache::SourceLocation;
use symbolic::smcache::{ScopeLookupResult, SmCache, SmCacheWriter, SourcePosition};

struct Inner<'a> {
    cache: SmCache<'a>,
}

impl<'slf, 'a: 'slf> AsSelf<'slf> for Inner<'a> {
    type Ref = Inner<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
    }
}

pub struct OwnedSmCache<'a> {
    inner: SelfCell<ByteView<'a>, Inner<'a>>,
}

/// Represents an smcache.
pub struct SymbolicSmCache;

impl ForeignObject for SymbolicSmCache {
    type RustObject = OwnedSmCache<'static>;
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
    /// The name of the function containing the token.
    pub function_name: SymbolicStr,

    pub pre_context: SymbolicStrVec,
    pub context: SymbolicStr,
    pub post_context: SymbolicStrVec,
}

ffi_fn! {
    /// Creates an smcache from a given minified source and sourcemap contents.
    ///
    /// This shares the underlying memory and does not copy it if that is
    /// possible.  Will ignore utf-8 decoding errors.
    unsafe fn symbolic_smcache_from_bytes(
        source_content: *const c_char,
        source_len: usize,
        sourcemap_content: *const c_char,
        sourcemap_len: usize,
    ) -> Result<*mut SymbolicSmCache> {
        let source_slice = slice::from_raw_parts(source_content as *const _, source_len);
        let sourcemap_slice = slice::from_raw_parts(sourcemap_content as *const _, sourcemap_len);

        let writer = SmCacheWriter::new(
            String::from_utf8_lossy(source_slice).deref(),
            String::from_utf8_lossy(sourcemap_slice).deref()
        )?;
        let mut buffer = Vec::new();
        writer.serialize(&mut buffer)?;

        let byteview = ByteView::from_vec(buffer);
        let inner = SelfCell::try_new::<symbolic::smcache::SmCacheError, _>(byteview, |data| {
            let cache = SmCache::parse(&*data)?;
            Ok(Inner { cache })
        })?;

        let cache = OwnedSmCache { inner };
        Ok(SymbolicSmCache::from_rust(cache))
    }
}

ffi_fn! {
    /// Frees an SmCache.
    unsafe fn symbolic_smcache_free(view: *mut SymbolicSmCache) {
        SymbolicSmCache::drop(view);
    }
}

fn make_token_match(token: SourceLocation, context_lines: u32) -> *mut SymbolicSmTokenMatch {
    let function_name = match token.scope() {
        ScopeLookupResult::NamedScope(name) => name,
        ScopeLookupResult::AnonymousScope => "<anonymous>",
        ScopeLookupResult::Unknown => "<unknown>",
    };

    let context = token.line_contents().unwrap_or_default();
    let context = SymbolicStr::new(context);

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
        // TODO: Discuss how we should handle column numbers
        // Currently in Sentry they are used and displayed to the user,
        // however it's not clear whether they provide any value.
        // NOTE: Possibly used in VSCode integrations, that opens file in your local editor
        // when configured or something of that sort.
        col: 0,
        function_name: SymbolicStr::new(function_name),

        pre_context,
        context,
        post_context,
    }))
}

ffi_fn! {
    /// Looks up a token.
    unsafe fn symbolic_smcache_lookup_token(
        source_map: *const SymbolicSmCache,
        line: u32,
        col: u32,
        context_lines: u32,
    ) -> Result<*mut SymbolicSmTokenMatch> {
        // Sentry JS events are 1-indexed, where SourcePosition is using 0-indexed locations
        let token_match = SymbolicSmCache::as_rust(source_map)
            .inner
            .get()
            .cache
            .lookup(SourcePosition::new(line - 1, col - 1))
            .map(|sp|make_token_match(sp, context_lines))
            .unwrap_or_else(ptr::null_mut);
        Ok(token_match)
    }
}

ffi_fn! {
    /// Free a token match.
    unsafe fn symbolic_smcache_token_match_free(token_match: *mut SymbolicSmTokenMatch) {
        if !token_match.is_null() {
            let boxed_match = Box::from_raw(token_match);

            Vec::from_raw_parts(boxed_match.pre_context.strs, boxed_match.pre_context.len, boxed_match.pre_context.len);
            Vec::from_raw_parts(boxed_match.post_context.strs, boxed_match.post_context.len, boxed_match.post_context.len);
        }
    }
}
