"""Minidump processing"""

import io
import shutil

from symbolic._compat import range_type
from symbolic._lowlevel import lib, ffi
from symbolic.utils import RustObject, rustcall, attached_refs, decode_uuid, encode_path, \
    encode_uuid, CacheReader

__all__ = ['CallStack', 'FrameInfoMap', 'FrameTrust', 'ProcessState', 'StackFrame']


def _make_frame_trust():
    enums = {}
    by_value = {}

    for attr in dir(lib):
        if attr.startswith('SYMBOLIC_FRAME_TRUST_'):
            name = attr[21:].lower().replace('_', '-')
            value = getattr(lib, attr)
            enums[name] = value
            by_value[value] = name

    enums['by_value'] = by_value
    return type('FrameTrust', (), enums)


FrameTrust = _make_frame_trust()
del _make_frame_trust


class StackFrame(RustObject):
    """A single frame in the call stack of a crashed process"""
    __dealloc_func__ = None

    @property
    def instruction(self):
        """The frame's program counter location as absolute virtual address"""
        return self._objptr.instruction

    @property
    def trust(self):
        """The confidence with with the instruction pointer was retrieved"""
        return FrameTrust.by_value[self._objptr.trust]

    @property
    def image_uuid(self):
        """UUID of the loaded module containing the instruction"""
        return decode_uuid(self._objptr.image_uuid)

    @property
    def image_addr(self):
        """Address at which the module is loaded in virtual memory"""
        return self._objptr.image_addr

    @property
    def image_size(self):
        """Size of the loaded module in virtual memory"""
        return self._objptr.image_size


class CallStack(RustObject):
    """A thread of the crashed process"""
    __dealloc_func__ = None

    @property
    def thread_id(self):
        """The id of the thread"""
        return self._objptr.thread_id

    @property
    def frame_count(self):
        """The size of the call stack of this thread"""
        return self._objptr.frame_count

    def frames(self):
        """An iterator over all frames in this call stack"""
        for idx in range_type(self.frame_count):
            yield self.get_frame(idx)

    def get_frame(self, idx):
        """Retrieves the stack frame at the given index (0 is the current frame)"""
        if idx < self.frame_count:
            frame = StackFrame._from_objptr(self._objptr.frames[idx], shared=True)
            attached_refs[frame] = self
            return frame
        else:
            raise IndexError("index %d out of bounds %d" % (idx, self.frame_count))


class ProcessState(RustObject):
    """State of a crashed process"""
    __dealloc_func__ = lib.symbolic_process_state_free

    @classmethod
    def from_minidump(cls, path, frame_infos=None):
        """Processes a minidump and get the state of the crashed process"""
        frame_infos_ptr = frame_infos._objptr if frame_infos is not None else ffi.NULL
        return ProcessState._from_objptr(
            rustcall(lib.symbolic_process_minidump, encode_path(path), frame_infos_ptr))

    @property
    def thread_count(self):
        """The number of threads that were running in the crashed process"""
        return self._objptr.thread_count

    def threads(self):
        """An iterator over all threads that were running in the crashed process"""
        for idx in range_type(self.thread_count):
            yield self.get_thread(idx)

    def get_thread(self, idx):
        """Retrieves the thread with the specified index"""
        if idx < self.thread_count:
            stack = CallStack._from_objptr(self._objptr.threads[idx], shared=True)
            attached_refs[stack] = self
            return stack
        else:
            raise IndexError("index %d out of bounds %d" % (idx, self.thread_count))


class FrameInfoMap(RustObject):
    """Stack frame information (CFI) for images"""
    __dealloc_func__ = lib.symbolic_frame_info_map_free

    @classmethod
    def new(cls):
        """Creates a new, empty frame info map"""
        return FrameInfoMap._from_objptr(
            rustcall(lib.symbolic_frame_info_map_new))

    def add(self, uuid, path):
        """Adds CFI for a code module specified by the `uuid` argument"""
        self._methodcall(lib.symbolic_frame_info_map_add,
            encode_uuid(uuid), encode_path(path))


class CfiCache(RustObject):
    """A cache for call frame information (CFI) to improve minidump stackwalking"""
    __dealloc_func__ = lib.symbolic_cfi_cache_free

    def open_stream(self):
        """Returns the underlying bytes of the cache."""
        buf = ffi.buffer(self._objptr.bytes, self._objptr.len)
        return io.BufferedReader(CacheReader(buf, self))

    def write_to(self, f):
        """Writes the symcache into a file object."""
        shutil.copyfileobj(self.open_stream(), f)
