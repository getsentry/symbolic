from symbolic._lowlevel import lib, ffi
from symbolic.utils import (
    RustObject,
    rustcall,
    decode_str,
    encode_str,
    decode_uuid,
    encode_path,
    attached_refs,
)


__all__ = ["ProguardMappingView", "ProguardMapper", "JavaStackFrame"]


class ProguardMappingView(RustObject):
    """Gives access to proguard mapping files.

    DEPRECATED: Use the `ProguardMapper` class instead.
    """

    __dealloc_func__ = lib.symbolic_proguardmappingview_free

    @classmethod
    def from_bytes(cls, data):
        """Constructs a mapping file from bytes."""
        data = bytes(data)
        rv = cls._from_objptr(
            rustcall(lib.symbolic_proguardmappingview_from_bytes, data, len(data))
        )
        # we need to keep this reference alive or we crash. hard.
        attached_refs[rv] = data
        return rv

    @classmethod
    def open(cls, path):
        """Constructs a mapping file from a path."""
        return cls._from_objptr(
            rustcall(lib.symbolic_proguardmappingview_open, encode_path(path))
        )

    @property
    def uuid(self):
        """Returns the UUID of the file."""
        return decode_uuid(self._methodcall(lib.symbolic_proguardmappingview_get_uuid))

    @property
    def has_line_info(self):
        """True if the file contains line information."""
        return bool(self._methodcall(lib.symbolic_proguardmappingview_has_line_info))

    def lookup(self, dotted_path, lineno=None):
        """Given a dotted path and an optional line number this resolves
        to the original dotted path.
        """
        result = self._methodcall(
            lib.symbolic_proguardmappingview_convert_dotted_path,
            encode_str(dotted_path),
            lineno or 0,
        )

        return decode_str(result, free=True)


class JavaStackFrame(object):
    def __init__(self, class_name, method, line, file=None):
        self.class_name = class_name
        self.method = method
        self.file = file or None
        self.line = line


class ProguardMapper(RustObject):
    """Gives access to proguard mapping files."""

    __dealloc_func__ = lib.symbolic_proguardmapper_free

    @classmethod
    def open(cls, path):
        """Constructs a mapping file from a path."""
        return cls._from_objptr(
            rustcall(lib.symbolic_proguardmapper_open, encode_path(path))
        )

    @property
    def uuid(self):
        """Returns the UUID of the file."""
        return decode_uuid(self._methodcall(lib.symbolic_proguardmapper_get_uuid))

    @property
    def has_line_info(self):
        """True if the file contains line information."""
        return bool(self._methodcall(lib.symbolic_proguardmapper_has_line_info))

    def remap_class(self, klass):
        """Remaps the given class name."""
        klass = self._methodcall(
            lib.symbolic_proguardmapper_remap_class, encode_str(klass)
        )
        return decode_str(klass, free=True) or None

    def remap_frame(self, klass, method, line):
        """Remaps the stackframe, given its class, method and line."""
        result = self._methodcall(
            lib.symbolic_proguardmapper_remap_frame,
            encode_str(klass),
            encode_str(method),
            line,
        )

        frames = []
        try:
            for idx in range(result.len):
                frame = result.frames[idx]
                frames.append(
                    JavaStackFrame(
                        decode_str(frame.class_name, free=False),
                        decode_str(frame.method, free=False),
                        frame.line,
                        decode_str(frame.file, free=False),
                    )
                )
        finally:
            rustcall(lib.symbolic_proguardmapper_result_free, ffi.addressof(result))

        return frames
