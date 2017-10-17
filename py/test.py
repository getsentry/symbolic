import time
from contextlib import contextmanager
from symbolic import FatObject, SymCache


@contextmanager
def timed_section(sect):
    start = time.time()
    try:
        yield
    finally:
        print '%s: %.4fs' % (sect, time.time() - start)


with timed_section('generate symcache'):
    fo = FatObject.from_path('/tmp/88ee46a9-a205-33a8-aa38-7fd10405f318')
    o = fo.get_object(arch='arm64')
    print 'object kind:', o.kind
    cache = fo.get_object(arch='arm64').make_symcache()
    with open('/tmp/testcache', 'wb') as f:
        cache.dump(f)

with timed_section('look from symcache'):
    cache = SymCache.from_path('/tmp/testcache')
    for item in cache.lookup(53344):
        print '>', item

    print 'latest cache file:', cache.is_latest_file_format
