"""Minidump processing"""

from symbolic._compat import range_type
from symbolic._lowlevel import lib, ffi
from symbolic.utils import RustObject, rustcall, attached_refs, decode_uuid

__all__ = ['CallStack', 'FrameTrust', 'ProcessState', 'StackFrame']


def _make_frame_trust():
    enums = {}
    for attr in dir(lib):
        if not attr.startswith('SYMBOLIC_FRAME_TRUST_'):
            continue

        name = attr[21:].title()
        value = getattr(lib, attr)
        enums[name] = value

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
        return self._objptr.trust

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
            frame = StackFrame._from_objptr(self._objptr[idx], shared=True)
            attached_refs[frame] = self
            return frame
        else:
            raise IndexError("index %d out of bounds %d" % (idx, self.frame_count))


class ProcessState(RustObject):
    """State of a crashed process"""
    __dealloc_func__ = lib.symbolic_process_state_free

    @classmethod
    def from_minidump(cls, path, cfis=ffi.NULL):
        """Processes a minidump and get the state of the crashed process"""
        return ProcessState._from_objptr(
            rustcall(lib.symbolic_process_minidump, path, cfis))

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
