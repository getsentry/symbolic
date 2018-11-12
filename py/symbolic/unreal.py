from symbolic._lowlevel import lib, ffi
from symbolic.utils import RustObject, rustcall

__all__ = ['Unreal4Crash']

class Unreal4Crash(RustObject):
    __dealloc_func__ = lib.symbolic_unreal4_crash_free

    @classmethod
    def from_bytes(cls, buffer):
        """Parses an Unreal Engine 4 crash"""
        buffer = ffi.from_buffer(buffer)
        return cls._from_objptr(rustcall(lib.symbolic_unreal4_crash_from_bytes,
                                buffer, len(buffer)))

    def minidump_bytes(self):
        """The minidump from the Unreal Engine 4 crash"""
        len_out = ffi.new('uintptr_t *')
        rv = self._methodcall(lib.symbolic_unreal4_crash_get_minidump_bytes, len_out)
        if rv == ffi.NULL:
            return None
        return ffi.buffer(rv, len_out[0])
