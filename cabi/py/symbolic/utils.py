from symbolic._lowlevel import ffi, lib
from symbolic._compat import text_type
from symbolic.exceptions import exceptions_by_code, SymbolicError


def rustcall(func, *args):
    """Calls rust method and does some error handling."""
    lib.symbolic_err_clear()
    rv = func(*args)
    err = lib.symbolic_err_get_last_code()
    if not err:
        return rv
    msg = lib.symbolic_err_get_last_message()
    cls = exceptions_by_code.get(err, SymbolicError)
    raise cls(decode_str(msg))


def decode_str(s):
    """Decodes a SymbolicStr"""
    return ffi.unpack(s.data, s.len).decode('utf-8')


def encode_str(s):
    """Encodes a SymbolicStr"""
    rv = ffi.new('SymbolicStr *')
    if isinstance(s, text_type):
        s = s.encode('utf-8')
    rv[0].data = ffi.from_buffer(s)
    rv[0].len = len(s)
    return rv


def test():
    return rustcall(lib.symbolic_str_from_cstr, '\xff\x23')
