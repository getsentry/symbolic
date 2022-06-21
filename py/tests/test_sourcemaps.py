import posixpath


def verify_index(index, sources):
    def get_source_line(token):
        return sources[posixpath.basename(token.src)][token.src_line]

    for token in index:
        # Ignore tokens that are None.
        # There's no simple way to verify they're correct
        if token.name is None:
            continue
        source_line = get_source_line(token)
        start = token.src_col
        end = start + len(token.name)
        substring = source_line[start:end]

        # jQuery's sourcemap has a few tokens that are identified
        # incorrectly.
        # For example, they have a token for 'embed', and
        # it maps to '"embe', which is wrong. This only happened
        # for a few strings, so we ignore
        if substring[:1] == '"':
            continue

        assert token.name == substring


def verify_token_search(index):
    for idx, token in enumerate(index):
        if not token.name:
            continue
        try:
            next_token = index[idx + 1]
            rng = range(token.dst_col, next_token.dst_col)
        except LookupError:
            rng = (token.dst_col,)
        for col in rng:
            token_match = index.lookup(token.dst_line, col)
            assert token_match == token


def test_basics(get_sourceview, get_sourcemapview):
    minified_source = get_sourceview("demo.min.js")
    sourcemap = get_sourcemapview("demo.js.map")

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

    sv = sourcemap.get_sourceview(0)
    assert sv is not None
    assert sv._shared
    assert sv[0] == "var makeAFailure = (function() {"
    assert sv[1] == "  function testingStuff() {"
    assert len(sv) == 32


def test_load_index(get_sourceview, get_sourcemapview):
    view = get_sourcemapview("indexed.sourcemap.js")
    f1 = get_sourceview("file1.js")
    f2 = get_sourceview("file2.js")
    verify_index(view, {"file1.js": f1, "file2.js": f2})
    verify_token_search(view)


def test_jquery(get_sourceview, get_sourcemapview):
    source = get_sourceview("jquery.js")
    index = get_sourcemapview("jquery.min.map")
    verify_index(index, {"jquery.js": source})


def test_coolstuff(get_sourceview, get_sourcemapview):
    source = get_sourceview("coolstuff.js")
    index = get_sourcemapview("coolstuff.min.map")
    verify_index(index, {"coolstuff.js": source})


def test_unicode_names(get_sourceview, get_sourcemapview):
    source = get_sourceview("unicode.js")
    index = get_sourcemapview("unicode.min.map")
    verify_index(index, {"unicode.js": source})


def test_react_dom(get_sourceview, get_sourcemapview):
    source = get_sourceview("react-dom.js")
    index = get_sourcemapview("react-dom.min.map")
    verify_index(index, {"react-dom.js": source})

    react_token = index.lookup(0, 319)
    assert react_token.dst_line == 0
    assert react_token.dst_col == 319
    assert react_token.src_line == 39
    assert react_token.src_col == 12
    assert react_token.src_id == 0
    assert react_token.src == "react-dom.js"
    assert react_token.name == "React"
    verify_token_search(index)


def test_source_access(get_sourcemapview):
    index = get_sourcemapview("react-dom-full.min.map")
    assert index.get_sourceview(0) is not None
    assert index.get_sourceview(1) is None


def test_wrong_rn_sourcemaps_android(get_sourceview, get_sourcemapview):
    index = get_sourcemapview("android-release.bundle.map")
    # Users need to update their jsc version for android
    # https://github.com/react-community/jsc-android-buildscripts
    # then the correct col will be reported.
    inline = index.lookup(308, 765)
    # To print found token
    # import pprint; pprint.pprint(inline.__dict__)
    function = index.lookup(308, 573)
    # To print found token
    # import pprint; pprint.pprint(inline.__dict__)

    # To print source code of file
    # print(str(index.get_sourceview(308).get_source()))
    assert inline.name == "invalidFunction"
    assert inline.src_col == 72
    assert inline.src_line == 40  # + 1

    assert function.name == "invalidFunction"
    assert function.src_col == 9
    assert function.src_line == 34  # + 1


def test_wrong_rn_sourcemaps_ios(get_sourceview, get_sourcemapview):
    index = get_sourcemapview("ios-release.bundle.map")
    inline = index.lookup(311, 765)
    # To print found token
    # import pprint; pprint.pprint(inline.__dict__)
    function = index.lookup(311, 573)
    # To print found token
    # import pprint; pprint.pprint(inline.__dict__)

    # To print source code of file
    # print(str(index.get_sourceview(311).get_source()))
    assert inline.name == "invalidFunction"
    assert inline.src_col == 72
    assert inline.src_line == 40  # + 1

    assert function.name == "invalidFunction"
    assert function.src_col == 9
    assert function.src_line == 34  # + 1


def test_react_native_hermes(get_empty_sourceview, get_sourcemapview):
    # This SourceMap is generated by the react-native + hermes pipeline.
    # See the rust-sourcemap repo for instructions on how to generate it.
    # Additionally, it was processed by sentry-cli, which basically inlines
    # all the sourceContents.
    index = get_sourcemapview("react-native-hermes.map")
    minified_source = get_empty_sourceview()
    # at foo (address at unknown:1:11940)
    token = index.lookup(0, 11939, "", minified_source)
    assert token.src_line == 1
    assert token.src_col == 10
    assert token.dst_line == 0
    assert token.dst_col == 11939
    assert token.src_id == 5
    assert token.name is None
    assert token.src == "module.js"
    assert token.function_name == "foo"

    # at anonymous (address at unknown:1:11858)
    token = index.lookup(0, 11857, "", minified_source)
    assert token.src_line == 2
    assert token.src_col == 0
    assert token.dst_line == 0
    assert token.dst_col == 11857
    assert token.src_id == 4
    assert token.name is None
    assert token.src == "input.js"
    assert token.function_name == "<global>"


def test_react_native_metro(get_sourceview, get_sourcemapview):
    # This is the SourceMap as generated by metro alone,
    # without running the code through hermes.
    index = get_sourcemapview("react-native-metro.js.map")
    minified_source = get_sourceview("react-native-metro.js")

    # e.foo (react-native-metro.js:7:101)
    token = index.lookup(6, 100, "e.foo", minified_source)
    assert token.src_line == 1
    assert token.src_col == 10
    assert token.dst_line == 6
    assert token.dst_col == 100
    assert token.src_id == 6
    assert token.name is None
    assert token.src == "module.js"
    assert token.function_name is None

    # at react-native-metro.js:6:44
    token = index.lookup(5, 43, "", minified_source)
    assert token.src_line == 2
    assert token.src_col == 0
    assert token.dst_line == 5
    assert token.dst_col == 39
    assert token.src_id == 5
    assert token.name == "foo"
    assert token.src == "input.js"
    assert token.function_name is None

    # in case we have a `metro` bundle, but a `hermes` bytecode offset (something out of range),
    # we can’t resolve this.
    assert index.lookup(0, 11857, "", minified_source) is None
