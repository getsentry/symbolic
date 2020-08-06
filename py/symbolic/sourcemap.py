from symbolic._lowlevel import lib, ffi
from symbolic._compat import range_type
from symbolic.utils import RustObject, rustcall, decode_str, encode_str, attached_refs


__all__ = ["SourceView", "SourceMapView", "SourceMapTokenMatch"]


class SourceMapTokenMatch(object):
    """Represents a token matched or looked up from the index."""

    def __init__(
        self,
        src_line,
        src_col,
        dst_line,
        dst_col,
        src_id=None,
        name=None,
        src=None,
        function_name=None,
    ):
        self.src_line = src_line
        self.src_col = src_col
        self.dst_line = dst_line
        self.dst_col = dst_col
        self.src_id = src_id
        self.name = name
        self.src = src
        self.function_name = function_name

    @classmethod
    def _from_objptr(cls, tm):
        rv = object.__new__(cls)
        rv.src_line = tm.src_line
        rv.src_col = tm.src_col
        rv.dst_line = tm.dst_line
        rv.dst_col = tm.dst_col
        rv.src_id = tm.src_id
        rv.name = decode_str(tm.name, free=False) or None
        rv.src = decode_str(tm.src, free=False) or None
        rv.function_name = decode_str(tm.function_name, free=False) or None
        return rv

    def __eq__(self, other):
        if self.__class__ is not other.__class__:
            return False
        return self.__dict__ == other.__dict__

    def __ne__(self, other):
        return not self.__eq__(other)

    def __repr__(self):
        return "<SourceMapTokenMatch %s:%d>" % (self.src, self.src_line,)


class SourceView(RustObject):
    """Gives reasonably efficient access to javascript sourcecode."""

    __dealloc_func__ = lib.symbolic_sourceview_free

    @classmethod
    def from_bytes(cls, data):
        """Constructs a source view from bytes."""
        data = bytes(data)
        rv = cls._from_objptr(
            rustcall(lib.symbolic_sourceview_from_bytes, data, len(data))
        )
        # we need to keep this reference alive or we crash. hard.
        attached_refs[rv] = data
        return rv

    def get_source(self):
        source = self._methodcall(lib.symbolic_sourceview_as_str)
        return decode_str(source, free=True)

    def __len__(self):
        return self._methodcall(lib.symbolic_sourceview_get_line_count)

    def __getitem__(self, idx):
        if not isinstance(idx, slice):
            if idx >= len(self):
                raise IndexError("No such line")
            line = self._methodcall(lib.symbolic_sourceview_get_line, idx)
            return decode_str(line, free=True)

        rv = []
        for idx in range_type(*idx.indices(len(self))):
            try:
                rv.append(self[idx])
            except IndexError:
                pass
        return rv

    def __iter__(self):
        for x in range_type(len(self)):
            yield self[x]


class SourceMapView(RustObject):
    """Gives access to a source map."""

    __dealloc_func__ = lib.symbolic_sourcemapview_free

    @classmethod
    def from_json_bytes(cls, data):
        """Constructs a sourcemap from bytes of JSON data."""
        data = bytes(data)
        return cls._from_objptr(
            rustcall(lib.symbolic_sourcemapview_from_json_slice, data, len(data))
        )

    def lookup(self, line, col, minified_function_name=None, minified_source=None):
        """Looks up a token from the sourcemap and optionally also
        resolves a function name from a stacktrace to the original one.
        """
        if minified_function_name is None or minified_source is None:
            rv = self._methodcall(lib.symbolic_sourcemapview_lookup_token, line, col)
        else:
            if not isinstance(minified_source, SourceView):
                raise TypeError("source view required")
            rv = self._methodcall(
                lib.symbolic_sourcemapview_lookup_token_with_function_name,
                line,
                col,
                encode_str(minified_function_name),
                minified_source._objptr,
            )
        if rv != ffi.NULL:
            try:
                return SourceMapTokenMatch._from_objptr(rv)
            finally:
                rustcall(lib.symbolic_token_match_free, rv)

    def get_sourceview(self, idx):
        """Given a source index returns the source view that created it."""
        rv = self._methodcall(lib.symbolic_sourcemapview_get_sourceview, idx)
        if rv != ffi.NULL:
            return SourceView._from_objptr(rv, shared=True)

    @property
    def source_count(self):
        """Returns the number of sources."""
        return self._methodcall(lib.symbolic_sourcemapview_get_source_count)

    def get_source_name(self, idx):
        """Returns the name of the source at the given index."""
        name = self._methodcall(lib.symbolic_sourcemapview_get_source_name, idx)
        return decode_str(name, free=True) or None

    def iter_sources(self):
        """Iterates over the sources in the file."""
        for src_id in range_type(self.source_count):
            yield src_id, self.get_source_name(src_id)

    def __len__(self):
        return self._methodcall(lib.symbolic_sourcemapview_get_tokens)

    def __getitem__(self, idx):
        rv = self._methodcall(lib.symbolic_sourcemapview_get_token, idx)
        if rv == ffi.NULL:
            raise IndexError("Token out of range")
        try:
            return SourceMapTokenMatch._from_objptr(rv)
        finally:
            rustcall(lib.symbolic_token_match_free, rv)

    def __iter__(self):
        for x in range_type(len(self)):
            yield self[x]
