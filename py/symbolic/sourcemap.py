from symbolic._lowlevel import lib, ffi
from symbolic._compat import range_type
from symbolic.utils import RustObject, rustcall, decode_str, encode_str, \
    attached_refs


__all__ = ['SourceView', 'SourceMapView', 'TokenMatch']


class TokenMatch(object):

    def __init__(self):
        raise TypeError('Cannot create token match objects')

    @classmethod
    def _from_objptr(cls, tm):
        rv = object.__new__(cls)
        rv.src_line = tm.src_line
        rv.src_col = tm.src_col
        rv.dst_line = tm.dst_line
        rv.dst_col = tm.dst_col
        rv.src_id = tm.src_id
        rv.name = decode_str(tm.name) or None
        rv.src = decode_str(tm.src) or None
        rv.function_name = decode_str(tm.function_name) or None
        return rv

    def __eq__(self, other):
        if self.__class__ is not other.__class__:
            return False
        return self.__dict__ == other.__dict__

    def __ne__(self, other):
        return not self.__eq__(other)

    def __repr__(self):
        return '<TokenMatch %s:%d>' % (
            self.src,
            self.src_line,
        )


class SourceView(RustObject):
    __dealloc_func__ = lib.symbolic_sourceview_free

    @classmethod
    def from_bytes(cls, data):
        data = bytes(data)
        rv = cls._from_objptr(rustcall(lib.symbolic_sourceview_from_bytes,
                              data, len(data)))
        # we need to keep this reference alive or we crash. hard.
        attached_refs[rv] = data
        return rv

    def __len__(self):
        return self._methodcall(lib.symbolic_sourceview_get_line_count)

    def __getitem__(self, idx):
        if idx >= len(self):
            raise LookupError('No such line')
        return decode_str(self._methodcall(
            lib.symbolic_sourceview_get_line, idx))

    def __iter__(self):
        for x in range_type(len(self)):
            yield self[x]


class SourceMapView(RustObject):
    __dealloc_func__ = lib.symbolic_sourceview_free

    @classmethod
    def from_json_bytes(cls, data):
        data = bytes(data)
        return cls._from_objptr(rustcall(
            lib.symbolic_sourcemapview_from_json_slice, data, len(data)))

    def lookup(self, line, col, minified_function_name=None,
               minified_source=None):
        if minified_function_name is None or minified_source is None:
            rv = self._methodcall(
                lib.symbolic_sourcemapview_lookup_token, line, col)
        else:
            if not isinstance(minified_source, SourceView):
                raise TypeError('source view required')
            rv = self._methodcall(
                lib.symbolic_sourcemapview_lookup_token_with_function_name,
                line, col, encode_str(minified_function_name),
                minified_source._objptr)
        if rv != ffi.NULL:
            try:
                return TokenMatch._from_objptr(rv)
            finally:
                rustcall(lib.symbolic_token_match_free, rv)

    def get_sourceview(self, idx):
        rv = self._methodcall(lib.symbolic_sourcemapview_get_sourceview, idx)
        if rv != ffi.NULL:
            return SourceView._from_objptr(rv, shared=True)

    def __len__(self):
        return self._methodcall(lib.symbolic_sourcemapview_get_tokens)

    def __getitem__(self, idx):
        rv = self._methodcall(lib.symbolic_sourcemapview_get_token, idx)
        if rv == ffi.NULL:
            raise LookupError('Token out of range')
        try:
            return TokenMatch._from_objptr(rv)
        finally:
            rustcall(lib.symbolic_token_match_free, rv)

    def __iter__(self):
        for x in range_type(len(self)):
            yield self[x]
