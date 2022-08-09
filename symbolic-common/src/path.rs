use std::borrow::Cow;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

trait IntoChar {
    fn into_char(self) -> char;
}

impl IntoChar for char {
    fn into_char(self) -> char {
        self
    }
}

impl IntoChar for u8 {
    fn into_char(self) -> char {
        char::from(self)
    }
}

impl<T: IntoChar + Copy> IntoChar for &'_ T {
    fn into_char(self) -> char {
        (*self).into_char()
    }
}

/// Returns `true` if the given character is any valid directory separator.
#[inline]
fn is_path_separator<C: IntoChar>(c: C) -> bool {
    matches!(c.into_char(), '\\' | '/')
}

/// Returns `true` if the given character is a valid Windows directory separator.
#[inline]
fn is_windows_separator<C: IntoChar>(c: C) -> bool {
    is_path_separator(c)
}

/// Returns `true` if the given character is a valid UNIX directory separator.
#[inline]
fn is_unix_separator<C: IntoChar>(c: C) -> bool {
    c.into_char() == '/'
}

/// Returns `true` if this is a Windows Universal Naming Convention path (UNC).
fn is_windows_unc<P: AsRef<[u8]>>(path: P) -> bool {
    let path = path.as_ref();
    path.starts_with(b"\\\\") || path.starts_with(b"//")
}

/// Returns `true` if this is an absolute Windows path starting with a drive letter.
fn is_windows_driveletter<P: AsRef<[u8]>>(path: P) -> bool {
    let path = path.as_ref();

    if let (Some(drive_letter), Some(b':')) = (path.first(), path.get(1)) {
        if matches!(drive_letter, b'A'..=b'Z' | b'a'..=b'z') {
            return path.get(2).map_or(true, is_windows_separator);
        }
    }

    false
}

/// Returns `true` if this is an absolute Windows path.
fn is_absolute_windows_path<P: AsRef<[u8]>>(path: P) -> bool {
    let path = path.as_ref();
    is_windows_unc(path) || is_windows_driveletter(path)
}

/// Returns `true`
fn is_semi_absolute_windows_path<P: AsRef<[u8]>>(path: P) -> bool {
    path.as_ref().first().map_or(false, is_windows_separator)
}

fn is_absolute_unix_path<P: AsRef<[u8]>>(path: P) -> bool {
    path.as_ref().first().map_or(false, is_unix_separator)
}

fn is_windows_path<P: AsRef<[u8]>>(path: P) -> bool {
    let path = path.as_ref();
    is_absolute_windows_path(path) || path.contains(&b'\\')
}

/// Joins paths of various platforms.
///
/// This attempts to detect Windows or Unix paths and joins with the correct directory separator.
/// Also, trailing directory separators are detected in the base string and empty paths are handled
/// correctly.
///
/// # Examples
///
/// Join a relative UNIX path:
///
/// ```
/// assert_eq!(symbolic_common::join_path("/a/b", "c/d"), "/a/b/c/d");
/// ```
///
/// Join a Windows drive letter path path:
///
/// ```
/// assert_eq!(symbolic_common::join_path("C:\\a", "b\\c"), "C:\\a\\b\\c");
/// ```
///
/// If the right-hand side is an absolute path, it replaces the left-hand side:
///
/// ```
/// assert_eq!(symbolic_common::join_path("/a/b", "/c/d"), "/c/d");
/// ```
pub fn join_path(base: &str, other: &str) -> String {
    // special case for things like <stdin> or others.
    if other.starts_with('<') && other.ends_with('>') {
        return other.into();
    }

    // absolute paths
    if base.is_empty() || is_absolute_windows_path(other) || is_absolute_unix_path(other) {
        return other.into();
    }

    // other weird cases
    if other.is_empty() {
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

    // Always trim by both separators, since as soon as the path is Windows, slashes also count as
    // valid path separators. However, use the main separator for joining.
    let is_windows = is_windows_path(base) || is_windows_path(other);
    format!(
        "{}{}{}",
        base.trim_end_matches(is_path_separator),
        if is_windows { '\\' } else { '/' },
        other.trim_start_matches(is_path_separator)
    )
}

fn pop_path(path: &mut String) -> bool {
    if let Some(idx) = path.rfind(is_path_separator) {
        path.truncate(idx);
        true
    } else if !path.is_empty() {
        path.truncate(0);
        true
    } else {
        false
    }
}

/// Simplifies paths by stripping redundant components.
///
/// This removes redundant `../` or `./` path components. However, this function does not operate on
/// the file system. Since it does not resolve symlinks, this is a potentially lossy operation.
///
/// # Examples
///
/// Remove `./` components:
///
/// ```
/// assert_eq!(symbolic_common::clean_path("/a/./b"), "/a/b");
/// ```
///
/// Remove path components followed by `../`:
///
/// ```
/// assert_eq!(symbolic_common::clean_path("/a/b/../c"), "/a/c");
/// ```
///
/// Note that when the path is relative, the parent dir components may exceed the top-level:
///
/// ```
/// assert_eq!(symbolic_common::clean_path("/foo/../../b"), "../b");
/// ```
pub fn clean_path(path: &str) -> Cow<'_, str> {
    // TODO: This function has a number of problems (see broken tests):
    //  - It does not collapse consequtive directory separators
    //  - Parent-directory directives may leave an absolute path
    //  - A path is converted to relative when the parent directory hits top-level

    let mut rv = String::with_capacity(path.len());
    let main_separator = if is_windows_path(path) { '\\' } else { '/' };

    let mut needs_separator = false;
    let mut is_past_root = false;

    for segment in path.split_terminator(is_path_separator) {
        if segment == "." {
            continue;
        } else if segment == ".." {
            if !is_past_root && pop_path(&mut rv) {
                if rv.is_empty() {
                    needs_separator = false;
                }
            } else {
                if !is_past_root {
                    needs_separator = false;
                    is_past_root = true;
                }
                if needs_separator {
                    rv.push(main_separator);
                }
                rv.push_str("..");
                needs_separator = true;
            }
            continue;
        }
        if needs_separator {
            rv.push(main_separator);
        } else {
            needs_separator = true;
        }
        rv.push_str(segment);
    }

    // For now, always return an owned string.
    // This can be optimized later.
    Cow::Owned(rv)
}

/// Splits off the last component of a path given as bytes.
///
/// The path should be a path to a file, and not a directory with a trailing directory separator. If
/// this path is a directory or the root path, the result is undefined.
///
/// This attempts to detect Windows or Unix paths and split off the last component of the path
/// accordingly. Note that for paths with mixed slash and backslash separators this might not lead
/// to the desired results.
///
/// **Note**: This is the same as [`split_path`], except that it operates on byte slices.
///
/// # Examples
///
/// Split the last component of a UNIX path:
///
/// ```
/// assert_eq!(
///     symbolic_common::split_path_bytes(b"/a/b/c"),
///     (Some("/a/b".as_bytes()), "c".as_bytes())
/// );
/// ```
///
/// Split the last component of a Windows path:
///
/// ```
/// assert_eq!(
///     symbolic_common::split_path_bytes(b"C:\\a\\b"),
///     (Some("C:\\a".as_bytes()), "b".as_bytes())
/// );
/// ```
///
/// [`split_path`]: fn.split_path.html
pub fn split_path_bytes(path: &[u8]) -> (Option<&[u8]>, &[u8]) {
    // Trim directory separators at the end, if any.
    let path = match path.iter().rposition(|c| !is_path_separator(c)) {
        Some(cutoff) => &path[..=cutoff],
        None => path,
    };

    // Split by all path separators. On Windows, both are valid and a path is considered a
    // Windows path as soon as it has a backslash inside.
    match path.iter().rposition(is_path_separator) {
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
///
/// **Note**: For a version that operates on byte slices, see [`split_path_bytes`].
///
/// # Examples
///
/// Split the last component of a UNIX path:
///
/// ```
/// assert_eq!(symbolic_common::split_path("/a/b/c"), (Some("/a/b"), "c"));
/// ```
///
/// Split the last component of a Windows path:
///
/// ```
/// assert_eq!(symbolic_common::split_path("C:\\a\\b"), (Some("C:\\a"), "b"));
/// ```
///
/// [`split_path_bytes`]: fn.split_path_bytes.html
pub fn split_path(path: &str) -> (Option<&str>, &str) {
    let (dir, name) = split_path_bytes(path.as_bytes());
    unsafe {
        (
            dir.map(|b| std::str::from_utf8_unchecked(b)),
            std::str::from_utf8_unchecked(name),
        )
    }
}

/// Truncates the given string at character boundaries.
fn truncate(path: &str, mut length: usize) -> &str {
    // Backtrack to the last code point. There is a unicode point at least at the beginning of the
    // string before the first character, which is why this cannot underflow.
    while !path.is_char_boundary(length) {
        length -= 1;
    }

    path.get(..length).unwrap_or_default()
}

/// Trims a path to a given length.
///
/// This attempts to not completely destroy the path in the process by trimming off the middle path
/// segments. In the process, this tries to determine whether the path is a Windows or Unix path and
/// handle directory separators accordingly.
///
/// # Examples
///
/// ```
/// assert_eq!(
///     symbolic_common::shorten_path("/foo/bar/baz/blah/blafasel", 21),
///     "/foo/.../blafasel"
/// );
/// ```
pub fn shorten_path(path: &str, length: usize) -> Cow<'_, str> {
    // trivial cases
    if path.len() <= length {
        return Cow::Borrowed(path);
    } else if length <= 3 {
        return Cow::Borrowed(truncate(path, length));
    } else if length <= 10 {
        return Cow::Owned(format!("{}...", truncate(path, length - 3)));
    }

    let mut rv = String::new();
    let mut last_idx = 0;
    let mut piece_iter = path.match_indices(is_path_separator);
    let mut final_sep = "/";
    let max_len = length - 4;

    // make sure we get two segments at the start.
    for (idx, sep) in &mut piece_iter {
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
        let basename = path.rsplit(is_path_separator).next().unwrap();
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
        rv.push_str(item);
    }

    Cow::Owned(rv)
}

/// Extensions to `Path` for handling `dSYM` directories.
///
/// # dSYM Files
///
/// `dSYM` files are actually folder structures that store debugging information on Apple platforms.
/// They are also referred to as debug companion. At the core of this structure is a `MachO` file
/// containing the actual debug information.
///
/// A full `dSYM` folder structure looks like this:
///
/// ```text
/// MyApp.dSYM
/// └── Contents
///     ├── Info.plist
///     └── Resources
///         └── DWARF
///             └── MyApp
/// ```
pub trait DSymPathExt {
    /// Returns `true` if this path points to an existing directory with a `.dSYM` extension.
    ///
    /// Note that this does not check if a full `dSYM` structure is contained within this folder.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use symbolic_common::DSymPathExt;
    ///
    /// assert!(Path::new("Foo.dSYM").is_dsym_dir());
    /// assert!(!Path::new("Foo").is_dsym_dir());
    /// ```
    fn is_dsym_dir(&self) -> bool;

    /// Resolves the path of the debug file in a `dSYM` directory structure.
    ///
    /// Returns `Some(path)` if this path is a dSYM directory according to [`is_dsym_dir`], and a
    /// file of the same name is located at `Contents/Resources/DWARF/`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use symbolic_common::DSymPathExt;
    ///
    /// let path = Path::new("Foo.dSYM");
    /// let dsym_path = path.resolve_dsym().unwrap();
    /// assert_eq!(dsym_path, Path::new("Foo.dSYM/Contents/Resources/DWARF/Foo"));
    /// ```
    ///
    /// [`is_dsym_dir`]: trait.DSymPathExt.html#tymethod.is_dsym_dir
    fn resolve_dsym(&self) -> Option<PathBuf>;

    /// Resolves the `dSYM` parent directory if this file is a dSYM.
    ///
    /// If this path points to the MachO file in a `dSYM` directory structure, this function returns
    /// the path to the dSYM directory. Returns `None` if the parent does not exist or the file name
    /// does not match.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use symbolic_common::DSymPathExt;
    ///
    /// let path = Path::new("Foo.dSYM/Contents/Resources/DWARF/Foo");
    /// let parent = path.dsym_parent().unwrap();
    /// assert_eq!(parent, Path::new("Foo.dSYM"));
    ///
    /// let path = Path::new("Foo.dSYM/Contents/Resources/DWARF/Bar");
    /// assert_eq!(path.dsym_parent(), None);
    /// ```
    fn dsym_parent(&self) -> Option<&Path>;
}

impl DSymPathExt for Path {
    fn is_dsym_dir(&self) -> bool {
        self.extension() == Some("dSYM".as_ref()) && self.is_dir()
    }

    fn resolve_dsym(&self) -> Option<PathBuf> {
        if !self.is_dsym_dir() || !self.is_dir() {
            return None;
        }

        let framework = self.file_stem()?;
        let mut full_path = self.to_path_buf();
        full_path.push("Contents/Resources/DWARF");
        full_path.push(framework);

        // XCode produces [appName].app.dSYM files where the debug file's name is just [appName],
        // so strip .app if it's present.
        if matches!(full_path.extension(), Some(extension) if extension == "app") {
            full_path = full_path.with_extension("")
        }

        if full_path.is_file() {
            Some(full_path)
        } else {
            None
        }
    }

    fn dsym_parent(&self) -> Option<&Path> {
        let framework = self.file_name()?;

        let mut parent = self.parent()?;
        if !parent.ends_with("Contents/Resources/DWARF") {
            return None;
        }

        for _ in 0..3 {
            parent = parent.parent()?;
        }

        // Accept both Filename.dSYM and Filename.framework.dSYM as
        // the bundle directory name.
        let stem_matches = parent
            .file_name()
            .and_then(|name| Path::new(name).file_stem())
            .map(|stem| {
                if stem == framework {
                    return true;
                }
                let alt = Path::new(stem);
                alt.file_stem() == Some(framework)
                    && alt.extension() == Some(OsStr::new("framework"))
            })
            .unwrap_or(false);
        if parent.is_dsym_dir() && stem_matches {
            Some(parent)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use similar_asserts::assert_eq;
    use symbolic_testutils::fixture;

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

        assert_eq!(
            join_path("foo", "아이쿱 조합원 앱카드"),
            "foo/아이쿱 조합원 앱카드"
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
        assert_eq!(clean_path("\\\\foo\\..\\bar"), "\\\\bar");
        assert_eq!(
            clean_path("foo/bar/../아이쿱 조합원 앱카드"),
            "foo/아이쿱 조합원 앱카드"
        );

        // XXX currently known broken tests:
        // assert_eq!(clean_path("/foo/../bar"), "/bar");
        // assert_eq!(clean_path("\\\\foo\\..\\..\\bar"), "\\\\bar");
        // assert_eq!(clean_path("/../../blah/"), "/blah");
        // assert_eq!(clean_path("c:\\..\\foo"), "c:\\foo");
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

        assert_eq!(shorten_path("아이쿱 조합원 앱카드", 9), "아...");
        assert_eq!(shorten_path("아이쿱 조합원 앱카드", 20), "...ᆸ카드");
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

        assert_eq!(
            split_path("foo/아이쿱 조합원 앱카드"),
            (Some("foo"), "아이쿱 조합원 앱카드")
        );
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

    #[test]
    fn test_is_dsym_dir() {
        assert!(fixture("macos/crash.dSYM").is_dsym_dir());
        assert!(!fixture("macos/crash").is_dsym_dir());
    }

    #[test]
    fn test_resolve_dsym() {
        let crash_path = fixture("macos/crash.dSYM");
        let resolved = crash_path.resolve_dsym().unwrap();
        assert!(resolved.exists());
        assert!(resolved.ends_with("macos/crash.dSYM/Contents/Resources/DWARF/crash"));

        let other_path = fixture("macos/other.dSYM");
        assert_eq!(other_path.resolve_dsym(), None);
    }

    // XCode and other tools (e.g. dwarfdump) produce a dSYM that includes the .app
    // suffix, which needs to be stripped.
    #[test]
    fn test_resolve_dsym_double_extension() {
        let crash_path = fixture("macos/crash.app.dSYM");
        let resolved = crash_path.resolve_dsym().unwrap();
        assert!(resolved.exists());
        assert!(resolved.ends_with("macos/crash.app.dSYM/Contents/Resources/DWARF/crash"));

        let other_path = fixture("macos/other.dmp.dSYM");
        assert_eq!(other_path.resolve_dsym(), None);
    }

    #[test]
    fn test_dsym_parent() {
        let crash_path = fixture("macos/crash.dSYM/Contents/Resources/DWARF/crash");
        let dsym_path = crash_path.dsym_parent().unwrap();
        assert!(dsym_path.exists());
        assert!(dsym_path.ends_with("macos/crash.dSYM"));

        let other_path = fixture("macos/crash.dSYM/Contents/Resources/DWARF/invalid");
        assert_eq!(other_path.dsym_parent(), None);
    }

    #[test]
    fn test_dsym_parent_framework() {
        let dwarf_path = fixture("macos/Example.framework.dSYM/Contents/Resources/DWARF/Example");
        let dsym_path = dwarf_path.dsym_parent().unwrap();
        assert!(dsym_path.exists());
        assert!(dsym_path.ends_with("macos/Example.framework.dSYM"));
    }
}
