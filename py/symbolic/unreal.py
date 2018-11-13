from symbolic._lowlevel import lib, ffi
from symbolic._compat import range_type
from symbolic.utils import RustObject, rustcall, decode_str, attached_refs

__all__ = ['Unreal4Crash']

class Unreal4Crash(RustObject):
    __dealloc_func__ = lib.symbolic_unreal4_crash_free

    @classmethod
    def from_bytes(cls, buffer):
        """Parses an Unreal Engine 4 crash"""
        buffer = ffi.from_buffer(buffer)
        rv = cls._from_objptr(rustcall(lib.symbolic_unreal4_crash_from_bytes,
                                buffer, len(buffer)))
        attached_refs[rv] = buffer
        return rv

    def minidump_bytes(self):
        """The minidump from the Unreal Engine 4 crash"""
        len_out = ffi.new('uintptr_t *')
        rv = self._methodcall(lib.symbolic_unreal4_crash_get_minidump_bytes, len_out)
        if rv == ffi.NULL:
            return None
        return ffi.buffer(rv, len_out[0])

    @property
    def _file_count(self):
        """The count of files within the crash dump"""
        return self._methodcall(lib.symbolic_unreal4_crash_file_count)

    def _file_by_index(self, idx):
        """The file at the specified index within the dump"""
        rv = self._methodcall(lib.symbolic_unreal4_crash_file_by_index, idx)
        if rv == ffi.NULL:
            return None

        rv = CrashFileMeta._from_objptr(rv)
        rv.crash = self
        return rv

    def files(self):
        for idx in range_type(self._file_count):
            yield self._file_by_index(idx)


class CrashFileMeta(RustObject):

    @property
    def name(self):
        """The file name."""
        return str(decode_str(self._methodcall(lib.symbolic_unreal4_crash_file_meta_name)))

    @property
    def contents(self):
        """The contents of the file"""
        len_out = ffi.new('uintptr_t *')
        rv = self._methodcall(lib.symbolic_unreal4_crash_file_meta_contents, self.crash._objptr, len_out)
        if rv == ffi.NULL:
            return None
        rv = ffi.buffer(rv, len_out[0])
        # attached_refs[rv] = self.crash
        return rv
