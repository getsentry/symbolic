from symbolic._lowlevel import lib
from symbolic.utils import encode_str, decode_str, rustcall


__all__ = ['demangle_symbol']


def demangle_symbol(symbol, no_args=False):
    """Demangles a symbol."""
    if no_args:
        func = lib.symbolic_demangle_no_args
    else:
        func = lib.symbolic_demangle
    return decode_str(rustcall(func, encode_str(symbol)), free=True)
