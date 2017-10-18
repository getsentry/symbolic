import os
import uuid

from symbolic import ProcessState


def test_macos_without_cfi(res_path):
    path = os.path.join(res_path, 'minidump', 'crash_macos.dmp')
    state = ProcessState.from_minidump(path)
    assert state.thread_count == 1

    thread = state.get_thread(0)
    assert thread.thread_id == 775
    assert thread.frame_count == 9

    frame = thread.get_frame(1)
    assert frame.trust == 'scan'
    assert frame.instruction == 4329952133
    assert frame.image_addr == 4329947136
    assert frame.image_size == 172032
    assert frame.image_uuid == uuid.UUID(
        "3f58bc3d-eabe-3361-b5fb-52a676298598")


def test_linux_without_cfi(res_path):
    path = os.path.join(res_path, 'minidump', 'crash_linux.dmp')
    state = ProcessState.from_minidump(path)
    assert state.thread_count == 1

    thread = state.get_thread(0)
    assert thread.thread_id == 133
    assert thread.frame_count == 18

    frame = thread.get_frame(1)
    assert frame.trust == 'scan'
    assert frame.instruction == 4202617
    assert frame.image_addr == 4194304
    assert frame.image_size == 196608
    assert frame.image_uuid == uuid.UUID(
        "d2554cdb-9261-36c4-b976-6a086583b9b5")
