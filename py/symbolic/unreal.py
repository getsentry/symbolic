import json

from symbolic._lowlevel import lib, ffi
from symbolic._compat import range_type

from symbolic.utils import (
    RustObject,
    rustcall,
    decode_str,
    attached_refs,
    make_buffered_slice_reader,
)

__all__ = ["Unreal4Crash"]


class Unreal4Crash(RustObject):
    __dealloc_func__ = lib.symbolic_unreal4_crash_free

    @classmethod
    def from_bytes(cls, buffer):
        """Parses an Unreal Engine 4 crash"""
        buffer = ffi.from_buffer(buffer)
        rv = cls._from_objptr(
            rustcall(lib.symbolic_unreal4_crash_from_bytes, buffer, len(buffer))
        )
        attached_refs[rv] = buffer
        return rv

    def get_context(self):
        context_json = self._methodcall(lib.symbolic_unreal4_get_context)
        return json.loads(decode_str(context_json, free=True))

    def get_logs(self):
        logs_json = self._methodcall(lib.symbolic_unreal4_get_logs)
        return json.loads(decode_str(logs_json, free=True))

    @property
    def _file_count(self):
        """The count of files within the crash dump"""
        return self._methodcall(lib.symbolic_unreal4_crash_file_count)

    def _file_by_index(self, idx):
        """The file at the specified index within the dump"""
        rv = self._methodcall(lib.symbolic_unreal4_crash_file_by_index, idx)
        if rv == ffi.NULL:
            return None

        return Unreal4CrashFile._from_objptr(rv)

    def files(self):
        """Enumerate files within the UE4 crash"""
        for idx in range_type(self._file_count):
            yield self._file_by_index(idx)


class Unreal4CrashFile(RustObject):
    __dealloc_func__ = lib.symbolic_unreal4_file_free

    @property
    def name(self):
        """The file name."""
        name = self._methodcall(lib.symbolic_unreal4_file_name)
        return str(decode_str(name, free=True))

    @property
    def type(self):
        """The type of the file"""
        ty = self._methodcall(lib.symbolic_unreal4_file_type)
        return str(decode_str(ty, free=True))

    def open_stream(self):
        """Returns a stream to read files from the internal buffer."""
        len_out = ffi.new("uintptr_t *")
        rv = self._methodcall(lib.symbolic_unreal4_file_data, len_out)
        if rv == ffi.NULL:
            return None
        return make_buffered_slice_reader(ffi.buffer(rv, len_out[0]), self)

    @property
    def size(self):
        """Returns the size of the file in bytes."""
        len_out = ffi.new("uintptr_t *")
        self._methodcall(lib.symbolic_unreal4_file_data, len_out)
        return len_out[0]
