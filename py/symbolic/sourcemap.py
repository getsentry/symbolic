from __future__ import annotations

from typing import Generator, overload

from symbolic._lowlevel import lib, ffi
from symbolic.utils import RustObject, rustcall, decode_str, encode_str, attached_refs


__all__ = ["SourceView", "SourceMapView", "SourceMapTokenMatch"]


class SourceMapTokenMatch:
    """Represents a token matched or looked up from the index."""

    def __init__(
        self,
        src_line: int,
        src_col: int,
        dst_line: int,
        dst_col: int,
        src_id: int | None = None,
        name: str | None = None,
        src: str | None = None,
        function_name: str | None = None,
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
        return cls(
            src_line=tm.src_line,
            src_col=tm.src_col,
            dst_line=tm.dst_line,
            dst_col=tm.dst_col,
            src_id=tm.src_id,
            name=decode_str(tm.name, free=False) or None,
            src=decode_str(tm.src, free=False) or None,
            function_name=decode_str(tm.function_name, free=False) or None,
        )

    def __eq__(self, other: object) -> bool:
        if self.__class__ is not other.__class__:
            return False
        return self.__dict__ == other.__dict__

    def __ne__(self, other: object) -> bool:
        return not self.__eq__(other)

    def __repr__(self) -> str:
        return "<SourceMapTokenMatch %s:%d>" % (
            self.src,
            self.src_line,
        )


class SourceView(RustObject):
    """Gives reasonably efficient access to javascript sourcecode."""

    __dealloc_func__ = lib.symbolic_sourceview_free

    @classmethod
    def from_bytes(cls, data: bytes) -> SourceView:
        """Constructs a source view from bytes."""
        data = bytes(data)
        rv = cls._from_objptr(
            rustcall(lib.symbolic_sourceview_from_bytes, data, len(data))
        )
        # we need to keep this reference alive or we crash. hard.
        attached_refs[rv] = data
        return rv

    def get_source(self) -> str:
        source = self._methodcall(lib.symbolic_sourceview_as_str)
        return decode_str(source, free=True)

    def __len__(self) -> int:
        return self._methodcall(lib.symbolic_sourceview_get_line_count)

    @overload
    def __getitem__(self, idx: int) -> str:
        ...

    @overload
    def __getitem__(self, idx: slice) -> list[str]:
        ...

    def __getitem__(self, idx: int | slice) -> str | list[str]:
        if not isinstance(idx, slice):
            if idx >= len(self):
                raise IndexError("No such line")
            line = self._methodcall(lib.symbolic_sourceview_get_line, idx)
            return decode_str(line, free=True)

        rv = []
        for idx in range(*idx.indices(len(self))):
            try:
                rv.append(self[idx])
            except IndexError:
                pass
        return rv

    def __iter__(self) -> Generator[str, None, None]:
        for x in range(len(self)):
            yield self[x]


class SourceMapView(RustObject):
    """Gives access to a source map."""

    __dealloc_func__ = lib.symbolic_sourcemapview_free

    @classmethod
    def from_json_bytes(cls, data: bytes) -> SourceMapView:
        """Constructs a sourcemap from bytes of JSON data."""
        data = bytes(data)
        return cls._from_objptr(
            rustcall(lib.symbolic_sourcemapview_from_json_slice, data, len(data))
        )

    def lookup(
        self,
        line: int,
        col: int,
        minified_function_name: str | None = None,
        minified_source: SourceView | None = None,
    ) -> SourceMapTokenMatch | None:
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

        return None

    def get_sourceview(self, idx: int) -> SourceView | None:
        """Given a source index returns the source view that created it."""
        rv = self._methodcall(lib.symbolic_sourcemapview_get_sourceview, idx)
        if rv != ffi.NULL:
            return SourceView._from_objptr(rv, shared=True)

        return None

    @property
    def source_count(self) -> int:
        """Returns the number of sources."""
        return self._methodcall(lib.symbolic_sourcemapview_get_source_count)

    def get_source_name(self, idx: int) -> str | None:
        """Returns the name of the source at the given index."""
        name = self._methodcall(lib.symbolic_sourcemapview_get_source_name, idx)
        return decode_str(name, free=True) or None

    def iter_sources(self) -> Generator[tuple[int, str | None], None, None]:
        """Iterates over the sources in the file."""
        for src_id in range(self.source_count):
            yield src_id, self.get_source_name(src_id)

    def __len__(self) -> int:
        return self._methodcall(lib.symbolic_sourcemapview_get_tokens)

    def __getitem__(self, idx: int) -> SourceMapTokenMatch:
        rv = self._methodcall(lib.symbolic_sourcemapview_get_token, idx)
        if rv == ffi.NULL:
            raise IndexError("Token out of range")
        try:
            return SourceMapTokenMatch._from_objptr(rv)
        finally:
            rustcall(lib.symbolic_token_match_free, rv)

    def __iter__(self) -> Generator[SourceMapTokenMatch, None, None]:
        for x in range(len(self)):
            yield self[x]
