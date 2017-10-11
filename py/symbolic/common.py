from symbolic._lowlevel import lib, ffi
from symbolic._compat import string_types, int_types
from symbolic.utils import rustcall, encode_str, decode_str
from symbolic import exceptions


__all__ = ['arch_is_known', 'arch_from_macho', 'arch_to_macho',
           'arch_get_ip_reg_name', 'parse_addr']


ignore_arch_exc = (exceptions.NotFound, exceptions.Parse)


# Make sure we init the lib
ffi.init_once(lib.symbolic_init, 'init')


def arch_is_known(value):
    """Checks if an architecture is known."""
    return rustcall(lib.symbolic_arch_is_known, encode_str(value))


def arch_from_macho(cputype, cpusubtype):
    """Converts a macho arch tuple into an arch string."""
    arch = ffi.new('SymbolicMachoArch *')
    arch[0].cputype = cputype & 0xffffffff
    arch[0].cpusubtype = cpusubtype & 0xffffffff
    try:
        return str(decode_str(rustcall(lib.symbolic_arch_from_macho, arch)))
    except ignore_arch_exc:
        pass


def arch_to_macho(arch):
    """Converts a macho arch tuple into an arch string."""
    try:
        arch = rustcall(lib.symbolic_arch_to_macho, encode_str(arch))
        return (arch.cputype, arch.cpusubtype)
    except ignore_arch_exc:
        pass


def arch_get_ip_reg_name(arch):
    """Returns the ip register if known for this arch."""
    try:
        return str(decode_str(rustcall(
            lib.symbolic_arch_ip_reg_name, encode_str(arch))))
    except ignore_arch_exc:
        pass


def parse_addr(x):
    """Parses an address."""
    if x is None:
        return 0
    if isinstance(x, int_types):
        return x
    if isinstance(x, string_types):
        if x[:2] == '0x':
            return int(x[2:], 16)
        return int(x)
    raise ValueError('Unsupported address format %r' % (x,))
