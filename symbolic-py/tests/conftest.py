import os
import pytest

@pytest.fixture(scope="module")
def res_path():
    here = os.path.abspath(os.path.dirname(__file__))
    return os.path.join(here, "res")
