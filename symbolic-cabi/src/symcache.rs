use std::ffi::CStr;
use std::io::Cursor;
use std::mem;
use std::os::raw::c_char;
use std::slice;

use symbolic::common::{ByteView, InstructionInfo, SelfCell};
use symbolic::symcache::{SymCache, SymCacheConverter, SYMCACHE_VERSION};

use crate::core::SymbolicStr;
use crate::debuginfo::SymbolicObject;
use crate::utils::ForeignObject;

/// Represents a symbolic sym cache.
pub struct SymbolicSymCache;

impl ForeignObject for SymbolicSymCache {
    type RustObject = SelfCell<ByteView<'static>, SymCache<'static>>;
}

/// Represents a single symbol after lookup.
#[repr(C)]
pub struct SymbolicSourceLocation {
    pub sym_addr: u64,
    pub instr_addr: u64,
    pub line: u32,
    pub lang: SymbolicStr,
    pub symbol: SymbolicStr,
    pub full_path: SymbolicStr,
}

/// Represents a lookup result of one or more items.
#[repr(C)]
pub struct SymbolicLookupResult {
    pub items: *mut SymbolicSourceLocation,
    pub len: usize,
}

/// Represents an instruction info.
#[repr(C)]
pub struct SymbolicInstructionInfo {
    /// The address of the instruction we want to use as a base.
    pub addr: u64,
    /// The architecture we are dealing with.
    pub arch: *const SymbolicStr,
    /// This is true if the frame is the cause of the crash.
    pub crashing_frame: bool,
    /// If a signal is know that triggers the crash, it can be stored here (0 if unknown)
    pub signal: u32,
    /// The optional value of the IP register (0 if unknown).
    pub ip_reg: u64,
}

ffi_fn! {
    /// Creates a symcache from a given path.
    unsafe fn symbolic_symcache_open(path: *const c_char) -> Result<*mut SymbolicSymCache> {
        let byteview = ByteView::open(CStr::from_ptr(path).to_str()?)?;
        let cell = SelfCell::try_new(byteview, |p| SymCache::parse(&*p))?;
        Ok(SymbolicSymCache::from_rust(cell))
    }
}

ffi_fn! {
    /// Creates a symcache from a byte buffer without taking ownership of the pointer.
    unsafe fn symbolic_symcache_from_bytes(
        bytes: *const u8,
        len: usize,
    ) -> Result<*mut SymbolicSymCache> {
        let byteview = ByteView::from_slice(slice::from_raw_parts(bytes, len));
        let cell = SelfCell::try_new(byteview, |p| SymCache::parse(&*p))?;
        Ok(SymbolicSymCache::from_rust(cell))
    }
}

ffi_fn! {
    /// Creates a symcache from a given object.
    unsafe fn symbolic_symcache_from_object(
        object: *const SymbolicObject,
    ) -> Result<*mut SymbolicSymCache> {
        let object = SymbolicObject::as_rust(object).get();

        let mut buffer = Vec::new();
        let mut converter = SymCacheConverter::new();
        converter.process_object(object)?;
        converter.serialize(&mut Cursor::new(&mut buffer))?;

        let byteview = ByteView::from_vec(buffer);
        let cell = SelfCell::try_new(byteview, |p| SymCache::parse(&*p))?;
        Ok(SymbolicSymCache::from_rust(cell))
    }
}

ffi_fn! {
    /// Frees a symcache object.
    unsafe fn symbolic_symcache_free(symcache: *mut SymbolicSymCache) {
        SymbolicSymCache::drop(symcache);
    }
}

ffi_fn! {
    /// Returns the internal buffer of the symcache.
    ///
    /// The internal buffer is exactly `symbolic_symcache_get_size` bytes long.
    unsafe fn symbolic_symcache_get_bytes(symcache: *const SymbolicSymCache) -> Result<*const u8> {
        Ok(SymbolicSymCache::as_rust(symcache).owner().as_slice().as_ptr())
    }
}

ffi_fn! {
    /// Returns the size in bytes of the symcache.
    unsafe fn symbolic_symcache_get_size(symcache: *const SymbolicSymCache) -> Result<usize> {
        Ok(SymbolicSymCache::as_rust(symcache).owner().len())
    }
}

ffi_fn! {
    /// Returns the architecture of the symcache.
    unsafe fn symbolic_symcache_get_arch(symcache: *const SymbolicSymCache) -> Result<SymbolicStr> {
        Ok(SymbolicSymCache::as_rust(symcache).get().arch().name().into())
    }
}

ffi_fn! {
    /// Returns the architecture of the symcache.
    unsafe fn symbolic_symcache_get_debug_id(symcache: *const SymbolicSymCache) -> Result<SymbolicStr> {
        Ok(SymbolicSymCache::as_rust(symcache).get().debug_id().to_string().into())
    }
}

ffi_fn! {
    /// Returns the version of the cache file.
    unsafe fn symbolic_symcache_get_version(symcache: *const SymbolicSymCache) -> Result<u32> {
        Ok(SymbolicSymCache::as_rust(symcache).get().version())
    }
}

ffi_fn! {
    /// Looks up a single symbol.
    #[allow(deprecated)]
    unsafe fn symbolic_symcache_lookup(
        symcache: *const SymbolicSymCache,
        addr: u64,
    ) -> Result<SymbolicLookupResult> {
        let cache = SymbolicSymCache::as_rust(symcache).get();

        let mut items = vec![];
        for source_location in cache.lookup(addr) {
            let full_path = source_location.file().map(|file| file.full_path()).unwrap_or_default();
            items.push(SymbolicSourceLocation {
                sym_addr: source_location.function().entry_pc() as u64,
                instr_addr: addr,
                line: source_location.line(),
                lang: SymbolicStr::new(source_location.function().language().name()),
                symbol: SymbolicStr::new(source_location.function().name()),
                full_path: SymbolicStr::from_string(full_path),
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
    unsafe fn symbolic_lookup_result_free(lookup_result: *mut SymbolicLookupResult) {
        if !lookup_result.is_null() {
            let result = &*lookup_result;
            Vec::from_raw_parts(result.items, result.len, result.len);
        }
    }
}

ffi_fn! {
    /// Return the best instruction for an isntruction info.
    unsafe fn symbolic_find_best_instruction(ii: *const SymbolicInstructionInfo) -> Result<u64> {
        let info = &*ii;

        let arch = (*info.arch).as_str().parse()?;
        let address = InstructionInfo::new(arch, info.addr)
            .is_crashing_frame(info.crashing_frame)
            .signal(Some(info.signal).filter(|&s| s != 0))
            .ip_register_value(Some(info.ip_reg).filter(|&r| r != 0))
            .caller_address();

        Ok(address)
    }
}

ffi_fn! {
    /// Returns the latest symcache version.
    unsafe fn symbolic_symcache_latest_version() -> Result<u32> {
        Ok(SYMCACHE_VERSION)
    }
}
