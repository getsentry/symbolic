from __future__ import annotations
from typing import Optional, Tuple

import uuid as uuid_mod

from symbolic._lowlevel import lib, ffi
from symbolic.utils import (
    RustObject,
    rustcall,
    decode_str,
    encode_str,
    decode_uuid,
    encode_path,
)


__all__ = ["ProguardMapper", "JavaStackFrame"]


class JavaStackFrame:
    def __init__(
        self,
        class_name: str,
        method: str,
        line: int,
        file: str | None = None,
        parameters: str | None = None,
    ) -> None:
        self.class_name = class_name
        self.method = method
        self.file = file or None
        self.line = line
        self.parameters = parameters


class ProguardMapper(RustObject):
    """Gives access to proguard mapping files."""

    __dealloc_func__ = lib.symbolic_proguardmapper_free

    @classmethod
    def open(cls, path: str, initialize_param_mapping: bool = False) -> ProguardMapper:
        """Constructs a mapping file from a path."""
        return cls._from_objptr(
            rustcall(
                lib.symbolic_proguardmapper_open,
                encode_path(path),
                initialize_param_mapping,
            )
        )

    @property
    def uuid(self) -> uuid_mod.UUID:
        """Returns the UUID of the file."""
        return decode_uuid(self._methodcall(lib.symbolic_proguardmapper_get_uuid))

    @property
    def has_line_info(self) -> bool:
        """True if the file contains line information."""
        return bool(self._methodcall(lib.symbolic_proguardmapper_has_line_info))

    def remap_class(self, klass: str) -> str | None:
        """Remaps the given class name."""
        klass = self._methodcall(
            lib.symbolic_proguardmapper_remap_class, encode_str(klass)
        )
        return decode_str(klass, free=True) or None

    def remap_method(self, klass: str, method: str) -> Tuple[str, str] | None:
        """Remaps the given class name."""
        result = self._methodcall(
            lib.symbolic_proguardmapper_remap_method,
            encode_str(klass),
            encode_str(method),
        )

        try:
            output = (
                decode_str(result.frames[0].class_name, free=False),
                decode_str(result.frames[0].method, free=False),
            )
        finally:
            rustcall(lib.symbolic_proguardmapper_result_free, ffi.addressof(result))

        return output if len(output[0]) > 0 and len(output[1]) > 0 else None

    def remap_frame(
        self,
        klass: str,
        method: str,
        line: int,
        parameters: Optional[str] = None,
    ) -> list[JavaStackFrame]:
        """Remaps the stackframe, given its class, method and line."""
        result = self._methodcall(
            lib.symbolic_proguardmapper_remap_frame,
            encode_str(klass),
            encode_str(method),
            line,
            encode_str("" if parameters is None else parameters),
            parameters is not None,
        )

        frames = []
        try:
            for idx in range(result.len):
                frame = result.frames[idx]
                frames.append(
                    JavaStackFrame(
                        decode_str(frame.class_name, free=False),
                        decode_str(frame.method, free=False),
                        frame.line,
                        decode_str(frame.file, free=False),
                    )
                )
        finally:
            rustcall(lib.symbolic_proguardmapper_result_free, ffi.addressof(result))

        return frames
