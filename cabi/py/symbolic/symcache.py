from symbolic._lowlevel import lib, ffi
from symbolic.utils import RustObject, rustcall, decode_str, decode_uuid


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

    def __repr__(self):
        return 'Symbol(%s)' % (
            ', '.join('%s=%r' % x for x in sorted(self.__dict__.items()))
        )


class SymCache(RustObject):
    __dealloc_func__ = lib.symbolic_symcache_free

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
