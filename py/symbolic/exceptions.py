from __future__ import annotations
from typing import TYPE_CHECKING

from symbolic._lowlevel import lib


__all__ = ["SymbolicError"]
exceptions_by_code: dict[int, type[SymbolicError]] = {}


class SymbolicError(Exception):
    if TYPE_CHECKING:
        code: int
    else:
        code = None

    def __init__(self, msg):
        Exception.__init__(self)
        self.message = msg
        self.rust_info = None

    def __str__(self) -> str:
        rv = self.message
        if self.rust_info is not None:
            return f"{rv}\n\n{self.rust_info}"
        return rv


def _make_error(
    error_name: str, base: type[SymbolicError] = SymbolicError, code: int | None = None
) -> type[SymbolicError]:
    class Exc(base):  # type: ignore[misc,valid-type]
        pass

    Exc.__name__ = Exc.__qualname__ = error_name
    if code is not None:
        Exc.code = code
    globals()[Exc.__name__] = Exc
    __all__.append(Exc.__name__)
    return Exc


def _get_error_base(error_name: str) -> type[SymbolicError]:
    pieces = error_name.split("Error", 1)
    if len(pieces) == 2 and pieces[0] and pieces[1]:
        base_error_name = pieces[0] + "Error"
        base_class = globals().get(base_error_name)
        if base_class is None:
            base_class = _make_error(base_error_name)
        return base_class
    return SymbolicError


def _make_exceptions() -> None:
    for attr in dir(lib):
        if not attr.startswith("SYMBOLIC_ERROR_CODE_"):
            continue

        error_name = attr[20:].title().replace("_", "")
        base = _get_error_base(error_name)
        exc = _make_error(error_name, base=base, code=getattr(lib, attr))
        exceptions_by_code[exc.code] = exc


_make_exceptions()

if TYPE_CHECKING:
    # treat unknown attribute names as exception types
    def __getattr__(name: str) -> type[SymbolicError]:
        ...
