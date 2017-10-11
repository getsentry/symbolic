from symbolic._compat import implements_to_string
from symbolic._lowlevel import lib


__all__ = ['SymbolicError']
exceptions_by_code = {}


@implements_to_string
class SymbolicError(Exception):
    code = None

    def __init__(self, msg):
        Exception.__init__(self)
        self.message = msg
        self.panic_info = None

    def __str__(self):
        rv = self.message
        if self.panic_info is not None:
            return u'%s\n\n%s' % (rv, self.panic_info)
        return rv


def _make_exceptions():
    for attr in dir(lib):
        if not attr.startswith('SYMBOLIC_ERROR_CODE_'):
            continue

        class Exc(SymbolicError):
            pass

        Exc.__name__ = attr[20:].title().replace('_', '')
        Exc.code = getattr(lib, attr)
        globals()[Exc.__name__] = Exc
        exceptions_by_code[Exc.code] = Exc
        __all__.append(Exc.__name__)


_make_exceptions()
