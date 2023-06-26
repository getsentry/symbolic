import pytest

from symbolic.common import arch_is_known, normalize_arch, parse_addr
from symbolic.exceptions import UnknownArchError


def test_arch_is_known():
    # Generic (MachO naming convention)
    assert arch_is_known("x86")
    assert arch_is_known("x86_64")
    assert arch_is_known("x86_64h")

    # Breakpad specific
    assert arch_is_known("amd64")

    # Unknown and invalid
    assert not arch_is_known("foo")
    assert not arch_is_known(None)  # type: ignore[arg-type]
    assert not arch_is_known(42)  # type: ignore[arg-type]


def test_normalize_arch():
    # Generic (MachO naming convention)
    assert normalize_arch("x86") == "x86"
    assert normalize_arch("x86_64") == "x86_64"
    assert normalize_arch("x86_64h") == "x86_64h"

    # Breakpad specific
    assert normalize_arch("amd64") == "x86_64"

    # Unknown and invalid
    assert normalize_arch(None) is None
    with pytest.raises(UnknownArchError):
        normalize_arch("foo")
    with pytest.raises(ValueError):
        normalize_arch(42)  # type: ignore[call-overload]


def test_parse_addr():
    assert parse_addr(None) == 0
    assert parse_addr(4096) == 0x1000
    assert parse_addr("4096") == 0x1000
    assert parse_addr("0x1000") == 0x1000

    with pytest.raises(ValueError):
        parse_addr("asdf")
