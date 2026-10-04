//! A shell's variables and arguments (user-space gate §9.4), and which
//! variables it exports (programmable shell gate §8.5): an exported
//! variable goes into the environment of every program the shell starts,
//! in the order the variables were exported.

use crate::expand::Error;
use crate::parser::is_name;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// The most a shell's variables hold, their names' and values' bytes.
pub const VARS_MAX: usize = 64 * 1024;

/// A shell's variables and arguments.
pub(crate) struct Vars {
    names: BTreeMap<String, Var>,
    /// The bytes of their names and values.
    size: usize,
    /// `$0`, then `$1` on.
    pub(crate) args: Vec<String>,
    /// The place the next variable exported takes.
    next: u64,
}

/// One variable: its value, none for one exported before it was given
/// one (`export B`, `OLDPWD` when the shell starts), and its place among
/// the exported ones.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Var {
    value: Option<String>,
    export: Option<u64>,
}

impl Vars {
    /// No variables, and no arguments after `$0`, which is `name`.
    pub fn new(name: &str) -> Vars {
        Vars {
            names: BTreeMap::new(),
            size: 0,
            args: alloc::vec![String::from(name)],
            next: 0,
        }
    }

    /// A script's: none set, `$0` its `name` and `args` after it.
    pub fn script(name: &str, args: &[String]) -> Vars {
        let mut vars = Vars::new(name);
        vars.args.extend_from_slice(args);
        vars
    }

    /// `$0` becomes `name`.
    pub fn set_name(&mut self, name: &str) {
        self.args[0] = String::from(name);
    }

    /// `$0` becomes `name`, and `args` come after it (a script's).
    pub fn set_args(&mut self, name: &str, args: &[String]) {
        self.args = alloc::vec![String::from(name)];
        self.args.extend_from_slice(args);
    }

    /// Imports an environment (programmable shell gate §8.5): each entry of
    /// `block`, `NAME=value` and a NUL, whose name is one and that is UTF-8
    /// text, becomes an exported variable, in the block's order; of two
    /// with one name the last wins, as in bash. A block `spawn` took holds
    /// at most 64 KiB, so its variables fit.
    pub fn import(&mut self, block: &[u8]) {
        for entry in block.split(|&b| b == 0) {
            let Ok(entry) = core::str::from_utf8(entry) else {
                continue;
            };
            if let Some((name, value)) = entry.split_once('=')
                && is_name(name)
            {
                let _ = self.export(name, Some(String::from(value)));
            }
        }
    }

    /// The arguments after `$0`, which `"$@"` gives.
    pub fn positional(&self) -> &[String] {
        self.args.get(1..).unwrap_or(&[])
    }

    /// The variable `name`'s value; an unset one, or one without a value,
    /// is empty.
    pub fn get(&self, name: &str) -> &str {
        self.value(name).unwrap_or("")
    }

    /// The variable `name`'s value, if it has one.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.names.get(name).and_then(|v| v.value.as_deref())
    }

    /// Sets the variable `name` to `value`, exported or not as it was,
    /// unless the variables would then hold more than `VARS_MAX`.
    pub fn set(&mut self, name: &str, value: String) -> Result<(), Error> {
        let export = self.names.get(name).and_then(|v| v.export);
        self.put(
            name,
            Var {
                value: Some(value),
                export,
            },
        )
    }

    /// Exports `name`, with `value` if one is given, or the value it has.
    /// One already exported keeps its place; another goes after the rest.
    pub fn export(&mut self, name: &str, value: Option<String>) -> Result<(), Error> {
        let old = self.names.get(name);
        let export = match old.and_then(|v| v.export) {
            Some(place) => place,
            None => self.next,
        };
        let value = value.or_else(|| old.and_then(|v| v.value.clone()));
        self.put(
            name,
            Var {
                value,
                export: Some(export),
            },
        )?;
        if export == self.next {
            self.next += 1;
        }
        Ok(())
    }

    /// Removes the variable `name`, exported or not.
    pub fn unset(&mut self, name: &str) {
        if let Some(old) = self.names.remove(name) {
            self.size -= size(name, &old);
        }
    }

    /// Every exported variable, by name: its value, if it has one.
    pub fn exported(&self) -> impl Iterator<Item = (&str, Option<&str>)> {
        self.names
            .iter()
            .filter(|(_, v)| v.export.is_some())
            .map(|(n, v)| (n.as_str(), v.value.as_deref()))
    }

    /// The environment of a program the shell starts: each exported
    /// variable that has a value, `NAME=value` and a NUL, in the order of
    /// export.
    pub fn environment(&self) -> Vec<u8> {
        let mut exported: Vec<(u64, &str, &str)> = self
            .names
            .iter()
            .filter_map(|(n, v)| Some((v.export?, n.as_str(), v.value.as_deref()?)))
            .collect();
        exported.sort_unstable_by_key(|&(place, _, _)| place);
        let mut block = Vec::new();
        for (_, name, value) in exported {
            block.extend_from_slice(name.as_bytes());
            block.push(b'=');
            block.extend_from_slice(value.as_bytes());
            block.push(0);
        }
        block
    }

    /// Stores `var` as `name`, unless the variables would then hold more
    /// than `VARS_MAX`.
    fn put(&mut self, name: &str, var: Var) -> Result<(), Error> {
        let old = self.names.get(name).map_or(0, |v| size(name, v));
        let new = self.size - old + size(name, &var);
        if new > VARS_MAX {
            return Err(Error::Full(String::from(name)));
        }
        self.size = new;
        self.names.insert(String::from(name), var);
        Ok(())
    }
}

/// What a variable counts towards `VARS_MAX`: its name and its value.
fn size(name: &str, var: &Var) -> usize {
    name.len() + var.value.as_ref().map_or(0, String::len)
}

#[cfg(test)]
impl Vars {
    /// `names` set, and `args` (`$0` first).
    pub fn of(names: &[(&str, &str)], args: &[&str]) -> Vars {
        let mut vars = Vars::new("");
        for (n, v) in names {
            vars.set(n, String::from(*v)).unwrap();
        }
        vars.args = args.iter().map(|a| String::from(*a)).collect();
        vars
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_variables_hold_at_most_64_kib() {
        let mut v = Vars::new("sh");
        let big = "x".repeat(VARS_MAX - 1);
        v.set("A", big.clone()).unwrap();
        assert_eq!(v.set("B", String::new()), Err(Error::Full("B".into())));
        assert_eq!(v.set("A", big.clone() + "y"), Err(Error::Full("A".into())));
        assert_eq!(v.get("A"), big, "unchanged");
        // A smaller value makes room again.
        v.set("A", String::from("1")).unwrap();
        v.set("B", "x".repeat(VARS_MAX - 3)).unwrap();
        assert_eq!(
            Error::Full("B".into()).to_string(),
            "B: the variables would hold more than 64 KiB"
        );
    }

    #[test]
    fn an_exported_variable_without_a_value_counts_its_name() {
        let mut v = Vars::new("sh");
        v.set("A", "x".repeat(VARS_MAX - 2)).unwrap();
        v.export("B", None).unwrap();
        assert_eq!(v.export("C", None), Err(Error::Full("C".into())));
        assert_eq!(v.exported().count(), 1, "nothing of C was kept");
        // Unset, a variable frees its room.
        v.unset("A");
        v.export("C", Some("x".repeat(VARS_MAX - 2))).unwrap();
    }

    #[test]
    fn a_program_gets_the_exported_variables_with_a_value_in_export_order() {
        let mut v = Vars::new("sh");
        v.set("Z", String::from("1")).unwrap();
        v.set("NOT", String::from("kept")).unwrap();
        v.export("B", Some(String::from("two words"))).unwrap();
        v.export("Z", None).unwrap();
        v.export("NONE", None).unwrap();
        v.export("A", Some(String::from("été"))).unwrap();
        assert_eq!(
            v.environment(),
            "B=two words\0Z=1\0A=été\0".as_bytes(),
            "in the order of export, not of names"
        );
        // A new value, or exporting it again, keeps a variable's place.
        v.set("B", String::from("2")).unwrap();
        v.export("Z", Some(String::from("3"))).unwrap();
        // Given a value, a variable exported without one goes where it was
        // exported.
        v.set("NONE", String::new()).unwrap();
        assert_eq!(v.environment(), b"B=2\0Z=3\0NONE=\0A=\xc3\xa9t\xc3\xa9\0");
        // Unset and exported again, it goes last.
        v.unset("B");
        v.export("B", Some(String::from("4"))).unwrap();
        assert_eq!(v.environment(), b"Z=3\0NONE=\0A=\xc3\xa9t\xc3\xa9\0B=4\0");
        assert_eq!(v.get("NOT"), "kept");
        assert!(!v.exported().any(|(n, _)| n == "NOT"));
    }

    #[test]
    fn exported_lists_every_exported_variable_by_name() {
        let mut v = Vars::new("sh");
        v.export("OLDPWD", None).unwrap();
        v.export("B", Some(String::from("1"))).unwrap();
        v.set("A", String::from("x")).unwrap();
        assert_eq!(
            v.exported().collect::<Vec<_>>(),
            [("B", Some("1")), ("OLDPWD", None)]
        );
        assert_eq!((v.get("OLDPWD"), v.value("OLDPWD")), ("", None));
        assert_eq!((v.get("B"), v.value("B")), ("1", Some("1")));
        assert_eq!(v.value("UNSET"), None);
    }

    #[test]
    fn an_environment_is_imported_as_exported_variables() {
        let mut v = Vars::new("sh");
        v.import(b"HOME=/root\0A=1=2\0C=\xc3\xa9t\xc3\xa9\0E=\0");
        assert_eq!(
            v.exported().collect::<Vec<_>>(),
            [
                ("A", Some("1=2")),
                ("C", Some("été")),
                ("E", Some("")),
                ("HOME", Some("/root"))
            ]
        );
    }

    #[test]
    fn an_entry_without_a_name_or_not_text_is_dropped() {
        // bash passes `1A=x` and `A-B=y` on to its programs (§10); `=z`
        // and an entry without `=` it drops too.
        let mut v = Vars::new("sh");
        v.import(b"1A=x\0A-B=y\0=z\0noeq\0\0B=\xff\0OK=1\0");
        assert_eq!(v.exported().collect::<Vec<_>>(), [("OK", Some("1"))]);
    }

    #[test]
    fn of_two_entries_with_one_name_the_last_wins() {
        // As bash's import (glibc's `getenv` finds the first).
        let mut v = Vars::new("sh");
        v.import(b"A=1\0B=2\0A=3\0");
        assert_eq!((v.get("A"), v.get("B")), ("3", "2"));
    }

    #[test]
    fn unset_removes_a_variable_exported_or_not() {
        let mut v = Vars::new("sh");
        v.set("A", String::from("1")).unwrap();
        v.export("B", Some(String::from("2"))).unwrap();
        v.unset("A");
        v.unset("B");
        v.unset("NEVER");
        assert_eq!((v.value("A"), v.value("B")), (None, None));
        assert_eq!(v.exported().count(), 0);
        assert_eq!(v.size, 0);
    }
}
