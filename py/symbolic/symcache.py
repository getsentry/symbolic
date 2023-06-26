import io
import shutil
from symbolic._lowlevel import lib, ffi
from symbolic.utils import (
    RustObject,
    rustcall,
    decode_str,
    encode_str,
    encode_path,
    attached_refs,
    SliceReader,
)
from symbolic.common import parse_addr
from symbolic import exceptions


__all__ = [
    "SourceLocation",
    "SymCache",
    "find_best_instruction",
    "SYMCACHE_LATEST_VERSION",
]


# the most recent version for the symcache file format.
SYMCACHE_LATEST_VERSION = rustcall(lib.symbolic_symcache_latest_version)


class SourceLocation:
    def __init__(self, sym_addr, instr_addr, line, lang, symbol, full_path=None):
        self.sym_addr = sym_addr
        self.instr_addr = instr_addr
        self.line = line
        self.lang = lang
        self.symbol = symbol
        self.full_path = full_path or None

    def __str__(self) -> str:
        return "{}:{} ({})".format(
            self.symbol,
            self.line,
            self.full_path,
        )

    def __repr__(self) -> str:
        return "SourceLocation(%s)" % (
            ", ".join("%s=%r" % x for x in sorted(self.__dict__.items()))
        )


class SymCache(RustObject):
    __dealloc_func__ = lib.symbolic_symcache_free

    @classmethod
    def open(cls, path):
        """Loads a symcache from a file via mmap."""
        return cls._from_objptr(rustcall(lib.symbolic_symcache_open, encode_path(path)))

    @classmethod
    def from_object(cls, obj):
        """Creates a symcache from the given object."""
        return cls._from_objptr(
            rustcall(lib.symbolic_symcache_from_object, obj._get_objptr())
        )

    @classmethod
    def from_bytes(cls, data):
        """Loads a symcache from a binary buffer."""
        symcache = cls._from_objptr(
            rustcall(lib.symbolic_symcache_from_bytes, data, len(data))
        )
        attached_refs[symcache] = data
        return symcache

    @property
    def arch(self):
        """The architecture of the symcache."""
        arch = self._methodcall(lib.symbolic_symcache_get_arch)
        # make it an ascii bytestring on 2.x
        return str(decode_str(arch, free=True))

    @property
    def debug_id(self):
        """The debug identifier of the object."""
        id = self._methodcall(lib.symbolic_symcache_get_debug_id)
        return decode_str(id, free=True)

    @property
    def version(self):
        """Version of the file format."""
        return self._methodcall(lib.symbolic_symcache_get_version)

    @property
    def is_latest_version(self):
        """Returns true if this is the latest file format."""
        return self.version >= SYMCACHE_LATEST_VERSION

    def open_stream(self):
        """Returns a stream to read files from the internal buffer."""
        buf = self._methodcall(lib.symbolic_symcache_get_bytes)
        size = self._methodcall(lib.symbolic_symcache_get_size)
        return io.BufferedReader(SliceReader(ffi.buffer(buf, size), self))

    def dump_into(self, f):
        """Dumps the symcache into a file object."""
        shutil.copyfileobj(self.open_stream(), f)

    def lookup(self, addr):
        """Look up a single address."""
        addr = parse_addr(addr)
        rv = self._methodcall(lib.symbolic_symcache_lookup, addr)
        try:
            matches = []
            for idx in range(rv.len):
                sym = rv.items[idx]
                lang = decode_str(sym.lang, free=False)
                symbol = decode_str(sym.symbol, free=False)
                full_path = decode_str(sym.full_path, free=True)
                matches.append(
                    SourceLocation(
                        sym_addr=sym.sym_addr,
                        instr_addr=sym.instr_addr,
                        line=sym.line,
                        lang=lang,
                        symbol=symbol,
                        full_path=full_path,
                    )
                )
        finally:
            rustcall(lib.symbolic_lookup_result_free, ffi.addressof(rv))
        return matches


def find_best_instruction(addr, arch, crashing_frame=False, signal=None, ip_reg=None):
    """Given an instruction and meta data attempts to find the best one
    by using a heuristic we inherited from symsynd.
    """
    # Ensure we keep this local alive until this function returns as we
    # would otherwise operate on garbage
    encoded_arch = encode_str(arch)

    addr = parse_addr(addr)
    ii = ffi.new("SymbolicInstructionInfo *")
    ii[0].addr = addr
    ii[0].arch = encoded_arch
    ii[0].crashing_frame = crashing_frame
    ii[0].signal = signal or 0
    ii[0].ip_reg = parse_addr(ip_reg)
    try:
        return int(rustcall(lib.symbolic_find_best_instruction, ii))
    except exceptions.UnknownArchError:
        return int(addr)
