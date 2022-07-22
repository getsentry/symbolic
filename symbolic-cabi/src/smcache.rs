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

/// Represents a single token after lookup.
#[repr(C)]
pub struct SymbolicSmTokenMatch {
    /// The line number in the original source file.
    pub line: u32,
    /// The column number in the original source file.
    pub col: u32,
    /// The path to the original source.
    pub src: SymbolicStr,
    /// The name of the function containing the token.
    pub function_name: SymbolicStr,
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

ffi_fn! {
    unsafe fn symbolic_smcache_files(
        smcache: *const SymbolicSmCache
    ) -> Result<SymbolicStr> {
        let cache = &SymbolicSmCache::as_rust(smcache).inner.get().cache;
        let rv: Vec<String> = cache.files().map(|file| file.name().to_owned()).collect();
        Ok(SymbolicStr::new(&rv.join(" ")))
    }
}

fn make_token_match(token: SourceLocation) -> *mut SymbolicSmTokenMatch {
    let function_name = match token.scope() {
        ScopeLookupResult::NamedScope(name) => name,
        _ => {
            // TODO: Implement call-site function name extraction or other heuristics
            // to get function names for frames that token do not provide it.
            "<unknown>"
            // if let Some(prev_frame_name) = self.previous_frame_name.as_ref() {
            //     prev_frame_name
            // } else {
            //     "<unknown>"
            // }
        }
    };

    Box::into_raw(Box::new(SymbolicSmTokenMatch {
        line: token.line() + 1,
        // TODO: Discuss how we should handle column numbers
        // Currently in Sentry they are used and displayed to the user,
        // however it's not clear whether they provide any value.
        // NOTE: Possibly used in VSCode integrations, that opens file in your local editor
        // when configured or something of that sort.
        col: 0,
        src: SymbolicStr::new(token.file_name().unwrap_or_default()),
        function_name: SymbolicStr::new(function_name),
    }))
}

ffi_fn! {
    /// Looks up a token.
    unsafe fn symbolic_smcache_lookup_token(
        source_map: *const SymbolicSmCache,
        line: u32,
        col: u32,
    ) -> Result<*mut SymbolicSmTokenMatch> {
        // Sentry JS events are 1-indexed, where SourcePosition is using 0-indexed locations
        let token_match = SymbolicSmCache::as_rust(source_map)
            .inner
            .get()
            .cache
            .lookup(SourcePosition::new(line - 1, col - 1))
            .map(make_token_match)
            .unwrap_or_else(ptr::null_mut);
        Ok(token_match)
    }
}

ffi_fn! {
    /// Free a token match.
    unsafe fn symbolic_smcache_token_match_free(token_match: *mut SymbolicSmTokenMatch) {
        if !token_match.is_null() {
            Box::from_raw(token_match);
        }
    }
}
