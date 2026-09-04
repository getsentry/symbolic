# ARM32 empty `.eh_frame`

This fixture reproduces an ARM32 executable with split debug information where the executable contains an
`eh_frame` section only containing a terminator.

Run `./build.sh` to regenerate `executable` and `debuginfo`.

This is a fixture for [SYMBOLICATOR-2025](https://github.com/getsentry/symbolicator/issues/2025).
