from symbolic._lowlevel import lib, ffi
from symbolic.utils import rustcall, encode_str, decode_str


def arch_is_known(value):
    """Checks if an architecture is known."""
    return rustcall(lib.symbolic_arch_is_known, encode_str(value))


def arch_from_macho(cputype, cpusubtype):
    """Converts a macho arch tuple into an arch string."""
    arch = ffi.new('SymbolicMachoArch *')
    arch[0].cputype = cputype
    arch[0].cpusubtype = cpusubtype
    return str(decode_str(rustcall(lib.symbolic_arch_from_macho, arch)))


def arch_to_macho(arch):
    """Converts a macho arch tuple into an arch string."""
    arch = rustcall(lib.symbolic_arch_to_macho, encode_str(arch))
    return (arch.cputype, arch.cpusubtype)
