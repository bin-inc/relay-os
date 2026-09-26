//! Path syntax (spec §8.1). Paths and names are raw bytes: `/` separates
//! names, repeated slashes count as one, and a trailing slash means the
//! target must be a directory. Resolving names against a filesystem is the
//! mount table's job; this module only splits and checks.

use crate::Errno;
use alloc::string::String;
use alloc::vec::Vec;

/// The longest name, in bytes.
pub const NAME_MAX: usize = 255;
/// The longest path, in bytes.
pub const PATH_MAX: usize = 4096;

/// One step of a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Component<'a> {
    /// `.`
    Current,
    /// `..`
    Parent,
    Name(&'a [u8]),
}

/// A split path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Path<'a> {
    /// Starts at `/` rather than at the current directory.
    pub absolute: bool,
    pub components: Vec<Component<'a>>,
    /// The path ends in `/` after at least one component, so its target
    /// must be a directory (`ENOTDIR` otherwise).
    pub trailing_slash: bool,
}

/// Splits a path. Errors: `ENOENT` for an empty path, `ENAMETOOLONG` for a
/// path over `PATH_MAX` bytes or a name over `NAME_MAX`, `EINVAL` for a NUL
/// byte.
pub fn parse(path: &[u8]) -> Result<Path<'_>, Errno> {
    if path.is_empty() {
        return Err(Errno::ENOENT);
    }
    if path.len() > PATH_MAX {
        return Err(Errno::ENAMETOOLONG);
    }
    if path.contains(&0) {
        return Err(Errno::EINVAL);
    }
    let mut components = Vec::new();
    for name in path.split(|&b| b == b'/').filter(|n| !n.is_empty()) {
        components.push(match name {
            b"." => Component::Current,
            b".." => Component::Parent,
            _ if name.len() > NAME_MAX => return Err(Errno::ENAMETOOLONG),
            _ => Component::Name(name),
        });
    }
    Ok(Path {
        absolute: path[0] == b'/',
        trailing_slash: path.ends_with(b"/") && !components.is_empty(),
        components,
    })
}

/// Checks a name that is about to be created, removed or renamed: not
/// empty (`ENOENT`), not `.` or `..` and without `/` or NUL (`EINVAL`), at
/// most `NAME_MAX` bytes (`ENAMETOOLONG`).
pub fn check_name(name: &[u8]) -> Result<(), Errno> {
    if name.is_empty() {
        Err(Errno::ENOENT)
    } else if name == b"." || name == b".." || name.contains(&b'/') || name.contains(&0) {
        Err(Errno::EINVAL)
    } else if name.len() > NAME_MAX {
        Err(Errno::ENAMETOOLONG)
    } else {
        Ok(())
    }
}

/// A name or path for the screen: UTF-8 as it is, with every byte that is
/// not valid UTF-8, and every control character, shown as `?`.
pub fn display(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    let mut rest = bytes;
    while !rest.is_empty() {
        let (valid, skip) = match core::str::from_utf8(rest) {
            Ok(s) => (s, 0),
            Err(e) => {
                let valid = core::str::from_utf8(&rest[..e.valid_up_to()]).unwrap_or_default();
                (valid, e.error_len().unwrap_or(rest.len() - e.valid_up_to()))
            }
        };
        out.extend(valid.chars().map(|c| if c.is_control() { '?' } else { c }));
        out.extend(core::iter::repeat_n('?', skip));
        rest = &rest[valid.len() + skip..];
    }
    out
}

/// `dir/name`, without doubling a slash `dir` already ends with.
pub fn join(dir: &[u8], name: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(dir.len() + 1 + name.len());
    out.extend_from_slice(dir);
    if !dir.is_empty() && !dir.ends_with(b"/") {
        out.push(b'/');
    }
    out.extend_from_slice(name);
    out
}

/// The last name in a path, ignoring trailing slashes: `a/b/` gives `b`,
/// `/` gives `/`.
pub fn basename(path: &[u8]) -> &[u8] {
    let trimmed = match path.iter().rposition(|&b| b != b'/') {
        Some(end) => &path[..=end],
        None if path.is_empty() => return path,
        None => return b"/",
    };
    match trimmed.iter().rposition(|&b| b == b'/') {
        Some(slash) => &trimmed[slash + 1..],
        None => trimmed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Component::*;

    #[test]
    fn absolute_and_relative_paths_split_into_components() {
        let p = parse(b"/etc/motd").unwrap();
        assert!(p.absolute && !p.trailing_slash);
        assert_eq!(p.components, [Name(b"etc"), Name(b"motd")]);
        let p = parse(b"a/./../b").unwrap();
        assert!(!p.absolute);
        assert_eq!(p.components, [Name(b"a"), Current, Parent, Name(b"b")]);
    }

    #[test]
    fn repeated_slashes_count_as_one() {
        let p = parse(b"//usr///bin").unwrap();
        assert!(p.absolute);
        assert_eq!(p.components, [Name(b"usr"), Name(b"bin")]);
    }

    #[test]
    fn root_has_no_components_and_no_trailing_slash() {
        for root in [&b"/"[..], b"///"] {
            let p = parse(root).unwrap();
            assert!(p.absolute && p.components.is_empty() && !p.trailing_slash);
        }
    }

    #[test]
    fn a_trailing_slash_is_remembered() {
        assert!(parse(b"tmp/").unwrap().trailing_slash);
        assert!(parse(b"/tmp//").unwrap().trailing_slash);
        assert!(!parse(b"tmp/.").unwrap().trailing_slash);
    }

    #[test]
    fn empty_long_and_nul_paths_are_rejected() {
        assert_eq!(parse(b""), Err(Errno::ENOENT));
        assert_eq!(parse(b"a\0b"), Err(Errno::EINVAL));
        let name = [b'n'; NAME_MAX];
        assert!(parse(&name).is_ok());
        assert_eq!(parse(&[b'n'; NAME_MAX + 1]), Err(Errno::ENAMETOOLONG));
        let mut path = Vec::new();
        while path.len() + 2 <= PATH_MAX {
            path.extend_from_slice(b"/a");
        }
        assert_eq!(path.len(), PATH_MAX);
        assert!(parse(&path).is_ok());
        path.push(b'/');
        assert_eq!(parse(&path), Err(Errno::ENAMETOOLONG));
    }

    #[test]
    fn names_for_changes_are_checked() {
        assert_eq!(check_name(b"notes.txt"), Ok(()));
        assert_eq!(check_name(b""), Err(Errno::ENOENT));
        assert_eq!(check_name(b"."), Err(Errno::EINVAL));
        assert_eq!(check_name(b".."), Err(Errno::EINVAL));
        assert_eq!(check_name(b"a/b"), Err(Errno::EINVAL));
        assert_eq!(check_name(b"a\0"), Err(Errno::EINVAL));
        assert_eq!(check_name(&[b'x'; NAME_MAX]), Ok(()));
        assert_eq!(check_name(&[b'x'; NAME_MAX + 1]), Err(Errno::ENAMETOOLONG));
    }

    #[test]
    fn invalid_utf8_and_control_characters_show_as_question_marks() {
        assert_eq!(display(b"caf\xc3\xa9"), "café");
        assert_eq!(display(b"a\xffb"), "a?b");
        assert_eq!(display(b"\xe2\x82"), "??");
        assert_eq!(display(b"\xe2\x82x"), "??x");
        assert_eq!(display(b"tab\there\x1b[2J"), "tab?here?[2J");
        assert_eq!(display(b""), "");
    }

    #[test]
    fn join_and_basename() {
        assert_eq!(join(b"/", b"etc"), b"/etc");
        assert_eq!(join(b"/etc", b"motd"), b"/etc/motd");
        assert_eq!(join(b"dir/", b"f"), b"dir/f");
        assert_eq!(join(b"", b"f"), b"f");
        assert_eq!(basename(b"/etc/motd"), b"motd");
        assert_eq!(basename(b"a/b//"), b"b");
        assert_eq!(basename(b"file"), b"file");
        assert_eq!(basename(b"/"), b"/");
        assert_eq!(basename(b"//"), b"/");
        assert_eq!(basename(b""), b"");
    }
}
