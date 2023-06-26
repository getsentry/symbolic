from __future__ import annotations

import io
import uuid
import ntpath
import weakref
import posixpath
from typing import Protocol
from symbolic._lowlevel import ffi, lib
from symbolic.exceptions import exceptions_by_code, SymbolicError


__all__ = ["common_path_join", "strip_common_path_prefix"]


attached_refs: weakref.WeakKeyDictionary[object, bytes]
attached_refs = weakref.WeakKeyDictionary()


def _is_win_path(x: str) -> bool:
    return "\\" in x or (ntpath.isabs(x) and not posixpath.isabs(x))


def common_path_join(a: str, b: str) -> str:
    """Joins two paths together while guessing the platform (win vs unix)."""
    if _is_win_path(a):
        return ntpath.normpath(ntpath.join(a, b))
    return posixpath.join(a, b)


class _PathMod(Protocol):
    sep: str

    def normpath(self, s: str) -> str:
        ...


def strip_common_path_prefix(base: str, prefix: str) -> str:
    """Strips `prefix` from `a`."""
    if _is_win_path(base):
        path: _PathMod = ntpath
    else:
        path = posixpath
    pieces_a = path.normpath(base).split(path.sep)
    pieces_b = path.normpath(prefix).split(path.sep)
    if pieces_a[: len(pieces_b)] == pieces_b:
        return path.sep.join(pieces_a[len(pieces_b) :])
    return path.normpath(base)


class RustObject:
    __dealloc_func__ = None
    _objptr = None
    _shared = False

    def __init__(self) -> None:
        raise TypeError("Cannot instanciate %r objects" % self.__class__.__name__)

    @classmethod
    def _from_objptr(cls, ptr, shared=False):
        rv = object.__new__(cls)
        rv._objptr = ptr
        rv._shared = shared
        return rv

    def _methodcall(self, func, *args):
        return rustcall(func, self._get_objptr(), *args)

    def _get_objptr(self):
        if not self._objptr:
            raise RuntimeError("Object is closed")
        return self._objptr

    def _move(self, target):
        self._shared = True
        ptr = self._get_objptr()
        self._objptr = None
        return ptr

    def __del__(self) -> None:
        if self._objptr is None or self._shared:
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
    exc = cls(decode_str(msg, free=True))
    backtrace = decode_str(lib.symbolic_err_get_backtrace(), free=True)
    if backtrace:
        exc.rust_info = backtrace
    raise exc


def decode_str(s: ffi.CData, free: bool = False) -> str:
    """Decodes a SymbolicStr"""
    try:
        if s.len == 0:
            return ""
        return ffi.unpack(s.data, s.len).decode("utf-8", "replace")
    finally:
        if free and s.owned:
            lib.symbolic_str_free(ffi.addressof(s))


def encode_str(s: str | bytes) -> ffi.CData:
    """Encodes a SymbolicStr"""
    rv = ffi.new("SymbolicStr *")
    if isinstance(s, str):
        s = s.encode("utf-8")
    rv.data = ffi.from_buffer(s)
    rv.len = len(s)
    # we have to hold a weak reference here to ensure our string does not
    # get collected before the string is used.
    attached_refs[rv] = s
    return rv


def encode_path(s: str | bytes) -> bytes:
    """Encodes a path value."""
    if isinstance(s, str):
        s = s.encode("utf-8")
    if 0 in s:
        raise TypeError("Null bytes are not allowed in paths")
    return s


def decode_uuid(value: ffi.CData) -> uuid.UUID:
    """Decodes the given uuid value."""
    return uuid.UUID(bytes=bytes(bytearray(ffi.unpack(value.data, 16))))


def encode_uuid(value: uuid.UUID | str) -> ffi.CData:
    """Encodes the given uuid value for FFI."""
    encoded = ffi.new("SymbolicUuid *")
    encoded.data[0:16] = bytearray(make_uuid(value).bytes)
    return encoded


def make_uuid(value: uuid.UUID | str) -> uuid.UUID:
    """Converts a value into a python uuid object."""
    if isinstance(value, uuid.UUID):
        return value
    return uuid.UUID(value)


class SliceReader(io.RawIOBase):
    """A buffered reader that keeps the cache in memory"""

    def __init__(self, buf, cache):
        self._buffer = buf
        # Hold the cache so we do not lose the reference and crash on
        # the buffer disappearing
        self.cache = cache
        self.pos = 0

    @property
    def size(self):
        return len(self._buffer)

    def readable(self):
        return True

    def readinto(self, buf):
        n = len(buf)
        if n is None:
            end = len(self._buffer)
        else:
            end = min(self.pos + n, len(self._buffer))
        rv = self._buffer[self.pos : end]
        buf[: len(rv)] = rv
        self.pos = end
        return len(rv)


class PassThroughBufferedReader(io.BufferedReader):
    __slots__ = ()

    def __getattr__(self, attr):
        return getattr(self.raw, attr)


def make_buffered_slice_reader(buffer, cache):
    return PassThroughBufferedReader(SliceReader(buffer, cache))
