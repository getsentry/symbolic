use std::ptr;

use symbolic_debuginfo::Object;
use symbolic_symcache::SymCache;

use debuginfo::SymbolicObject;

/// Represents a symbolic sym cache.
pub struct SymbolicSymCache;

/// The result of a symbol lookup.
pub struct SymbolicLookupResult;


ffi_fn! {
    /// Creates a symcache from a given object.
    unsafe fn symbolic_symcache_from_object(sobj: *const SymbolicObject)
        -> Result<*mut SymbolicSymCache>
    {
        let cache = SymCache::from_object(&*(sobj as *const Object))?;
        Ok(Box::into_raw(Box::new(cache)) as *mut SymbolicSymCache)
    }
}

ffi_fn! {
    /// Frees a symcache object.
    unsafe fn symbolic_symcache_free(scache: *mut SymbolicSymCache) {
        if !scache.is_null() {
            let cache = scache as *mut SymCache<'static>;
            Box::from_raw(cache);
        }
    }
}

ffi_fn! {
    /// Looks up a single symbol.
    unsafe fn symbolic_symcache_lookup(scache: *const SymbolicSymCache,
                                       addr: u64) -> Result<*mut SymbolicLookupResult> {
        let cache = scache as *const SymCache<'static>;
        let vec = (*cache).lookup(addr)?;
        if vec.is_empty() {
            Ok(ptr::null_mut())
        } else {
            Ok(Box::into_raw(Box::new(vec)) as *mut SymbolicLookupResult)
        }
    }
}
