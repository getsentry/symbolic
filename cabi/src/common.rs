use symbolic::common::{Arch, UnknownArchError};

use crate::core::SymbolicStr;

ffi_fn! {
    /// Checks if an architecture is known.
    unsafe fn symbolic_arch_is_known(arch: *const SymbolicStr) -> Result<bool> {
        Ok((*arch).as_str().parse::<Arch>().is_ok())
    }
}

ffi_fn! {
    /// Returns the name of the instruction pointer if known.
    unsafe fn symbolic_arch_ip_reg_name(arch: *const SymbolicStr) -> Result<SymbolicStr> {
        let arch: Arch = (*arch).as_str().parse()?;
        let name = arch.ip_register_name().ok_or(UnknownArchError)?;
        Ok(SymbolicStr::new(name))
    }
}
