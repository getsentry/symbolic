from symbolic import FatObject

#fo = FatObject.from_path('/Users/mitsuhiko/Development/symbolic/py/tests/res/ext/1.4.1/release/dSYMs/F9C4433B-260E-32C9-B5BB-ED10D8D591C3.dSYM/Contents/Resources/DWARF/CrashLibiOS')
fo = FatObject.from_path('/tmp/88ee46a9-a205-33a8-aa38-7fd10405f318')
o = fo.get_object(arch='arm64')
c = o.make_symcache()
