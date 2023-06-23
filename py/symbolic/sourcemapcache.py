from __future__ import annotations

from symbolic._lowlevel import lib, ffi
from symbolic.utils import RustObject, rustcall, decode_str


__all__ = ["SourceMapCache", "SourceMapCacheToken"]


class SourceMapCacheToken:
    """Represents a token matched or looked up from the cache."""

    line: int
    col: int
    src: str | None
    name: str | None
    function_name: str | None
    context_line: str | None
    pre_context: list[str]
    post_context: list[str]

    @classmethod
    def _from_objptr(cls, tm: ffi.CData) -> SourceMapCacheToken:
        rv = object.__new__(cls)
        rv.line = tm.line
        rv.col = tm.col
        rv.src = decode_str(tm.src, free=False) or None
        rv.name = decode_str(tm.name, free=False) or None
        rv.function_name = decode_str(tm.function_name, free=False) or None

        rv.context_line = decode_str(tm.context_line, free=False) or None

        rv.pre_context = []
        for idx in range(tm.pre_context.len):
            s = decode_str(tm.pre_context.strs[idx], free=False)
            rv.pre_context.append(s)

        rv.post_context = []
        for idx in range(tm.post_context.len):
            s = decode_str(tm.post_context.strs[idx], free=False)
            rv.post_context.append(s)

        return rv

    def __repr__(self) -> str:
        return "<SourceMapCacheToken %s:%d>" % (
            self.src,
            self.line,
        )


class SourceMapCache(RustObject):
    """Gives access to a sm cache."""

    __dealloc_func__ = lib.symbolic_sourcemapcache_free

    @classmethod
    def from_bytes(
        cls, source_content: bytes, sourcemap_content: bytes
    ) -> SourceMapCache:
        """Constructs a sourcemapcache from bytes."""
        return cls._from_objptr(
            rustcall(
                lib.symbolic_sourcemapcache_from_bytes,
                source_content,
                len(source_content),
                sourcemap_content,
                len(sourcemap_content),
            )
        )

    def lookup(
        self, line: int, col: int, context_lines: int
    ) -> SourceMapCacheToken | None:
        """Looks up a token from the sourcemap."""
        rv = self._methodcall(
            lib.symbolic_sourcemapcache_lookup_token, line, col, context_lines
        )

        if rv != ffi.NULL:
            try:
                return SourceMapCacheToken._from_objptr(rv)
            finally:
                rustcall(lib.symbolic_sourcemapcache_token_match_free, rv)

        return None
