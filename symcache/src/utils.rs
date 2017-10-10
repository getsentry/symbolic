/// A quick binary search by key.
pub fn binsearch_by_key<'a, T, B, F>(slice: &'a [T], item: B, mut f: F) -> Option<(usize, &'a T)>
    where B: Ord, F: FnMut(usize, &T) -> B
{
    let mut low = 0;
    let mut high = slice.len();

    while low < high {
        let mid = (low + high) / 2;
        let cur_item = &slice[mid as usize];
        if item < f(mid as usize, cur_item) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }

    if low > 0 && low <= slice.len() {
        Some((low - 1, &slice[low - 1]))
    } else {
        None
    }
}

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

#[test]
fn test_binsearch() {
    let seq = [0u32, 2, 4, 6, 8, 10];
    let m = binsearch_by_key(&seq[..], 5, |_, &x| x);
    assert_eq!(*m.unwrap().1, 4);

    let m = binsearch_by_key(&seq[..], 4, |_, &x| x);
    assert_eq!(m.unwrap().0, 2);
    assert_eq!(*m.unwrap().1, 4);
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
