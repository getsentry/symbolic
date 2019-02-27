use std::ffi::CStr;
use std::io::Cursor;
use std::mem;
use std::os::raw::c_char;
use std::slice;

use symbolic::common::{Arch, ByteView, InstructionInfo, SelfCell};
use symbolic::symcache::{format::SYMCACHE_VERSION, SymCache, SymCacheWriter};

use crate::core::SymbolicStr;
use crate::debuginfo::{ObjectCell, SymbolicObject};

type SymCacheCell = SelfCell<ByteView<'static>, SymCache<'static>>;

/// Represents a symbolic sym cache.
pub struct SymbolicSymCache;

/// Represents a single symbol after lookup.
#[repr(C)]
pub struct SymbolicLineInfo {
    pub sym_addr: u64,
    pub line_addr: u64,
    pub instr_addr: u64,
    pub line: u32,
    pub lang: SymbolicStr,
    pub symbol: SymbolicStr,
    pub filename: SymbolicStr,
    pub base_dir: SymbolicStr,
    pub comp_dir: SymbolicStr,
}

/// Represents a lookup result of one or more items.
#[repr(C)]
pub struct SymbolicLookupResult {
    pub items: *mut SymbolicLineInfo,
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
    unsafe fn symbolic_symcache_from_path(path: *const c_char) -> Result<*mut SymbolicSymCache> {
        let byteview = ByteView::open(CStr::from_ptr(path).to_str()?)?;
        let cell = SelfCell::try_new(byteview, |p| SymCache::parse(&*p))?;
        Ok(Box::into_raw(Box::new(cell)) as *mut SymbolicSymCache)
    }
}

ffi_fn! {
    /// Creates a symcache from a byte buffer WITHOUT taking ownership.
    unsafe fn symbolic_symcache_from_bytes(
        bytes: *const u8,
        len: usize,
    ) -> Result<*mut SymbolicSymCache> {
        let byteview = ByteView::from_slice(slice::from_raw_parts(bytes, len));
        let cell = SelfCell::try_new(byteview, |p| SymCache::parse(&*p))?;
        Ok(Box::into_raw(Box::new(cell)) as *mut SymbolicSymCache)
    }
}

ffi_fn! {
    /// Creates a symcache from a given object.
    unsafe fn symbolic_symcache_from_object(
        sobj: *const SymbolicObject,
    ) -> Result<*mut SymbolicSymCache> {
        let object = (*(sobj as *const ObjectCell)).get();

        let mut buffer = Vec::new();
        SymCacheWriter::write_object(object, Cursor::new(&mut buffer))?;

        let byteview = ByteView::from_vec(buffer);
        let cell = SelfCell::try_new(byteview, |p| SymCache::parse(&*p))?;
        Ok(Box::into_raw(Box::new(cell)) as *mut SymbolicSymCache)
    }
}

ffi_fn! {
    /// Frees a symcache object.
    unsafe fn symbolic_symcache_free(scache: *mut SymbolicSymCache) {
        if !scache.is_null() {
            let cache = scache as *mut SymCacheCell;
            Box::from_raw(cache);
        }
    }
}

ffi_fn! {
    /// Returns the internal buffer of the symcache.
    ///
    /// The internal buffer is exactly `symbolic_symcache_get_size` bytes long.
    unsafe fn symbolic_symcache_get_bytes(scache: *const SymbolicSymCache) -> Result<*const u8> {
        let cache = scache as *const SymCacheCell;
        Ok((*cache).owner().as_slice().as_ptr())
    }
}

ffi_fn! {
    /// Returns the size in bytes of the symcache.
    unsafe fn symbolic_symcache_get_size(scache: *const SymbolicSymCache) -> Result<usize> {
        let cache = scache as *const SymCacheCell;
        Ok((*cache).owner().len())
    }
}

ffi_fn! {
    /// Returns the architecture of the symcache.
    unsafe fn symbolic_symcache_get_arch(scache: *const SymbolicSymCache) -> Result<SymbolicStr> {
        let cache = scache as *const SymCacheCell;
        Ok(SymbolicStr::new((*cache).get().arch().name()))
    }
}

ffi_fn! {
    /// Returns the architecture of the symcache.
    unsafe fn symbolic_symcache_get_id(scache: *const SymbolicSymCache) -> Result<SymbolicStr> {
        let cache = scache as *const SymCacheCell;
        Ok((*cache).get().debug_id().to_string().into())
    }
}

ffi_fn! {
    /// Returns true if the symcache has line infos.
    unsafe fn symbolic_symcache_has_line_info(scache: *const SymbolicSymCache) -> Result<bool> {
        let cache = scache as *const SymCacheCell;
        Ok((*cache).get().has_line_info())
    }
}

ffi_fn! {
    /// Returns true if the symcache has file infos.
    unsafe fn symbolic_symcache_has_file_info(scache: *const SymbolicSymCache) -> Result<bool> {
        let cache = scache as *const SymCacheCell;
        Ok((*cache).get().has_file_info())
    }
}

ffi_fn! {
    /// Returns the version of the cache file.
    unsafe fn symbolic_symcache_get_version(scache: *const SymbolicSymCache) -> Result<u32> {
        let cache = scache as *const SymCacheCell;
        Ok((*cache).get().version())
    }
}

ffi_fn! {
    /// Looks up a single symbol.
    unsafe fn symbolic_symcache_lookup(
        scache: *const SymbolicSymCache,
        addr: u64,
    ) -> Result<SymbolicLookupResult> {
        let cache = scache as *const SymCacheCell;
        let lookup = (*cache).get().lookup(addr)?;

        let mut items = vec![];
        for line_info in lookup {
            let line_info = line_info?;
            items.push(SymbolicLineInfo {
                sym_addr: line_info.function_address(),
                line_addr: line_info.line_address(),
                instr_addr: line_info.instruction_address(),
                line: line_info.line(),
                lang: SymbolicStr::new(line_info.language().name()),
                symbol: SymbolicStr::new(line_info.symbol()),
                filename: SymbolicStr::new(line_info.filename()),
                base_dir: SymbolicStr::new(line_info.base_dir()),
                comp_dir: SymbolicStr::new(line_info.compilation_dir()),
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

ffi_fn! {
    /// Return the best instruction for an isntruction info.
    unsafe fn symbolic_find_best_instruction(ii: *const SymbolicInstructionInfo) -> Result<u64> {
        let real_ii = InstructionInfo {
            addr: (*ii).addr,
            arch: (*(*ii).arch).as_str().parse::<Arch>()?,
            crashing_frame: (*ii).crashing_frame,
            signal: if (*ii).signal == 0 { None } else { Some((*ii).signal) },
            ip_reg: if (*ii).ip_reg == 0 { None } else { Some((*ii).ip_reg) },
        };
        Ok(real_ii.caller_address())
    }
}

ffi_fn! {
    /// Returns the latest symcache version.
    unsafe fn symbolic_symcache_latest_file_format_version() -> Result<u32> {
        Ok(SYMCACHE_VERSION)
    }
}
