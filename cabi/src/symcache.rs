use std::mem;

use symbolic_debuginfo::Object;
use symbolic_symcache::SymCache;

use core::SymbolicStr;
use debuginfo::SymbolicObject;

/// Represents a symbolic sym cache.
pub struct SymbolicSymCache;

/// Represents a single symbol after lookup.
#[repr(C)]
pub struct SymbolicSymbol {
    pub sym_addr: u64,
    pub instr_addr: u64,
    pub line: u32,
    pub symbol: SymbolicStr,
    pub filename: SymbolicStr,
    pub base_dir: SymbolicStr,
    pub comp_dir: SymbolicStr,
}

/// Represents a lookup result of one or more items.
#[repr(C)]
pub struct SymbolicLookupResult {
    pub items: *mut SymbolicSymbol,
    pub len: usize,
}


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
                                       addr: u64) -> Result<SymbolicLookupResult> {
        let cache = scache as *const SymCache<'static>;
        let vec = (*cache).lookup(addr)?;

        let mut items = vec![];
        for symbol in vec {
            items.push(SymbolicSymbol {
                sym_addr: symbol.sym_addr(),
                instr_addr: symbol.instr_addr(),
                line: symbol.line(),
                symbol: SymbolicStr::new(symbol.symbol()),
                filename: SymbolicStr::new(symbol.filename()),
                base_dir: SymbolicStr::new(symbol.base_dir()),
                comp_dir: SymbolicStr::new(symbol.comp_dir()),
            });
        }

        items.shrink_to_fit();
        let rv = SymbolicLookupResult {
            items: items.as_mut_ptr(),
            len: items.len(),
        };
        mem::forget(items);
        Ok(rv)
    }
}

ffi_fn! {
    /// Frees a lookup result.
    unsafe fn symbolic_lookup_result_free(slr: *mut SymbolicLookupResult) {
        if !slr.is_null() {
            Vec::from_raw_parts((*slr).items, (*slr).len, (*slr).len);
        }
    }
}
