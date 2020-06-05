import os
import uuid
from symbolic import ProguardMappingView, ProguardMapper, JavaStackFrame


def test_basics(res_path):
    with open(os.path.join(res_path, "proguard.txt"), "rb") as f:
        mapping = f.read()

    view = ProguardMappingView.from_bytes(mapping)
    assert view.has_line_info
    assert view.uuid == uuid.UUID("a48ca62b-df26-544e-a8b9-2a5ce210d1d5")

    assert (
        view.lookup("android.support.constraint.ConstraintLayout$a")
        == "android.support.constraint.ConstraintLayout$LayoutParams"
    )

    assert (
        view.lookup("android.support.constraint.a.b:a", 116)
        == "android.support.constraint.solver.ArrayRow:createRowDefinition"
    )


def test_mmap(res_path):
    view = ProguardMappingView.open(os.path.join(res_path, "proguard.txt"))
    assert view.has_line_info
    assert view.uuid == uuid.UUID("a48ca62b-df26-544e-a8b9-2a5ce210d1d5")

    assert (
        view.lookup("android.support.constraint.ConstraintLayout$a")
        == "android.support.constraint.ConstraintLayout$LayoutParams"
    )

    assert (
        view.lookup("android.support.constraint.a.b:a", 116)
        == "android.support.constraint.solver.ArrayRow:createRowDefinition"
    )


def test_mapper(res_path):
    mapper = ProguardMapper.open(os.path.join(res_path, "proguard.txt"))
    assert mapper.has_line_info
    assert mapper.uuid == uuid.UUID("a48ca62b-df26-544e-a8b9-2a5ce210d1d5")

    assert (
        mapper.remap_class("android.support.constraint.ConstraintLayout$a")
        == "android.support.constraint.ConstraintLayout$LayoutParams"
    )

    remapped = mapper.remap_frame("android.support.constraint.a.b", "a", 116)
    assert len(remapped) == 1
    assert remapped[0].class_name == "android.support.constraint.solver.ArrayRow"
    assert remapped[0].method == "createRowDefinition"
    assert remapped[0].line == 116

    remapped = mapper.remap_frame("io.sentry.sample.MainActivity", "a", 1)
    assert len(remapped) == 3
    assert remapped[0].method == "bar"
    assert remapped[0].line == 54
    assert remapped[1].method == "foo"
    assert remapped[1].line == 44
    assert remapped[2].method == "onClickHandler"
    assert remapped[2].line == 40
