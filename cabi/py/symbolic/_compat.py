import sys


PY2 = sys.version_info[0] == 2

if PY2:
    text_type = unicode
    NUL = '\x00'
else:
    text_type = str
    NUL = 0
