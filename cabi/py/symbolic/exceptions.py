from symbolic._compat import PY2
from symbolic._lowlevel import lib


exceptions_by_code = {}


class SymbolicError(Exception):
    code = None

    def __init__(self, msg):
        Exception.__init__(self)
        self.message = msg

    def __str__(self):
        return self.message

    if PY2:
        __unicode__ = __str__
        def __str__(self):
            return unicode(self).encode('utf-8')


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


_make_exceptions()
