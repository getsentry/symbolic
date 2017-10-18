import os
import uuid
from symbolic import ProguardMappingView


def test_basics(res_path):
    with open(os.path.join(res_path, 'proguard.txt'), 'rb') as f:
        mapping = f.read()

    view = ProguardMappingView.from_bytes(mapping)
    assert view.has_line_info
    assert view.uuid == uuid.UUID('5cd8e873-1127-5276-81b7-8ff25043ecfd')

    assert view.lookup('android.support.constraint.ConstraintLayout$a') \
        == 'android.support.constraint.ConstraintLayout$LayoutParams'

    assert view.lookup('android.support.constraint.a.b:a', 116) \
        == 'android.support.constraint.solver.ArrayRow:createRowDefinition'


def test_mmap(res_path):
    view = ProguardMappingView.from_path(os.path.join(res_path, 'proguard.txt'))
    assert view.has_line_info
    assert view.uuid == uuid.UUID('5cd8e873-1127-5276-81b7-8ff25043ecfd')

    assert view.lookup('android.support.constraint.ConstraintLayout$a') \
        == 'android.support.constraint.ConstraintLayout$LayoutParams'

    assert view.lookup('android.support.constraint.a.b:a', 116) \
        == 'android.support.constraint.solver.ArrayRow:createRowDefinition'
