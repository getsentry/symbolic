use std::borrow::Cow;


fn is_absolute_windows_path(s: &str) -> bool {
    // UNC
    if s.len() > 2 && &s[..2] == "\\\\" {
        return true;
    }

    // other paths
    let mut char_iter = s.chars();
    if_chain! {
        if let Some(fc) = char_iter.next();
        if matches!(fc, 'A'...'Z') || matches!(fc, 'a'...'z');
        if let Some(sc) = char_iter.next();
        if sc == ':';
        if let Some(tc) = char_iter.next();
        if tc == '\\' || tc == '/';
        then {
            true
        } else {
            false
        }
    }
}

fn is_absolute_unix_path(s: &str) -> bool {
    let mut char_iter = s.chars();
    char_iter.next() == Some('/')
}

/// Joins unknown paths together.
///
/// This kinda implements some windows/unix path joining semantics but it does
/// not attempt to be perfect.  It for instance currently does not fully
/// understand windows paths.
pub fn common_join_path(base: &str, other: &str) -> String {
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
    let win_style = win_abs || (!unix_abs && base.chars().any(|x| x == '\\'));

    return if win_style {
        format!("{}\\{}", base.trim_right_matches(&['\\', '/'][..]),
                other.trim_left_matches(&['\\', '/'][..]))
    } else {
        format!("{}/{}", base.trim_right_matches('/'),
                other.trim_left_matches('/'))
    };
}

/// Trims a path to a given length.
///
/// This attempts to not completely destroy the path in the process.
pub fn shorten_filename<'a>(filename: &'a str, length: usize) -> Cow<'a, str> {
    // trivial cases
    if filename.len() <= length {
        return Cow::Borrowed(filename);
    } else if length <= 10 {
        if length > 3 {
            return Cow::Owned(format!("{}...", &filename[..length - 3]));
        }
        return Cow::Borrowed(&filename[..length]);
    }

    let mut rv = String::new();
    let mut last_idx = 0;
    let mut piece_iter = filename.match_indices(&['\\', '/'][..]);
    let mut final_sep = "/";
    let max_len = length - 3;

    // make sure we get two segments at the start.
    loop {
        if let Some((idx, sep)) = piece_iter.next() {
            let slice = &filename[last_idx..idx + sep.len()];
            rv.push_str(slice);
            let done = last_idx > 0;
            last_idx = idx + sep.len();
            final_sep = sep;
            if done {
                break;
            }
        } else {
            break;
        }
    }

    // collect the rest of the segments into a temporary we can then reverse.
    let mut final_length = rv.len() as i64;
    let mut rest = vec![];
    let mut next_idx = filename.len();

    while let Some((idx, _)) = piece_iter.next_back() {
        if idx <= last_idx {
            break;
        }
        let slice = &filename[idx + 1..next_idx];
        if final_length + (slice.len() as i64) > max_len as i64 {
            break;
        }

        rest.push(slice);
        next_idx = idx + 1;
        final_length += slice.len() as i64;
    }

    // if at this point already we're too long we just take the last element
    // of the filename and strip it.
    if rv.len() > max_len || rest.len() == 0 {
        let basename = filename.rsplit(&['\\', '/'][..]).next().unwrap();
        if basename.len() > max_len {
            return Cow::Owned(format!("...{}", &basename[basename.len() - max_len..]));
        } else {
            return Cow::Owned(format!("..{}{}", final_sep, basename));
        }
    }

    rest.reverse();
    rv.push_str("..");
    rv.push_str(final_sep);
    for item in rest {
        rv.push_str(&item);
    }

    Cow::Owned(rv)
}

#[test]
fn test_common_join_path() {
    assert_eq!(common_join_path("C:\\test", "other"), "C:\\test\\other");
    assert_eq!(common_join_path("C:/test", "other"), "C:/test\\other");
    assert_eq!(common_join_path("C:\\test", "other\\stuff"), "C:\\test\\other\\stuff");
    assert_eq!(common_join_path("C:/test", "C:\\other"), "C:\\other");
    assert_eq!(common_join_path("foo\\bar\\baz", "blub\\blah"), "foo\\bar\\baz\\blub\\blah");

    assert_eq!(common_join_path("/usr/bin", "bash"), "/usr/bin/bash");
    assert_eq!(common_join_path("/usr/local", "bin/bash"), "/usr/local/bin/bash");
    assert_eq!(common_join_path("/usr/bin", "/usr/local/bin"), "/usr/local/bin");
    assert_eq!(common_join_path("foo/bar/", "blah"), "foo/bar/blah");
}

#[test]
fn test_shorten_filename() {
    assert_eq!(&shorten_filename("/foo/bar/baz/blah/blafasel", 6), "/fo...");
    assert_eq!(&shorten_filename("/foo/bar/baz/blah/blafasel", 2), "/f");
    assert_eq!(&shorten_filename("/foo/bar/baz/blah/blafasel", 20), "/foo/../blafasel");
    assert_eq!(&shorten_filename("/foo/bar/baz/blah/blafasel", 21), "/foo/../blah/blafasel");
    assert_eq!(&shorten_filename("C:\\bar\\baz\\blah\\blafasel", 20), "C:\\bar\\..\\blafasel");
    assert_eq!(&shorten_filename("/foo/bar/baz/blah/blafasel", 26), "/foo/bar/baz/blah/blafasel");
    assert_eq!(&shorten_filename("/foo/bar/baz/blah/blafasel", 25), "/foo/../baz/blah/blafasel");
    assert_eq!(&shorten_filename("/foo/b/baz/blah/blafasel", 23), "/foo/../blah/blafasel");
    assert_eq!(&shorten_filename("/foobarbaz/blahblah", 16), "../blahblah");
    assert_eq!(&shorten_filename("/foobarbazblahblah", 12), "...zblahblah");
}
