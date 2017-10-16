import os
from symbolic import SourceView, SourceMapView


def test_basics(res_path):
    with open(os.path.join(res_path, 'demo.min.js'), 'rb') as f:
        minified_source = SourceView.from_bytes(f.read())
    with open(os.path.join(res_path, 'demo.js.map'), 'rb') as f:
        sourcemap = SourceMapView.from_json_bytes(f.read())

    locs = [
        (0, 107, "e", "onFailure", 11, 10),
        (0, 179, "i", "invoke", 21, 4),
        (0, 226, "u", "test", 26, 4),
    ]

    for line, col, minified_name, original_name, src_line, src_col in locs:
        tok = sourcemap.lookup(line, col, minified_name, minified_source)
        assert tok is not None
        assert tok.src_line == src_line
        assert tok.src_col == src_col
        assert tok.function_name == original_name

    for line, col, minified_name, original_name, src_line, src_col in locs:
        tok = sourcemap.lookup(line, col)
        assert tok is not None
        assert tok.src_line == src_line
        assert tok.src_col == src_col
        assert tok.function_name is None
