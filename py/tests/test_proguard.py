import os
import uuid
from symbolic import ProguardMappingView, ProguardMapper, JavaStackFrame


def test_basics(res_path):
    with open(os.path.join(res_path, "proguard.txt"), "rb") as f:
        mapping = f.read()

    view = ProguardMappingView.from_bytes(mapping)
    assert view.has_line_info
    assert view.uuid == uuid.UUID("5cd8e873-1127-5276-81b7-8ff25043ecfd")

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
    assert view.uuid == uuid.UUID("5cd8e873-1127-5276-81b7-8ff25043ecfd")

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
    assert mapper.uuid == uuid.UUID("5cd8e873-1127-5276-81b7-8ff25043ecfd")

    assert (
        mapper.remap_class("android.support.constraint.ConstraintLayout$a")
        == "android.support.constraint.ConstraintLayout$LayoutParams"
    )

    remapped = mapper.remap_frame("android.support.constraint.a.b", "a", 116)
    assert len(remapped) == 1
    assert remapped[0].class_name == "android.support.constraint.solver.ArrayRow"
    assert remapped[0].method == "createRowDefinition"
    assert remapped[0].line == 116
