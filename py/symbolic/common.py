import os
from symbolic._lowlevel import lib, ffi
from symbolic._compat import string_types, int_types
from symbolic.utils import rustcall, encode_str, decode_str
from symbolic import exceptions


__all__ = ['arch_is_known', 'arch_from_macho',
           'arch_from_elf', 'arch_from_breakpad', 'arch_to_breakpad',
           'arch_get_ip_reg_name', 'parse_addr']


ignore_arch_exc = (exceptions.UnknownArchError,)


# Make sure we init the lib and turn on rust backtraces
os.environ['RUST_BACKTRACE'] = '1'
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


def arch_from_elf(machine):
    """Converts an ELF machine id into an arch string."""
    arch = ffi.new('SymbolicElfArch *')
    arch[0].machine = machine & 0xffff
    try:
        return str(decode_str(rustcall(lib.symbolic_arch_from_elf, encode_str(arch))))
    except ignore_arch_exc:
        pass


def arch_from_breakpad(arch):
    """Converts a Breakpad arch into our arch string"""
    try:
        return str(decode_str(rustcall(lib.symbolic_arch_from_breakpad, encode_str(arch))))
    except ignore_arch_exc:
        pass


def arch_to_breakpad(arch):
    """Converts an arch string into a Breakpad arch"""
    try:
        return str(decode_str(rustcall(lib.symbolic_arch_to_breakpad, arch)))
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
