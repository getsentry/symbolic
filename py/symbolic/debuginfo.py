from __future__ import annotations

import bisect
from weakref import WeakValueDictionary
from typing import overload, Generator

from symbolic._lowlevel import lib, ffi
from symbolic.utils import RustObject, rustcall, decode_str, encode_str
from symbolic.common import parse_addr, arch_is_known
from symbolic.symcache import SymCache
from symbolic.cfi import CfiCache


__all__ = [
    "Archive",
    "Object",
    "ObjectLookup",
    "BcSymbolMap",
    "UuidMapping",
    "id_from_breakpad",
    "normalize_code_id",
    "normalize_debug_id",
]


class Archive(RustObject):
    __dealloc_func__ = lib.symbolic_archive_free

    _objcache: WeakValueDictionary[int, Object]

    @classmethod
    def open(self, path: str | bytes) -> Archive:
        """Opens an archive from a given path."""
        if isinstance(path, str):
            path = path.encode("utf-8")
        return Archive._from_objptr(rustcall(lib.symbolic_archive_open, path))

    @classmethod
    def from_bytes(self, data: bytes) -> Archive:
        """Loads an archive from a binary buffer."""
        return Archive._from_objptr(
            rustcall(lib.symbolic_archive_from_bytes, data, len(data))
        )

    @property
    def object_count(self) -> int:
        """The number of objects in this archive."""
        return self._methodcall(lib.symbolic_archive_object_count)

    def iter_objects(self) -> Generator[Object, None, None]:
        """Iterates over all objects."""
        for idx in range(self.object_count):
            try:
                yield self._get_object(idx)
            except LookupError:
                pass

    def get_object(
        self, debug_id: str | None = None, arch: str | None = None
    ) -> Object:
        """Get an object by either arch or id."""
        if debug_id is not None:
            debug_id = debug_id.lower()
        for obj in self.iter_objects():
            if obj.debug_id == debug_id or obj.arch == arch:
                return obj
        raise LookupError("Object not found")

    def _get_object(self, idx: int) -> Object:
        """Returns the object at a certain index."""
        cache = getattr(self, "_objcache", None)
        if cache is None:
            cache = self._objcache = WeakValueDictionary()
        rv = cache.get(idx)
        if rv is not None:
            return rv
        ptr = self._methodcall(lib.symbolic_archive_get_object, idx)
        if ptr == ffi.NULL:
            raise LookupError("No object #%d" % idx)
        rv = cache[idx] = Object._from_objptr(ptr)
        return rv


class Object(RustObject):
    __dealloc_func__ = lib.symbolic_object_free

    @property
    def arch(self) -> str:
        """The architecture of the object."""
        # make it an ascii bytestring on 2.x
        arch = self._methodcall(lib.symbolic_object_get_arch)
        return str(decode_str(arch, free=True))

    @property
    def code_id(self) -> str | None:
        """The code identifier of the object. Returns None if there is no code id."""
        code_id = self._methodcall(lib.symbolic_object_get_code_id)
        code_id = decode_str(code_id, free=True)
        if code_id:
            return code_id
        return None

    @property
    def debug_id(self) -> str:
        """The debug identifier of the object."""
        debug_id = self._methodcall(lib.symbolic_object_get_debug_id)
        return decode_str(debug_id, free=True)

    @property
    def kind(self) -> str:
        """The kind of the object (e.g. executable, debug file, library, ...)."""
        kind = self._methodcall(lib.symbolic_object_get_kind)
        return str(decode_str(kind, free=True))

    @property
    def file_format(self) -> str:
        """The file format of the object file (e.g. MachO, ELF, ...)."""
        format = self._methodcall(lib.symbolic_object_get_file_format)
        return str(decode_str(format, free=True))

    @property
    def features(self) -> frozenset[str]:
        """The list of features offered by this debug file."""
        struct = self._methodcall(lib.symbolic_object_get_features)
        features = set()
        if struct.symtab:
            features.add("symtab")
        if struct.debug:
            features.add("debug")
        if struct.unwind:
            features.add("unwind")
        if struct.sources:
            features.add("sources")
        return frozenset(features)

    def make_symcache(self) -> SymCache:
        """Creates a symcache from the object."""
        return SymCache._from_objptr(
            self._methodcall(lib.symbolic_symcache_from_object)
        )

    def make_cficache(self) -> CfiCache:
        """Creates a cficache from the object."""
        return CfiCache._from_objptr(
            self._methodcall(lib.symbolic_cficache_from_object)
        )

    def __repr__(self) -> str:
        return "<Object {} {!r}>".format(
            self.debug_id,
            self.arch,
        )


class ObjectRef:
    """Holds a reference to an object in a format."""

    def __init__(self, data):
        self.addr = parse_addr(data.get("image_addr"))
        # not a real address but why handle it differently
        self.size = parse_addr(data.get("image_size"))
        self.vmaddr = data.get("image_vmaddr")
        self.code_id = data.get("code_id")
        self.code_file = data.get("code_file") or data.get("name")
        self.debug_id = normalize_debug_id(
            data.get("debug_id") or data.get("id") or data.get("uuid") or None
        )
        self.debug_file = data.get("debug_file")

        if data.get("arch") is not None and arch_is_known(data["arch"]):
            self.arch = data["arch"]
        else:
            self.arch = None

        # Legacy alias for backwards compatibility
        self.name = self.code_file

    def __repr__(self) -> str:
        return "<ObjectRef {} {!r}>".format(
            self.debug_id,
            self.arch,
        )


class ObjectLookup:
    """Helper to look up objects based on the info a client provides."""

    def __init__(self, objects):
        self._addresses = []
        self._by_addr = {}
        self.objects = {}
        for ref_data in objects:
            obj = ObjectRef(ref_data)
            self._addresses.append(obj.addr)
            self._by_addr[obj.addr] = obj
            self.objects[obj.debug_id] = obj
        self._addresses.sort()

    def iter_objects(self):
        """Iterates over all objects."""
        return self.objects.values()

    def get_debug_ids(self):
        """Returns a list of ids."""
        return sorted(self.objects)

    def iter_debug_ids(self):
        """Iterates over all ids."""
        return iter(self.objects)

    def find_object(self, addr):
        """Given an instruction address this locates the image this address
        is contained in.
        """
        idx = bisect.bisect_right(self._addresses, parse_addr(addr))
        if idx > 0:
            rv = self._by_addr[self._addresses[idx - 1]]
            if not rv.size or parse_addr(addr) < rv.addr + rv.size:
                return rv

    def get_object(self, debug_id):
        """Finds an object by the given debug id."""
        return self.objects.get(debug_id)


class BcSymbolMap(RustObject):
    """Object representing an Apple ``.bcsymbolmap`` file."""

    __dealloc_func__ = lib.symbolic_bcsymbolmap_free

    @classmethod
    def open(cls, path: str | bytes) -> BcSymbolMap:
        """Parses a BCSymbolMap file."""
        if isinstance(path, str):
            path = path.encode("utf-8")
        return cls._from_objptr(rustcall(lib.symbolic_bcsymbolmap_open, path))


class UuidMapping(RustObject):
    """Object representing a mapping from one DebugID to another."""

    __dealloc_func__ = lib.symbolic_uuidmapping_free

    @classmethod
    def from_plist(cls, debug_id: str, path: str | bytes) -> UuidMapping:
        """Parses a PList."""
        if isinstance(path, str):
            path = path.encode("utf-8")
        return cls._from_objptr(
            rustcall(lib.symbolic_uuidmapping_from_plist, encode_str(debug_id), path)
        )


def id_from_breakpad(breakpad_id):
    """Converts a Breakpad CodeModuleId to DebugId"""
    if breakpad_id is None:
        return None

    s = encode_str(breakpad_id)
    id = rustcall(lib.symbolic_id_from_breakpad, s)
    return decode_str(id, free=True)


def normalize_code_id(code_id):
    """Normalizes a code identifier to default representation"""
    if code_id is None:
        return None

    s = encode_str(code_id)
    id = rustcall(lib.symbolic_normalize_code_id, s)
    return decode_str(id, free=True)


@overload
def normalize_debug_id(debug_id: None) -> None:
    ...


@overload
def normalize_debug_id(debug_id: str) -> str:
    ...


def normalize_debug_id(debug_id: str | None) -> str | None:
    """Normalizes a debug identifier to default representation"""
    if debug_id is None:
        return None

    s = encode_str(debug_id)
    id = rustcall(lib.symbolic_normalize_debug_id, s)
    return decode_str(id, free=True)
