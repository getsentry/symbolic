"""Minidump processing"""
from __future__ import annotations

import shutil
from typing import IO

from symbolic._lowlevel import lib, ffi
from symbolic.utils import (
    RustObject,
    rustcall,
    encode_path,
    make_buffered_slice_reader,
)

__all__ = [
    "CfiCache",
    "CFICACHE_LATEST_VERSION",
]

# The most recent version for the CFI cache file format
CFICACHE_LATEST_VERSION = rustcall(lib.symbolic_cficache_latest_version)


class CfiCache(RustObject):
    """A cache for call frame information (CFI) to improve native stackwalking"""

    __dealloc_func__ = lib.symbolic_cficache_free

    @classmethod
    def open(cls, path: str) -> CfiCache:
        """Loads a cficache from a file via mmap."""
        return cls._from_objptr(rustcall(lib.symbolic_cficache_open, encode_path(path)))

    @classmethod
    def from_object(cls, obj: RustObject) -> CfiCache:
        """Creates a cficache from the given object."""
        return cls._from_objptr(
            rustcall(lib.symbolic_cficache_from_object, obj._get_objptr())
        )

    @property
    def version(self) -> str:
        """Version of the file format."""
        return self._methodcall(lib.symbolic_cficache_get_version)

    @property
    def is_latest_version(self) -> bool:
        """Returns true if this is the latest file format."""
        return self.version >= CFICACHE_LATEST_VERSION

    def open_stream(self) -> IO[bytes]:
        """Returns a stream to read files from the internal buffer."""
        buf = self._methodcall(lib.symbolic_cficache_get_bytes)
        size = self._methodcall(lib.symbolic_cficache_get_size)
        return make_buffered_slice_reader(ffi.buffer(buf, size), self)

    def write_to(self, f: IO[bytes]) -> None:
        """Writes the CFI cache into a file object."""
        # https://github.com/python/mypy/issues/15031
        shutil.copyfileobj(self.open_stream(), f)  # type: ignore[misc]
