"""Minidump processing"""

import io
import shutil
from datetime import datetime

from symbolic._compat import range_type
from symbolic._lowlevel import lib, ffi
from symbolic.utils import RustObject, rustcall, attached_refs, encode_path, \
    encode_str, decode_str, CacheReader

__all__ = ['CallStack', 'FrameInfoMap', 'FrameTrust', 'ProcessState',
           'StackFrame', 'CfiCache']


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


class CodeModule(RustObject):
    """Carries information about a code module loaded into the crashed process"""
    __dealloc_func__ = None

    @property
    def id(self):
        """ID of the loaded module containing the instruction"""
        return decode_str(self._objptr.id) or None

    @property
    def addr(self):
        """Address at which the module is loaded in virtual memory"""
        return self._objptr.addr

    @property
    def size(self):
        """Size of the loaded module in virtual memory"""
        return self._objptr.size

    @property
    def name(self):
        """File name of the loaded module's debug file"""
        return decode_str(self._objptr.name)


class StackFrame(RustObject):
    """A single frame in the call stack of a crashed process"""
    __dealloc_func__ = None

    @property
    def return_address(self):
        """The frame's return address as saved in registers or on the stack"""
        return self._objptr.return_address

    @property
    def instruction(self):
        """The frame's program counter location as absolute virtual address"""
        return self._objptr.instruction

    @property
    def trust(self):
        """The confidence with with the instruction pointer was retrieved"""
        return FrameTrust.by_value[self._objptr.trust]

    @property
    def module(self):
        """The code module that defines code for this frame"""
        module = CodeModule._from_objptr(self._objptr.module, shared=True)
        if not module.id and not module.addr and not module.size:
            return None

        attached_refs[module] = self
        return module

    @property
    def registers(self):
        if hasattr(self, '_registers'):
            return self._registers

        self._registers = {}
        for idx in range_type(self._objptr.register_count):
            register = self._objptr.registers[idx]
            name = decode_str(register.name)
            value = decode_str(register.value)
            self._registers[name] = value

        return self._registers


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
            frame = StackFrame._from_objptr(
                self._objptr.frames[idx], shared=True)
            attached_refs[frame] = self
            return frame
        else:
            raise IndexError("index %d out of bounds %d" %
                             (idx, self.frame_count))


class SystemInfo(RustObject):
    """Information about the CPU and OS on which a minidump was generated."""
    __dealloc_func__ = None

    @property
    def os_name(self):
        """A string identifying the operating system, such as "Windows NT",
        "Mac OS X", or "Linux".  If the information is present in the dump but
        its value is unknown, this field will contain a numeric value.  If
        the information is not present in the dump, this field will be empty.
        """
        return decode_str(self._objptr.os_name)

    @property
    def os_version(self):
        """A string identifying the version of the operating system, such as
        "5.1.2600" or "10.4.8".  The version will be formatted as three-
        component semantic version.  If the dump does not contain this
        information, this field will contain "0.0.0"."""
        return decode_str(self._objptr.os_version)

    @property
    def os_build(self):
        """A string identifying the build of the operating system, such as
        "Service Pack 2" or "8L2127".  If the dump does not contain this
        information, this field will be empty."""
        return decode_str(self._objptr.os_build)

    @property
    def cpu_family(self):
        """A string identifying the basic CPU family, such as "x86" or "ppc".
        If this information is present in the dump but its value is unknown,
        this field will contain a numeric value.  If the information is not
        present in the dump, this field will be empty.  The values stored in
        this field should match those used by MinidumpSystemInfo::GetCPU."""
        return decode_str(self._objptr.cpu_family)

    @property
    def cpu_info(self):
        """A string further identifying the specific CPU, such as
        "GenuineIntel level 6 model 13 stepping 8".  If the information is not
        present in the dump, or additional identifying information is not
        defined for the CPU family, this field will be empty."""
        return decode_str(self._objptr.cpu_info)

    @property
    def cpu_count(self):
        """The number of processors in the system.  Will be greater than one for
        multi-core systems."""
        return self._objptr.cpu_count


class ProcessState(RustObject):
    """State of a crashed process"""
    __dealloc_func__ = lib.symbolic_process_state_free

    @classmethod
    def from_minidump(cls, path, frame_infos=None):
        """Processes a minidump and get the state of the crashed process"""
        frame_infos_ptr = frame_infos._objptr if frame_infos is not None else ffi.NULL
        return ProcessState._from_objptr(
            rustcall(lib.symbolic_process_minidump, encode_path(path), frame_infos_ptr))

    @classmethod
    def from_minidump_buffer(cls, buffer, frame_infos=None):
        """Processes a minidump and get the state of the crashed process"""
        frame_infos_ptr = frame_infos._objptr if frame_infos is not None else ffi.NULL
        return ProcessState._from_objptr(rustcall(
            lib.symbolic_process_minidump_buffer,
            ffi.from_buffer(buffer),
            len(buffer),
            frame_infos_ptr,
        ))

    @property
    def requesting_thread(self):
        """The index of the thread that requested a dump be written in the
        threads vector.  If a dump was produced as a result of a crash, this
        will point to the thread that crashed.  If the dump was produced as
        by user code without crashing, and the dump contains extended Breakpad
        information, this will point to the thread that requested the dump.
        If the dump was not produced as a result of an exception and no
        extended Breakpad information is present, this field will be set to -1,
        indicating that the dump thread is not available."""
        return self._objptr.requesting_thread

    @property
    def timestamp(self):
        """The time-date stamp of the minidump (time_t format)"""
        return self._objptr.timestamp

    @property
    def crashed(self):
        """True if the process crashed, false if the dump was produced outside
        of an exception handler."""
        return self._objptr.crashed

    @property
    def crash_time(self):
        """The UTC time at which the process crashed"""
        if self.timestamp == 0:
            return None
        return datetime.utcfromtimestamp(float(self.timestamp))

    @property
    def crash_address(self):
        """If the process crashed, and if crash_reason implicates memory,
        the memory address that caused the crash.  For data access errors,
        this will be the data address that caused the fault.  For code errors,
        this will be the address of the instruction that caused the fault."""
        return self._objptr.crash_address

    @property
    def crash_reason(self):
        """If the process crashed, the type of crash.  OS- and possibly CPU-
        specific.  For example, "EXCEPTION_ACCESS_VIOLATION" (Windows),
        "EXC_BAD_ACCESS / KERN_INVALID_ADDRESS" (Mac OS X), "SIGSEGV"
        (other Unix)."""
        return decode_str(self._objptr.crash_reason)

    @property
    def assertion(self):
        """If there was an assertion that was hit, a textual representation
        of that assertion, possibly including the file and line at which
        it occurred."""
        return decode_str(self._objptr.assertion)

    @property
    def system_info(self):
        """Returns a weak pointer to OS and CPU information."""
        info = SystemInfo._from_objptr(self._objptr.system_info, shared=True)
        attached_refs[info] = self
        return info

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
            stack = CallStack._from_objptr(
                self._objptr.threads[idx], shared=True)
            attached_refs[stack] = self
            return stack
        else:
            raise IndexError("index %d out of bounds %d" %
                             (idx, self.thread_count))

    @property
    def module_count(self):
        return self._objptr.module_count

    def modules(self):
        for idx in range_type(self.module_count):
            yield self.get_module(idx)

    def get_module(self, idx):
        if idx < self.module_count:
            module = CodeModule._from_objptr(
                self._objptr.modules[idx], shared=True)
            attached_refs[module] = self
            return module
        else:
            raise IndexError("index %d out of bounds %d" %
                             (idx, self.module_count))


class FrameInfoMap(RustObject):
    """Stack frame information (CFI) for images"""
    __dealloc_func__ = lib.symbolic_frame_info_map_free

    @classmethod
    def new(cls):
        """Creates a new, empty frame info map"""
        return FrameInfoMap._from_objptr(
            rustcall(lib.symbolic_frame_info_map_new))

    def add(self, id, path):
        """Adds CFI for a code module specified by the `id` argument"""
        self._methodcall(lib.symbolic_frame_info_map_add,
                         encode_str(id), encode_path(path))


# The most recent version for the CFI cache file format
CFICACHE_LATEST_VERSION = rustcall(lib.symbolic_cficache_latest_version)


class CfiCache(RustObject):
    """A cache for call frame information (CFI) to improve native stackwalking"""
    __dealloc_func__ = lib.symbolic_cficache_free

    @classmethod
    def from_path(cls, path):
        """Loads a cficache from a file via mmap."""
        return cls._from_objptr(
            rustcall(lib.symbolic_cficache_from_path, encode_path(path)))

    @classmethod
    def from_object(cls, obj):
        """Creates a cficache from the given object."""
        return cls._from_objptr(
            rustcall(lib.symbolic_cficache_from_object, obj._get_objptr()))

    @property
    def file_format_version(self):
        """Version of the file format."""
        return self._methodcall(lib.symbolic_cficache_get_version)

    @property
    def is_latest_file_format(self):
        """Returns true if this is the latest file format."""
        return self.file_format_version >= CFICACHE_LATEST_VERSION

    def open_stream(self):
        """Returns a stream to read files from the internal buffer."""
        buf = self._methodcall(lib.symbolic_cficache_get_bytes)
        size = self._methodcall(lib.symbolic_cficache_get_size)
        return io.BufferedReader(CacheReader(ffi.buffer(buf, size), self))

        buf = ffi.buffer(self._objptr.bytes, self._objptr.len)
        return io.BufferedReader(CacheReader(buf, self))

    def write_to(self, f):
        """Writes the CFI cache into a file object."""
        shutil.copyfileobj(self.open_stream(), f)
