from symbolic import *
#c = SymCache.from_path('/tmp/sentry-dsym-cache/3/97db1fd6-2f22-33d7-9f41-e079be222717.symcache')
c = FatObject.from_path('/tmp/test.dsym').get_object(arch='arm64').make_symcache()

for item in c.lookup(0x102a3c5b4 - 0x102a34000):
    print repr(item)
    print '  (%s)' % item.function_name
#
#print c.lookup(34224L)

#for item in c.lookup(34224L):
#    print repr(item)
#    print '  (%s)' % item.function_name
