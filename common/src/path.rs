use std::borrow::Cow;

fn is_absolute_windows_path(s: &str) -> bool {
    // UNC
    if s.len() > 2 && (&s[..2] == "\\\\" || &s[..2] == "//") {
        return true;
    }

    // other paths
    let mut char_iter = s.chars();
    let (fc, sc, tc) = (char_iter.next(), char_iter.next(), char_iter.next());

    match fc.unwrap_or_default() {
        'A'..='Z' | 'a'..='z' => {
            if sc == Some(':') && tc.map_or(true, |tc| tc == '\\' || tc == '/') {
                return true;
            }
        }
        _ => (),
    }

    false
}

fn is_semi_absolute_windows_path(s: &str) -> bool {
    s.starts_with(&['/', '\\'][..])
}

fn is_absolute_unix_path(s: &str) -> bool {
    s.starts_with('/')
}

fn is_windows_path(path: &str) -> bool {
    path.contains('\\') || is_absolute_windows_path(path)
}

/// Joins paths of various platforms.
///
/// This attempts to detect Windows or Unix paths and joins with the correct directory separator.
/// Also, trailing directory separators are detected in the base string and empty paths are handled
/// correctly.
pub fn join_path(base: &str, other: &str) -> String {
    // special case for things like <stdin> or others.
    if other.starts_with('<') && other.ends_with('>') {
        return other.into();
    }

    // absolute paths
    if base == "" || is_absolute_windows_path(other) || is_absolute_unix_path(other) {
        return other.into();
    }

    // other weird cases
    if other == "" {
        return base.into();
    }

    // C:\test + \bar -> C:\bar
    if is_semi_absolute_windows_path(other) {
        if is_absolute_windows_path(base) {
            return format!("{}{}", &base[..2], other);
        } else {
            return other.into();
        }
    }

    let win_style = is_windows_path(base) || is_windows_path(other);

    if win_style {
        format!(
            "{}\\{}",
            base.trim_end_matches(&['\\', '/'][..]),
            other.trim_start_matches(&['\\', '/'][..])
        )
    } else {
        format!(
            "{}/{}",
            base.trim_end_matches('/'),
            other.trim_start_matches('/')
        )
    }
}

fn pop_path(path: &mut String) -> bool {
    if let Some(idx) = path.rfind(&['/', '\\'][..]) {
        path.truncate(idx);
        true
    } else if !path.is_empty() {
        path.truncate(0);
        true
    } else {
        false
    }
}

/// Cleans up a path from various platforms.
///
/// This removes redundant `../` or `./` references.  Since this does not resolve symlinks this
/// is a lossy operation.
pub fn clean_path(path: &str) -> String {
    let mut rv = String::with_capacity(path.len());
    let is_windows = path.contains('\\');
    let mut needs_separator = false;
    let mut is_past_root = false;

    for segment in path.split_terminator(&['/', '\\'][..]) {
        if segment == "." {
            continue;
        } else if segment == ".." {
            if !is_past_root && pop_path(&mut rv) {
                if rv.is_empty() {
                    needs_separator = false;
                }
                continue;
            } else {
                if !is_past_root {
                    needs_separator = false;
                    is_past_root = true;
                }
                if needs_separator {
                    rv.push(if is_windows { '\\' } else { '/' });
                }
                rv.push_str("..");
                needs_separator = true;
                continue;
            }
        }
        if needs_separator {
            rv.push(if is_windows { '\\' } else { '/' });
        } else {
            needs_separator = true;
        }
        rv.push_str(segment);
    }

    rv
}

/// Splits off the last component of a binary path.
///
/// The path should be a path to a file, and not a directory. If this path is a directory or the
/// root path, the result is undefined.
///
/// This attempts to detect Windows or Unix paths and split off the last component of the path
/// accordingly. Note that for paths with mixed slash and backslash separators this might not lead
/// to the desired results.
pub fn split_path_bytes(path: &[u8]) -> (Option<&[u8]>, &[u8]) {
    // Trim directory separators at the end, if any.
    let path = match path.iter().rposition(|b| *b != b'\\' && *b != b'/') {
        Some(cutoff) => &path[..=cutoff],
        None => path,
    };

    // Try to find a backslash which could indicate a Windows path.
    let split_char = if path.contains(&b'\\') {
        b'\\' // Probably Windows
    } else {
        b'/' // Probably UNIX
    };

    match path.iter().rposition(|b| *b == split_char) {
        Some(0) => (Some(&path[..1]), &path[1..]),
        Some(pos) => (Some(&path[..pos]), &path[pos + 1..]),
        None => (None, path),
    }
}

/// Splits off the last component of a path.
///
/// The path should be a path to a file, and not a directory. If this path is a directory or the
/// root path, the result is undefined.
///
/// This attempts to detect Windows or Unix paths and split off the last component of the path
/// accordingly. Note that for paths with mixed slash and backslash separators this might not lead
/// to the desired results.
pub fn split_path(path: &str) -> (Option<&str>, &str) {
    let (dir, name) = split_path_bytes(path.as_bytes());
    unsafe {
        (
            dir.map(|b| std::str::from_utf8_unchecked(b)),
            std::str::from_utf8_unchecked(name),
        )
    }
}

/// Trims a path to a given length.
///
/// This attempts to not completely destroy the path in the process by trimming off the middle path
/// segments. In the process, this tries to determine whether the path is a Windows or Unix path and
/// handle directory separators accordingly.
pub fn shorten_path(path: &str, length: usize) -> Cow<'_, str> {
    // trivial cases
    if path.len() <= length {
        return Cow::Borrowed(path);
    } else if length <= 10 {
        if length > 3 {
            return Cow::Owned(format!("{}...", &path[..length - 3]));
        }
        return Cow::Borrowed(&path[..length]);
    }

    let mut rv = String::new();
    let mut last_idx = 0;
    let mut piece_iter = path.match_indices(&['\\', '/'][..]);
    let mut final_sep = "/";
    let max_len = length - 4;

    // make sure we get two segments at the start.
    while let Some((idx, sep)) = piece_iter.next() {
        let slice = &path[last_idx..idx + sep.len()];
        rv.push_str(slice);
        let done = last_idx > 0;
        last_idx = idx + sep.len();
        final_sep = sep;
        if done {
            break;
        }
    }

    // collect the rest of the segments into a temporary we can then reverse.
    let mut final_length = rv.len() as i64;
    let mut rest = vec![];
    let mut next_idx = path.len();

    while let Some((idx, _)) = piece_iter.next_back() {
        if idx <= last_idx {
            break;
        }
        let slice = &path[idx + 1..next_idx];
        if final_length + (slice.len() as i64) > max_len as i64 {
            break;
        }

        rest.push(slice);
        next_idx = idx + 1;
        final_length += slice.len() as i64;
    }

    // if at this point already we're too long we just take the last element
    // of the path and strip it.
    if rv.len() > max_len || rest.is_empty() {
        let basename = path.rsplit(&['\\', '/'][..]).next().unwrap();
        if basename.len() > max_len {
            return Cow::Owned(format!("...{}", &basename[basename.len() - max_len + 1..]));
        } else {
            return Cow::Owned(format!("...{}{}", final_sep, basename));
        }
    }

    rest.reverse();
    rv.push_str("...");
    rv.push_str(final_sep);
    for item in rest {
        rv.push_str(&item);
    }

    Cow::Owned(rv)
}

#[test]
fn test_join_path() {
    assert_eq!(join_path("foo", "C:"), "C:");
    assert_eq!(join_path("foo", "C:bar"), "foo/C:bar");
    assert_eq!(join_path("C:\\a", "b"), "C:\\a\\b");
    assert_eq!(join_path("C:/a", "b"), "C:/a\\b");
    assert_eq!(join_path("C:\\a", "b\\c"), "C:\\a\\b\\c");
    assert_eq!(join_path("C:/a", "C:\\b"), "C:\\b");
    assert_eq!(join_path("a\\b\\c", "d\\e"), "a\\b\\c\\d\\e");
    assert_eq!(join_path("\\\\UNC\\", "a"), "\\\\UNC\\a");

    assert_eq!(join_path("C:\\foo/bar", "\\baz"), "C:\\baz");
    assert_eq!(join_path("\\foo/bar", "\\baz"), "\\baz");
    assert_eq!(join_path("/a/b", "\\c"), "\\c");

    assert_eq!(join_path("/a/b", "c"), "/a/b/c");
    assert_eq!(join_path("/a/b", "c/d"), "/a/b/c/d");
    assert_eq!(join_path("/a/b", "/c/d/e"), "/c/d/e");
    assert_eq!(join_path("a/b/", "c"), "a/b/c");

    assert_eq!(join_path("a/b/", "<stdin>"), "<stdin>");
    assert_eq!(
        join_path("C:\\test", "<::core::macros::assert_eq macros>"),
        "<::core::macros::assert_eq macros>"
    );
}

#[test]
fn test_clean_path() {
    assert_eq!(clean_path("/foo/bar/baz/./blah"), "/foo/bar/baz/blah");
    assert_eq!(clean_path("/foo/bar/baz/./blah/"), "/foo/bar/baz/blah");
    assert_eq!(clean_path("foo/bar/baz/./blah/"), "foo/bar/baz/blah");
    assert_eq!(clean_path("foo/bar/baz/../blah/"), "foo/bar/blah");
    assert_eq!(clean_path("../../blah/"), "../../blah");
    assert_eq!(clean_path("..\\../blah/"), "..\\..\\blah");
    assert_eq!(clean_path("foo\\bar\\baz/../blah/"), "foo\\bar\\blah");
    assert_eq!(clean_path("foo\\bar\\baz/../../../../blah/"), "..\\blah");
    assert_eq!(clean_path("foo/bar/baz/../../../../blah/"), "../blah");
    assert_eq!(clean_path("..\\foo"), "..\\foo");
    assert_eq!(clean_path("foo"), "foo");
    assert_eq!(clean_path("foo\\bar\\baz/../../../blah/"), "blah");
    assert_eq!(clean_path("foo/bar/baz/../../../blah/"), "blah");

    // XXX currently known broken tests:
    //assert_eq!(clean_path("/../../blah/"), "/blah");
    //assert_eq!(clean_path("c:\\..\\foo"), "c:\\foo");
}

#[test]
fn test_shorten_path() {
    assert_eq!(shorten_path("/foo/bar/baz/blah/blafasel", 6), "/fo...");
    assert_eq!(shorten_path("/foo/bar/baz/blah/blafasel", 2), "/f");
    assert_eq!(
        shorten_path("/foo/bar/baz/blah/blafasel", 21),
        "/foo/.../blafasel"
    );
    assert_eq!(
        shorten_path("/foo/bar/baz/blah/blafasel", 22),
        "/foo/.../blah/blafasel"
    );
    assert_eq!(
        shorten_path("C:\\bar\\baz\\blah\\blafasel", 20),
        "C:\\bar\\...\\blafasel"
    );
    assert_eq!(
        shorten_path("/foo/blar/baz/blah/blafasel", 27),
        "/foo/blar/baz/blah/blafasel"
    );
    assert_eq!(
        shorten_path("/foo/blar/baz/blah/blafasel", 26),
        "/foo/.../baz/blah/blafasel"
    );
    assert_eq!(
        shorten_path("/foo/b/baz/blah/blafasel", 23),
        "/foo/.../blah/blafasel"
    );
    assert_eq!(shorten_path("/foobarbaz/blahblah", 16), ".../blahblah");
    assert_eq!(shorten_path("/foobarbazblahblah", 12), "...lahblah");
    assert_eq!(shorten_path("", 0), "");
}

#[test]
fn test_split_path() {
    assert_eq!(split_path("C:\\a\\b"), (Some("C:\\a"), "b"));
    assert_eq!(split_path("C:/a\\b"), (Some("C:/a"), "b"));
    assert_eq!(split_path("C:\\a\\b\\c"), (Some("C:\\a\\b"), "c"));
    assert_eq!(split_path("a\\b\\c\\d\\e"), (Some("a\\b\\c\\d"), "e"));
    assert_eq!(split_path("\\\\UNC\\a"), (Some("\\\\UNC"), "a"));

    assert_eq!(split_path("/a/b/c"), (Some("/a/b"), "c"));
    assert_eq!(split_path("/a/b/c/d"), (Some("/a/b/c"), "d"));
    assert_eq!(split_path("a/b/c"), (Some("a/b"), "c"));

    assert_eq!(split_path("a"), (None, "a"));
    assert_eq!(split_path("a/"), (None, "a"));
    assert_eq!(split_path("/a"), (Some("/"), "a"));
    assert_eq!(split_path(""), (None, ""));
}

#[test]
fn test_split_path_bytes() {
    assert_eq!(
        split_path_bytes(&b"C:\\a\\b"[..]),
        (Some(&b"C:\\a"[..]), &b"b"[..])
    );
    assert_eq!(
        split_path_bytes(&b"C:/a\\b"[..]),
        (Some(&b"C:/a"[..]), &b"b"[..])
    );
    assert_eq!(
        split_path_bytes(&b"C:\\a\\b\\c"[..]),
        (Some(&b"C:\\a\\b"[..]), &b"c"[..])
    );
    assert_eq!(
        split_path_bytes(&b"a\\b\\c\\d\\e"[..]),
        (Some(&b"a\\b\\c\\d"[..]), &b"e"[..])
    );
    assert_eq!(
        split_path_bytes(&b"\\\\UNC\\a"[..]),
        (Some(&b"\\\\UNC"[..]), &b"a"[..])
    );

    assert_eq!(
        split_path_bytes(&b"/a/b/c"[..]),
        (Some(&b"/a/b"[..]), &b"c"[..])
    );
    assert_eq!(
        split_path_bytes(&b"/a/b/c/d"[..]),
        (Some(&b"/a/b/c"[..]), &b"d"[..])
    );
    assert_eq!(
        split_path_bytes(&b"a/b/c"[..]),
        (Some(&b"a/b"[..]), &b"c"[..])
    );

    assert_eq!(split_path_bytes(&b"a"[..]), (None, &b"a"[..]));
    assert_eq!(split_path_bytes(&b"a/"[..]), (None, &b"a"[..]));
    assert_eq!(split_path_bytes(&b"/a"[..]), (Some(&b"/"[..]), &b"a"[..]));
    assert_eq!(split_path_bytes(&b""[..]), (None, &b""[..]));
}
