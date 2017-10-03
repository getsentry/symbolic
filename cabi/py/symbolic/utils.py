import uuid
from symbolic._lowlevel import ffi, lib
from symbolic._compat import text_type, NUL
from symbolic.exceptions import exceptions_by_code, SymbolicError


class RustObject(object):
    __dealloc_func__ = None
    _objptr = None

    def __init__(self):
        raise TypeError('Cannot instanciate %r objects' %
                        self.__class__.__name__)

    @classmethod
    def _from_objptr(cls, ptr):
        rv = object.__new__(cls)
        rv._objptr = ptr
        return rv

    def _methodcall(self, func, *args):
        return rustcall(func, self._get_objptr(), *args)

    def _get_objptr(self):
        if not self._objptr:
            raise RuntimeError('Object is closed')
        return self._objptr

    def __del__(self):
        if self._objptr is None:
            return
        f = self.__class__.__dealloc_func__
        if f is not None:
            rustcall(f, self._objptr)
            self._objptr = None


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


def encode_path(s):
    """Encodes a path value."""
    if isinstance(s, text_type):
        s = s.encode('utf-8')
    if NUL in s:
        raise TypeError('Null bytes are not allowed in paths')
    return s


def decode_uuid(value):
    """Decodes the given uuid value."""
    return uuid.UUID(bytes=ffi.string(value.data))
