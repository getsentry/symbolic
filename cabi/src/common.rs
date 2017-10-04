use symbolic_common::Arch;

use core::SymbolicStr;

#[repr(C)]
pub struct SymbolicMachoArch {
    pub cputype: u32,
    pub cpusubtype: u32,
}

ffi_fn! {
    /// Checks if an architecture is known.
    unsafe fn symbolic_arch_is_known(arch: *const SymbolicStr) -> Result<bool> {
        Ok(Arch::parse((*arch).as_str()).is_ok())
    }
}

ffi_fn! {
    /// Checks if an architecture is known.
    unsafe fn symbolic_arch_from_macho(arch: &SymbolicMachoArch) -> Result<SymbolicStr> {
        Ok(SymbolicStr::new(Arch::from_mach(arch.cputype, arch.cpusubtype).map(|x| x.name())?))
    }
}

ffi_fn! {
    /// Returns the macho code for a CPU architecture.
    unsafe fn symbolic_arch_to_macho(arch: *const SymbolicStr) -> Result<SymbolicMachoArch> {
        let (cputype, cpusubtype) = Arch::parse((*arch).as_str())?.to_mach()?;
        Ok(SymbolicMachoArch {
            cputype: cputype,
            cpusubtype: cpusubtype,
        })
    }
}
