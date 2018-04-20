use symbolic::common::types::{Arch, UnknownArchError};

use core::SymbolicStr;

/// Mach-O architecture
#[repr(C)]
pub struct SymbolicMachoArch {
    pub cputype: u32,
    pub cpusubtype: u32,
}

/// ELF architecture
#[repr(C)]
pub struct SymbolicElfArch {
    pub machine: u16,
}

ffi_fn! {
    /// Checks if an architecture is known.
    unsafe fn symbolic_arch_is_known(arch: *const SymbolicStr) -> Result<bool> {
        Ok((*arch).as_str().parse::<Arch>().is_ok())
    }
}

ffi_fn! {
    /// Parses a Mach-O architecture.
    unsafe fn symbolic_arch_from_macho(arch: *const SymbolicMachoArch) -> Result<SymbolicStr> {
        let arch = &*arch;
        Ok(SymbolicStr::new(Arch::from_mach(arch.cputype, arch.cpusubtype)?.name()))
    }
}

ffi_fn! {
    /// Parses an ELF architecture.
    unsafe fn symbolic_arch_from_elf(arch: *const SymbolicElfArch) -> Result<SymbolicStr> {
        Ok(SymbolicStr::new(Arch::from_elf((*arch).machine)?.name()))
    }
}

ffi_fn! {
    /// Parses a Breakpad architecture.
    unsafe fn symbolic_arch_from_breakpad(arch: *const SymbolicStr) -> Result<SymbolicStr> {
        Ok(SymbolicStr::new(Arch::from_breakpad((*arch).as_str())?.name()))
    }
}

ffi_fn! {
    /// Returns the breakpad name for an architecture.
    unsafe fn symbolic_arch_to_breakpad(arch: *const SymbolicStr) -> Result<SymbolicStr> {
        let arch: Arch = (*arch).as_str().parse()?;
        Ok(SymbolicStr::new(arch.to_breakpad()))
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
