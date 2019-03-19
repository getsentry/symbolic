use std::borrow::Cow;

fn is_absolute_windows_path(s: &str) -> bool {
    // UNC
    if s.len() > 2 && &s[..2] == "\\\\" {
        return true;
    }

    // other paths
    let mut char_iter = s.chars();
    let (fc, sc, tc) = (char_iter.next(), char_iter.next(), char_iter.next());

    match fc.unwrap_or_default() {
        'A'..='Z' | 'a'..='z' => {
            if sc == Some(':') && tc.map_or(false, |tc| tc == '\\' || tc == '/') {
                return true;
            }
        }
        _ => (),
    }

    false
}

fn is_absolute_unix_path(s: &str) -> bool {
    s.starts_with('/')
}

/// Joins paths of various platforms.
///
/// This attempts to detect Windows or Unix paths and joins with the correct directory separator.
/// Also, trailing directory separators are detected in the base string and empty paths are handled
/// correctly.
pub fn join_path(base: &str, other: &str) -> String {
    // absolute paths
    if base == "" || is_absolute_windows_path(other) || is_absolute_unix_path(other) {
        return other.into();
    }

    // other weird cases
    if other == "" {
        return base.into();
    }

    let win_abs = is_absolute_windows_path(base);
    let unix_abs = is_absolute_unix_path(base);
    let win_style = win_abs || (!unix_abs && base.contains('\\'));

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
    let cutoff = path
        .iter()
        .rposition(|b| *b != b'\\' && *b != b'/')
        .unwrap_or_else(|| path.len());
    let path = &path[..cutoff];

    // Try to find a backslash which could indicate a Windows path.
    let split_char = if !path.starts_with(b"/") && path.iter().any(|b| *b == b'\\') {
        b'\\' // Probably Windows
    } else {
        b'/' // Probably UNIX
    };

    match path.iter().rposition(|b| *b == split_char) {
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
    assert_eq!(join_path("C:\\test", "other"), "C:\\test\\other");
    assert_eq!(join_path("C:/test", "other"), "C:/test\\other");
    assert_eq!(
        join_path("C:\\test", "other\\stuff"),
        "C:\\test\\other\\stuff"
    );
    assert_eq!(join_path("C:/test", "C:\\other"), "C:\\other");
    assert_eq!(
        join_path("foo\\bar\\baz", "blub\\blah"),
        "foo\\bar\\baz\\blub\\blah"
    );

    assert_eq!(join_path("/usr/bin", "bash"), "/usr/bin/bash");
    assert_eq!(join_path("/usr/local", "bin/bash"), "/usr/local/bin/bash");
    assert_eq!(join_path("/usr/bin", "/usr/local/bin"), "/usr/local/bin");
    assert_eq!(join_path("foo/bar/", "blah"), "foo/bar/blah");
}

#[test]
fn test_shorten_path() {
    assert_eq!(&shorten_path("/foo/bar/baz/blah/blafasel", 6), "/fo...");
    assert_eq!(&shorten_path("/foo/bar/baz/blah/blafasel", 2), "/f");
    assert_eq!(
        &shorten_path("/foo/bar/baz/blah/blafasel", 21),
        "/foo/.../blafasel"
    );
    assert_eq!(
        &shorten_path("/foo/bar/baz/blah/blafasel", 22),
        "/foo/.../blah/blafasel"
    );
    assert_eq!(
        &shorten_path("C:\\bar\\baz\\blah\\blafasel", 20),
        "C:\\bar\\...\\blafasel"
    );
    assert_eq!(
        &shorten_path("/foo/blar/baz/blah/blafasel", 27),
        "/foo/blar/baz/blah/blafasel"
    );
    assert_eq!(
        &shorten_path("/foo/blar/baz/blah/blafasel", 26),
        "/foo/.../baz/blah/blafasel"
    );
    assert_eq!(
        &shorten_path("/foo/b/baz/blah/blafasel", 23),
        "/foo/.../blah/blafasel"
    );
    assert_eq!(&shorten_path("/foobarbaz/blahblah", 16), ".../blahblah");
    assert_eq!(&shorten_path("/foobarbazblahblah", 12), "...lahblah");
}
