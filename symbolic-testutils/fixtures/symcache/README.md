# Symcache Fixtures

## Current Fixtures

The current symcache files are generated from the following files:

| Cache | Source debug file |
| --- | --- |
| `linux.symc` | `symbolic-testutils/fixtures/linux/crash.debug` |
| `macos.symc` | `symbolic-testutils/fixtures/macos/crash.dSYM/Contents/Resources/DWARF/crash` |

To regenerate the fixtures:

```shell
cargo run -p symcache_debug -- \
  --debug-file symbolic-testutils/fixtures/linux/crash.debug \
  --write-cache-file \
  --symcache-file symbolic-testutils/fixtures/symcache/current/linux.symc

cargo run -p symcache_debug -- \
  --debug-file symbolic-testutils/fixtures/macos/crash.dSYM/Contents/Resources/DWARF/crash \
  --write-cache-file \
  --symcache-file symbolic-testutils/fixtures/symcache/current/macos.symc
```
