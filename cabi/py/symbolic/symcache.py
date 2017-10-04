from symbolic._compat import implements_to_string
from symbolic._lowlevel import lib, ffi
from symbolic.demangle import demangle
from symbolic.utils import RustObject, rustcall, decode_str, decode_uuid, \
     common_path_join, strip_common_path_prefix, encode_path


@implements_to_string
class Symbol(object):

    def __init__(self, sym_addr, instr_addr, line, symbol,
                 filename, base_dir, comp_dir):
        self.sym_addr = sym_addr
        self.instr_addr = instr_addr
        self.line = line
        self.symbol = symbol
        self.filename = filename
        self.base_dir = base_dir
        self.comp_dir = comp_dir

    @property
    def function_name(self):
        """The demangled function name."""
        return demangle(self.symbol)

    @property
    def abs_path(self):
        """Returns the absolute path."""
        return common_path_join(self.base_dir, self.filename)

    @property
    def rel_path(self):
        """Returns the relative path to the comp dir."""
        return strip_common_path_prefix(self.abs_path, self.comp_dir)

    def __str__(self):
        return '%s:%s (%s)' % (
            self.function_name,
            self.line,
            self.rel_path,
        )

    def __repr__(self):
        return 'Symbol(%s)' % (
            ', '.join('%s=%r' % x for x in sorted(self.__dict__.items()))
        )


class SymCache(RustObject):
    __dealloc_func__ = lib.symbolic_symcache_free

    @classmethod
    def from_path(self, path):
        """Loads a symcache from a file via mmap."""
        return SymCache._from_objptr(
            rustcall(lib.symbolic_symcache_from_path, path))

    @classmethod
    def from_bytes(self, bytes):
        """Loads a symcache from a file via mmap."""
        bytes = memoryview(bytes)
        return SymCache._from_objptr(
            rustcall(lib.symbolic_symcache_from_bytes, bytes, len(bytes)))

    @property
    def arch(self):
        """The architecture of the symcache."""
        # make it an ascii bytestring on 2.x
        return str(decode_str(self._methodcall(lib.symbolic_symcache_get_arch)))

    @property
    def uuid(self):
        """The UUID of the object."""
        return decode_uuid(self._methodcall(lib.symbolic_symcache_get_uuid))

    @property
    def has_line_info(self):
        """Does this file have line information?"""
        return self._methodcall(lib.symbolic_symcache_has_line_info)

    @property
    def has_file_info(self):
        """Does this file have file information?"""
        return self._methodcall(lib.symbolic_symcache_has_file_info)

    @property
    def buffer(self):
        """Returns the underlying bytes of the cache."""
        buf = self._methodcall(lib.symbolic_symcache_get_bytes)
        size = self._methodcall(lib.symbolic_symcache_get_size)
        return ffi.buffer(buf, size)

    def dump(self, f):
        """Dumps the symcache into a file object."""
        f.write(self.buffer)

    def lookup(self, addr):
        """Look up a single address."""
        rv = self._methodcall(lib.symbolic_symcache_lookup, addr)
        try:
            matches = []
            for idx in range(rv.len):
                sym = rv.items[idx]
                matches.append(Symbol(
                    sym_addr=sym.sym_addr,
                    instr_addr=sym.instr_addr,
                    line=sym.line,
                    symbol=decode_str(sym.symbol),
                    filename=decode_str(sym.filename),
                    base_dir=decode_str(sym.base_dir),
                    comp_dir=decode_str(sym.comp_dir),
                ))
        finally:
            rustcall(lib.symbolic_lookup_result_free, ffi.addressof(rv))
        return matches
