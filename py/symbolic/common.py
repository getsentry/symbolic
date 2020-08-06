import os

from symbolic._lowlevel import lib, ffi
from symbolic._compat import string_types, int_types
from symbolic.utils import rustcall, encode_str, decode_str
from symbolic import exceptions


__all__ = ["arch_is_known", "arch_get_ip_reg_name", "normalize_arch", "parse_addr"]


ignore_arch_exc = (exceptions.UnknownArchError,)


# Make sure we init the lib and turn on rust backtraces
os.environ["RUST_BACKTRACE"] = "1"
ffi.init_once(lib.symbolic_init, "init")


def arch_is_known(arch):
    """Checks if an architecture is known."""
    if not isinstance(arch, string_types):
        return False
    return rustcall(lib.symbolic_arch_is_known, encode_str(arch))


def normalize_arch(arch):
    """Normalizes an architecture name."""
    if arch is None:
        return None
    if not isinstance(arch, string_types):
        raise ValueError("Invalid architecture: expected string")

    normalized = rustcall(lib.symbolic_normalize_arch, encode_str(arch))
    return decode_str(normalized, free=True)


def arch_get_ip_reg_name(arch):
    """Returns the ip register if known for this arch."""
    try:
        rv = rustcall(lib.symbolic_arch_ip_reg_name, encode_str(arch))
        return str(decode_str(rv, free=True))
    except ignore_arch_exc:
        pass


def parse_addr(x):
    """Parses an address."""
    if x is None:
        return 0
    if isinstance(x, int_types):
        return x
    if isinstance(x, string_types):
        if x[:2] == "0x":
            return int(x[2:], 16)
        return int(x)
    raise ValueError("Unsupported address format %r" % (x,))
