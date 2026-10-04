//! A shell's variables and arguments (user-space gate §9.4), and which
//! variables it exports (programmable shell gate §8.5): an exported
//! variable goes into the environment of every program the shell starts,
//! in the order the variables were exported.

use crate::expand::Error;
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
