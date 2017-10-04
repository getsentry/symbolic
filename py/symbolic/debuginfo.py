from symbolic._lowlevel import lib, ffi
from symbolic.utils import RustObject, rustcall, decode_str, decode_uuid
from symbolic.symcache import SymCache


__all__ = ['FatObject', 'Object']


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
        for idx in range(self.object_count):
            yield self._get_object(idx)

    def get_object(self, uuid=None, arch=None):
        """Get an object by either arch or uuid."""
        for obj in self.iter_objects():
            if obj.uuid == uuid or obj.arch == arch:
                return obj
        raise LookupError('Object not found')

    def _get_object(self, idx):
        """Returns the object at a certain index."""
        cache = getattr(self, '_objcache', None)
        if cache is None:
            cache = self._objcache = {}
        rv = cache.get(idx)
        if rv is not None:
            return rv
        ptr = self._methodcall(lib.symbolic_fatobject_get_object, idx)
        if ptr == ffi.NULL:
            raise LookupError('No object #%d' % idx)
        rv = cache[idx] = Object._from_objptr(ptr)
        return rv


class Object(RustObject):
    __dealloc_func__ = lib.symbolic_object_free

    @property
    def arch(self):
        """The architecture of the object."""
        # make it an ascii bytestring on 2.x
        return str(decode_str(self._methodcall(lib.symbolic_object_get_arch)))

    @property
    def uuid(self):
        """The UUID of the object."""
        return decode_uuid(self._methodcall(lib.symbolic_object_get_uuid))

    def make_symcache(self):
        """Creates a symcache from the object."""
        return SymCache._from_objptr(self._methodcall(
            lib.symbolic_symcache_from_object))

    def __repr__(self):
        return '<Object %s %r>' % (
            self.uuid,
            self.arch,
        )
