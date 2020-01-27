from symbolic._compat import implements_to_string
from symbolic._lowlevel import lib


__all__ = ["SymbolicError"]
exceptions_by_code = {}


@implements_to_string
class SymbolicError(Exception):
    code = None

    def __init__(self, msg):
        Exception.__init__(self)
        self.message = msg
        self.rust_info = None

    def __str__(self):
        rv = self.message
        if self.rust_info is not None:
            return u"%s\n\n%s" % (rv, self.rust_info)
        return rv


def _make_error(error_name, base=SymbolicError, code=None):
    class Exc(base):
        pass

    Exc.__name__ = Exc.__qualname__ = error_name
    if code is not None:
        Exc.code = code
    globals()[Exc.__name__] = Exc
    __all__.append(Exc.__name__)
    return Exc


def _get_error_base(error_name):
    pieces = error_name.split("Error", 1)
    if len(pieces) == 2 and pieces[0] and pieces[1]:
        base_error_name = pieces[0] + "Error"
        base_class = globals().get(base_error_name)
        if base_class is None:
            base_class = _make_error(base_error_name)
        return base_class
    return SymbolicError


def _make_exceptions():
    for attr in dir(lib):
        if not attr.startswith("SYMBOLIC_ERROR_CODE_"):
            continue

        error_name = attr[20:].title().replace("_", "")
        base = _get_error_base(error_name)
        exc = _make_error(error_name, base=base, code=getattr(lib, attr))
        exceptions_by_code[exc.code] = exc


_make_exceptions()
