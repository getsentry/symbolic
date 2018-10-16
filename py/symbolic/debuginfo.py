import bisect
from weakref import WeakValueDictionary

from symbolic._compat import itervalues, range_type
from symbolic._lowlevel import lib, ffi
from symbolic.utils import RustObject, rustcall, decode_str, encode_str, attached_refs
from symbolic.common import parse_addr, arch_is_known, arch_from_macho
from symbolic.symcache import SymCache
from symbolic.minidump import CfiCache


__all__ = [
    'FatObject',
    'Object',
    'ObjectLookup',
    'id_from_breakpad',
    'normalize_debug_id',
]


class FatObject(RustObject):
    __dealloc_func__ = lib.symbolic_fatobject_free

    @classmethod
    def from_path(self, path):
        """Opens a fat object from a given path."""
        return FatObject._from_objptr(
            rustcall(lib.symbolic_fatobject_open, path))

    @property
    def object_count(self):
        """The number of objects in this fat object."""
        return self._methodcall(lib.symbolic_fatobject_object_count)

    def iter_objects(self):
        """Iterates over all objects."""
        for idx in range_type(self.object_count):
            try:
                yield self._get_object(idx)
            except LookupError:
                pass

    def get_object(self, id=None, arch=None):
        """Get an object by either arch or id."""
        if id is not None:
            id = id.lower()
        for obj in self.iter_objects():
            if obj.id == id or obj.arch == arch:
                return obj
        raise LookupError('Object not found')

    def _get_object(self, idx):
        """Returns the object at a certain index."""
        cache = getattr(self, '_objcache', None)
        if cache is None:
            cache = self._objcache = WeakValueDictionary()
        rv = cache.get(idx)
        if rv is not None:
            return rv
        ptr = self._methodcall(lib.symbolic_fatobject_get_object, idx)
        if ptr == ffi.NULL:
            raise LookupError('No object #%d' % idx)
        rv = cache[idx] = Object._from_objptr(ptr)
        # Hold a reference here so that we don't crash if the fat object
        # is not held otherwise
        attached_refs[rv] = self
        return rv


class Object(RustObject):
    __dealloc_func__ = lib.symbolic_object_free

    @property
    def arch(self):
        """The architecture of the object."""
        # make it an ascii bytestring on 2.x
        return str(decode_str(self._methodcall(lib.symbolic_object_get_arch)))

    @property
    def id(self):
        """The unique ID of the object."""
        return decode_str(self._methodcall(lib.symbolic_object_get_id))

    @property
    def kind(self):
        """The object kind."""
        return str(decode_str(self._methodcall(lib.symbolic_object_get_kind)))

    @property
    def type(self):
        """The object type."""
        return str(decode_str(self._methodcall(lib.symbolic_object_get_type)))

    @property
    def debug_kind(self):
        """The kind of debug information in this object."""
        return str(decode_str(self._methodcall(lib.symbolic_object_get_debug_kind)))

    @property
    def features(self):
        """The list of features offered by this debug file."""
        struct = self._methodcall(lib.symbolic_object_get_features)
        features = set(decode_str(struct.data[i]) for i in range(0, struct.len))
        rustcall(lib.symbolic_object_features_free, ffi.addressof(struct))
        return frozenset(features)

    def make_symcache(self):
        """Creates a symcache from the object."""
        return SymCache._from_objptr(self._methodcall(
            lib.symbolic_symcache_from_object))

    def make_cficache(self):
        """Creates a cficache from the object."""
        return CfiCache._from_objptr(self._methodcall(
            lib.symbolic_cficache_from_object))

    def __repr__(self):
        return '<Object %s %r>' % (
            self.id,
            self.arch,
        )


class ObjectRef(object):
    """Holds a reference to an object in a format."""

    def __init__(self, data):
        self.addr = parse_addr(data['image_addr'])
        # not a real address but why handle it differently
        self.size = parse_addr(data['image_size'])
        self.vmaddr = data.get('image_vmaddr')
        self.id = normalize_debug_id(
            data.get('id') or data.get('uuid') or None)
        if data.get('arch') is not None and arch_is_known(data['arch']):
            self.arch = data['arch']
        elif data.get('cpu_type') is not None \
                and data.get('cpu_subtype') is not None:
            self.arch = arch_from_macho(data['cpu_type'],
                                        data['cpu_subtype'])
        else:
            self.arch = None
        self.name = data.get('name')

    def __repr__(self):
        return '<ObjectRef %s %r>' % (
            self.id,
            self.arch,
        )


class ObjectLookup(object):
    """Helper to look up objects based on the info a client provides."""

    def __init__(self, objects):
        self._addresses = []
        self._by_addr = {}
        self.objects = {}
        for ref_data in objects:
            obj = ObjectRef(ref_data)
            self._addresses.append(obj.addr)
            self._by_addr[obj.addr] = obj
            self.objects[obj.id] = obj
        self._addresses.sort()

    def iter_objects(self):
        """Iterates over all objects."""
        return itervalues(self.objects)

    def get_ids(self):
        """Returns a list of ids."""
        return sorted(self.objects)

    def iter_ids(self):
        """Iterates over all ids."""
        return iter(self.objects)

    def find_object(self, addr):
        """Given an instruction address this locates the image this address
        is contained in.
        """
        idx = bisect.bisect_left(self._addresses, parse_addr(addr))
        if idx > 0:
            rv = self._by_addr[self._addresses[idx - 1]]
            if not rv.size or addr < rv.addr + rv.size:
                return rv

    def get_object(self, id):
        """Finds an object by the given id."""
        return self.objects.get(id)


def id_from_breakpad(breakpad_id):
    """Converts a Breakpad CodeModuleId to DebugId"""
    if breakpad_id is None:
        return None

    s = encode_str(breakpad_id)
    id = rustcall(lib.symbolic_id_from_breakpad, s)
    return decode_str(id)


def normalize_debug_id(debug_id):
    """Normalizes a debug identifier to default representation"""
    if debug_id is None:
        return None

    s = encode_str(debug_id)
    id = rustcall(lib.symbolic_normalize_debug_id, s)
    return decode_str(id)
