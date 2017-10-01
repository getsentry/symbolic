use std::os::raw::c_char;
use std::ffi::CStr;

use symbolic_common::ByteView;
use symbolic_debuginfo::FatObject;

#[repr(C)]
pub struct SymbolicFatObject;

ffi_fn! {
    /// Loads a fat object from a given path.
    unsafe fn symbolic_di_fatobject_open(path: *const c_char) -> Result<*mut SymbolicFatObject> {
        let byteview = ByteView::from_path(CStr::from_ptr(path).to_str()?)?;
        let obj = FatObject::parse(byteview)?;
        Ok(Box::into_raw(Box::new(obj)) as *mut SymbolicFatObject)
    }
}

ffi_fn! {
    /// Frees the given fat object.
    unsafe fn symbolic_di_fatobject_free(sfo: *mut SymbolicFatObject) {
        if sfo.is_null() {
            let fo = sfo as *mut FatObject<'static>;
            Box::from_raw(fo);
        }
    }
}
