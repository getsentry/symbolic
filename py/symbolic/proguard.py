from symbolic._lowlevel import lib, ffi
from symbolic.utils import RustObject, rustcall, decode_str, encode_str, \
    decode_uuid, attached_refs


__all__ = ['ProguardMappingView']


class ProguardMappingView(RustObject):
    __dealloc_func__ = lib.symbolic_proguardmappingview_free

    @classmethod
    def from_bytes(cls, data):
        data = bytes(data)
        rv = cls._from_objptr(rustcall(
            lib.symbolic_proguardmappingview_from_bytes,
            data, len(data)))
        # we need to keep this reference alive or we crash. hard.
        attached_refs[rv] = data
        return rv

    @property
    def uuid(self):
        return decode_uuid(self._methodcall(
            lib.symbolic_proguardmappingview_get_uuid))

    @property
    def has_line_info(self):
        return bool(self._methodcall(
            lib.symbolic_proguardmappingview_has_line_info))

    def lookup(self, dotted_path, lineno=None):
        return decode_str(self._methodcall(
            lib.symbolic_proguardmappingview_convert_dotted_path,
            encode_str(dotted_path), lineno or 0))
