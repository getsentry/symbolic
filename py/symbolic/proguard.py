from symbolic._lowlevel import lib, ffi
from symbolic.utils import RustObject, rustcall, decode_str, encode_str, \
    decode_uuid, encode_path, attached_refs


__all__ = ['ProguardMappingView']


class ProguardMappingView(RustObject):
    """Gives access to proguard mapping files."""
    __dealloc_func__ = lib.symbolic_proguardmappingview_free

    @classmethod
    def from_bytes(cls, data):
        """Constructs a mapping file from bytes."""
        data = bytes(data)
        rv = cls._from_objptr(rustcall(
            lib.symbolic_proguardmappingview_from_bytes,
            data, len(data)))
        # we need to keep this reference alive or we crash. hard.
        attached_refs[rv] = data
        return rv

    @classmethod
    def open(cls, path):
        """Constructs a mapping file from a path."""
        return cls._from_objptr(rustcall(
            lib.symbolic_proguardmappingview_open,
            encode_path(path)))

    @property
    def uuid(self):
        """Returns the UUID of the file."""
        return decode_uuid(self._methodcall(
            lib.symbolic_proguardmappingview_get_uuid))

    @property
    def has_line_info(self):
        """True if the file contains line information."""
        return bool(self._methodcall(
            lib.symbolic_proguardmappingview_has_line_info))

    def lookup(self, dotted_path, lineno=None):
        """Given a dotted path and an optional line number this resolves
        to the original dotted path.
        """
        return decode_str(self._methodcall(
            lib.symbolic_proguardmappingview_convert_dotted_path,
            encode_str(dotted_path), lineno or 0))
