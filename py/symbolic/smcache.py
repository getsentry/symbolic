from symbolic._lowlevel import lib, ffi
from symbolic.utils import RustObject, rustcall, decode_str


__all__ = ["SmCache", "SmCacheToken"]


class SmCacheToken(object):
    """Represents a token matched or looked up from the cache."""

    @classmethod
    def _from_objptr(cls, tm):
        rv = object.__new__(cls)
        rv.line = tm.line
        rv.col = tm.col
        rv.function_name = decode_str(tm.function_name, free=False) or None

        rv.context = decode_str(tm.src, free=False) or None

        rv.pre_context = []
        for idx in range(tm.pre_context.len):
            s = decode_str(tm.pre_context.strs[idx], free=False)
            rv.pre_context.append(s)

        rv.post_context = []
        for idx in range(tm.post_context.len):
            s = decode_str(tm.post_context.strs[idx], free=False)
            rv.post_context.append(s)

        return rv

    def __repr__(self):
        return "<SmCacheToken %s:%d>" % (
            self.src,
            self.line,
        )


class SmCache(RustObject):
    """Gives access to a sm cache."""

    __dealloc_func__ = lib.symbolic_smcache_free

    @classmethod
    def from_bytes(cls, source_content, sourcemap_content):
        """Constructs a smcache from bytes."""
        source_content = bytes(source_content.encode("utf-8"))
        sourcemap_content = bytes(sourcemap_content.encode("utf-8"))
        return cls._from_objptr(
            rustcall(
                lib.symbolic_smcache_from_bytes,
                source_content,
                len(source_content),
                sourcemap_content,
                len(sourcemap_content),
            )
        )

    def lookup(self, line, col):
        """Looks up a token from the sourcemap."""
        rv = self._methodcall(lib.symbolic_smcache_lookup_token, line, col)

        if rv != ffi.NULL:
            try:
                return SmCacheToken._from_objptr(rv)
            finally:
                rustcall(lib.symbolic_smcache_token_match_free, rv)
