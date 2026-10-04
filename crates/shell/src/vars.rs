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

/// The most a program's environment holds, as `spawn` takes it
/// (programmable shell gate §8.1, §8.5).
pub const ENVIRONMENT_MAX: usize = 64 * 1024;

/// A shell's variables and arguments.
pub(crate) struct Vars {
    names: BTreeMap<String, Var>,
    /// The bytes of their names and values.
    size: usize,
    /// `$0`, then `$1` on.
    pub(crate) args: Vec<String>,
    /// The place the next variable exported takes.
    next: u64,
    /// While a built-in runs with assignments before it: each assigned
    /// name with what it was before, and whether the built-in has set or
    /// exported it since.
    held: Vec<Held>,
}

/// An assignment before a built-in, held while it runs.
struct Held {
    name: String,
    before: Option<Var>,
    changed: bool,
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
            held: Vec::new(),
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

    /// The variable `name`'s value, if it has one.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.names.get(name).and_then(|v| v.value.as_deref())
    }

    /// What a shell sets when it starts, after its import (programmable
    /// shell gate §8.5): `PWD`, exported, the current directory `cwd`, in
    /// its place if it was imported; and `OLDPWD` exported without a value,
    /// unless it was imported naming a directory, as bash's are. The first
    /// that does not fit `VARS_MAX` is the error, the other still set.
    pub fn start(&mut self, cwd: &str, oldpwd_is_dir: bool) -> Result<(), Error> {
        let pwd = self.export("PWD", Some(String::from(cwd)));
        let mut oldpwd = Ok(());
        if !oldpwd_is_dir {
            self.unset("OLDPWD");
            oldpwd = self.export("OLDPWD", None);
        }
        pwd.and(oldpwd)
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
        )?;
        self.changed(name);
        Ok(())
    }

    /// Takes `name`'s value away, exported or not as it was, as bash's
    /// `cd` does to `OLDPWD` when `PWD` has none: it stays a variable.
    pub fn clear(&mut self, name: &str) -> Result<(), Error> {
        let export = self.names.get(name).and_then(|v| v.export);
        self.put(
            name,
            Var {
                value: None,
                export,
            },
        )?;
        self.changed(name);
        Ok(())
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
        self.changed(name);
        Ok(())
    }

    /// Sets the assignments before a built-in, `NAME=value` each, while it
    /// runs (programmable shell gate §15 item 7); [`Vars::release`] puts
    /// back what they replaced. One that does not fit puts back those
    /// before it.
    pub fn hold(&mut self, assigns: &[String]) -> Result<(), Error> {
        let mut set = Ok(());
        for (name, value) in assigns.iter().filter_map(|a| a.split_once('=')) {
            if !self.held.iter().any(|h| h.name == name) {
                let before = self.names.get(name).cloned();
                self.held.push(Held {
                    name: String::from(name),
                    before,
                    changed: false,
                });
            }
            set = self.set(name, String::from(value));
            if set.is_err() {
                break;
            }
        }
        // Setting them was not the built-in's doing.
        for h in &mut self.held {
            h.changed = false;
        }
        if set.is_err() {
            self.release();
        }
        set
    }

    /// The built-in has ended: each name it held is as it was before,
    /// unless the built-in set or exported it, as bash's are (`A=1 export
    /// A` keeps `A=1`, `C=1 unset C` brings back the old `C`).
    pub fn release(&mut self) {
        for h in core::mem::take(&mut self.held) {
            if h.changed {
                continue;
            }
            if let Some(old) = self.names.remove(&h.name) {
                self.size -= size(&h.name, &old);
            }
            if let Some(before) = h.before {
                // It fitted before, so it is put back whatever the
                // built-in added.
                self.size += size(&h.name, &before);
                self.names.insert(h.name, before);
            }
        }
    }

    /// Notes that `name` was set or exported, for a built-in holding it.
    fn changed(&mut self, name: &str) {
        if let Some(h) = self.held.iter_mut().find(|h| h.name == name) {
            h.changed = true;
        }
    }

    /// Removes the variable `name`, exported or not.
    pub fn unset(&mut self, name: &str) {
        if let Some(old) = self.names.remove(name) {
            self.size -= size(name, &old);
        }
    }

    /// Every exported variable, by name: its value, if it has one. A name
    /// a built-in holds is as it was before, as bash's `export -p` lists it.
    pub fn exported(&self) -> impl Iterator<Item = (&str, Option<&str>)> {
        self.names.iter().filter_map(|(n, v)| {
            let v = match self.held.iter().find(|h| h.name == *n) {
                Some(h) => h.before.as_ref()?,
                None => v,
            };
            v.export?;
            Some((n.as_str(), v.value.as_deref()))
        })
    }

    /// The environment of a program the shell starts (programmable shell
    /// gate §8.5): each exported variable that has a value, `NAME=value` and
    /// a NUL, in the order of export; with `assigns` before it (`A=1 cmd`,
    /// `NAME=value` each), an exported name keeps its place with the value
    /// assigned, and the others follow in the order typed, of a name
    /// assigned twice the last value.
    pub fn environment_with(&self, assigns: &[String]) -> Vec<u8> {
        let mut over: Vec<(&str, &str)> = Vec::new();
        for a in assigns {
            let Some((name, value)) = a.split_once('=') else {
                continue;
            };
            match over.iter_mut().find(|(n, _)| *n == name) {
                Some(o) => o.1 = value,
                None => over.push((name, value)),
            }
        }
        let assigned = |name: &str| over.iter().find(|(n, _)| *n == name).map(|o| o.1);
        let mut exported: Vec<(u64, &str, &str)> = self
            .names
            .iter()
            .filter_map(|(n, v)| {
                let value = assigned(n).or(v.value.as_deref())?;
                Some((v.export?, n.as_str(), value))
            })
            .collect();
        exported.sort_unstable_by_key(|&(place, _, _)| place);
        let rest = over.iter().filter(|(n, _)| !self.is_exported(n));
        let mut block = Vec::new();
        for (name, value) in exported
            .iter()
            .map(|&(_, n, v)| (n, v))
            .chain(rest.copied())
        {
            block.extend_from_slice(name.as_bytes());
            block.push(b'=');
            block.extend_from_slice(value.as_bytes());
            block.push(0);
        }
        block
    }

    /// Whether `name` is exported.
    fn is_exported(&self, name: &str) -> bool {
        self.names.get(name).is_some_and(|v| v.export.is_some())
    }

    /// Stores `var` as `name`, unless the variables would then hold more
    /// than `VARS_MAX`, or would once a built-in releases what it holds.
    fn put(&mut self, name: &str, var: Var) -> Result<(), Error> {
        let old = self.names.get(name).map_or(0, |v| size(name, v));
        let new = self.size - old + size(name, &var);
        if new + self.held_back(name, &var) > VARS_MAX {
            return Err(Error::Full(String::from(name)));
        }
        self.size = new;
        self.names.insert(String::from(name), var);
        Ok(())
    }
}

impl Vars {
    /// What releasing the names a built-in holds would add, were `name` to
    /// hold `var`: each held name counts the larger of its size before and
    /// its size now, so that it fits when it comes back.
    fn held_back(&self, name: &str, var: &Var) -> usize {
        self.held
            .iter()
            .map(|h| {
                let before = h.before.as_ref().map_or(0, |v| size(&h.name, v));
                let now = if h.name == name {
                    size(name, var)
                } else {
                    self.names.get(&h.name).map_or(0, |v| size(&h.name, v))
                };
                before.saturating_sub(now)
            })
            .sum()
    }
}

/// What a variable counts towards `VARS_MAX`: its name and its value.
fn size(name: &str, var: &Var) -> usize {
    name.len() + var.value.as_ref().map_or(0, String::len)
}

#[cfg(test)]
impl Vars {
    /// The variable `name`'s value; an unset one, or one without a value,
    /// is empty.
    pub fn get(&self, name: &str) -> &str {
        self.value(name).unwrap_or("")
    }

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
            v.environment_with(&[]),
            "B=two words\0Z=1\0A=été\0".as_bytes(),
            "in the order of export, not of names"
        );
        // A new value, or exporting it again, keeps a variable's place.
        v.set("B", String::from("2")).unwrap();
        v.export("Z", Some(String::from("3"))).unwrap();
        // Given a value, a variable exported without one goes where it was
        // exported.
        v.set("NONE", String::new()).unwrap();
        assert_eq!(
            v.environment_with(&[]),
            b"B=2\0Z=3\0NONE=\0A=\xc3\xa9t\xc3\xa9\0"
        );
        // Unset and exported again, it goes last.
        v.unset("B");
        v.export("B", Some(String::from("4"))).unwrap();
        assert_eq!(
            v.environment_with(&[]),
            b"Z=3\0NONE=\0A=\xc3\xa9t\xc3\xa9\0B=4\0"
        );
        assert_eq!(v.get("NOT"), "kept");
        assert!(!v.exported().any(|(n, _)| n == "NOT"));
    }

    #[test]
    fn assignments_before_a_program_go_over_the_exported_variables() {
        let mut v = Vars::new("sh");
        v.export("HOME", Some(String::from("/root"))).unwrap();
        v.export("B", None).unwrap();
        v.export("X", Some(String::from("1"))).unwrap();
        v.set("L", String::from("local")).unwrap();
        let assigns = ["X=2", "N=new", "B=b", "L=l=m", "N=last"].map(String::from);
        assert_eq!(
            v.environment_with(&assigns),
            b"HOME=/root\0B=b\0X=2\0N=last\0L=l=m\0",
            "exported names in their places, the others as typed, the last value"
        );
        // The shell's own are as they were.
        assert_eq!((v.get("X"), v.value("B"), v.get("L")), ("1", None, "local"));
    }

    #[test]
    fn assignments_held_for_a_built_in_come_back_unless_it_set_them() {
        let mut v = Vars::new("sh");
        v.set("C", String::from("0")).unwrap();
        v.export("H", Some(String::from("/root"))).unwrap();
        v.hold(&["H=/d", "C=1", "N=new", "S=x", "E=y"].map(String::from))
            .unwrap();
        assert_eq!((v.get("H"), v.get("C"), v.get("N")), ("/d", "1", "new"));
        // Listed as they were before.
        assert_eq!(v.exported().collect::<Vec<_>>(), [("H", Some("/root"))]);
        // What the built-in does: `unset C`, `S=set`, `export E`.
        v.unset("C");
        v.set("S", String::from("set")).unwrap();
        v.export("E", None).unwrap();
        v.release();
        assert_eq!(
            (v.value("H"), v.value("C"), v.value("N")),
            (Some("/root"), Some("0"), None),
            "put back"
        );
        assert_eq!((v.get("S"), v.get("E")), ("set", "y"), "kept");
        assert_eq!(
            v.exported().collect::<Vec<_>>(),
            [("E", Some("y")), ("H", Some("/root"))]
        );
        assert_eq!(v.size, "C0H/rootSsetEy".len());
    }

    #[test]
    fn a_held_name_keeps_room_for_what_it_held() {
        // `A=<40K>; A= export B=<40K>` left the variables over `VARS_MAX`
        // once `A` came back, and nothing more could be set (plan 3's final
        // review, m-1): while held, a name counts the larger of its sizes.
        let mut v = Vars::new("sh");
        let big = "x".repeat(40 * 1024);
        v.set("A", big.clone()).unwrap();
        v.hold(&[String::from("A=")]).unwrap();
        assert_eq!(
            v.export("B", Some(big.clone())),
            Err(Error::Full("B".into()))
        );
        v.set("B", "x".repeat(VARS_MAX - big.len() - 2)).unwrap();
        v.release();
        assert_eq!((v.value("A"), v.size), (Some(big.as_str()), VARS_MAX));
        // Removed while held, it still comes back.
        v.unset("B");
        v.hold(&[String::from("A=")]).unwrap();
        v.unset("A");
        assert_eq!(v.set("C", big.clone()), Err(Error::Full("C".into())));
        v.release();
        assert_eq!(v.value("A"), Some(big.as_str()));
        // Given its old value again while held, it fits as it did.
        v.hold(&[String::from("A=")]).unwrap();
        v.set("A", big.clone()).unwrap();
        v.release();
        assert_eq!(v.value("A"), Some(big.as_str()));
        // A held name given more counts what it holds now, once: the rest
        // fits exactly.
        v.hold(&[alloc::format!("A={big}y")]).unwrap();
        v.set("D", "x".repeat(VARS_MAX - big.len() - 3)).unwrap();
        assert_eq!(v.size, VARS_MAX);
        v.release();
        assert_eq!((v.value("A"), v.size), (Some(big.as_str()), VARS_MAX - 1));
    }

    #[test]
    fn an_assignment_that_does_not_fit_puts_back_the_ones_before() {
        let mut v = Vars::new("sh");
        v.set("A", String::from("old")).unwrap();
        let big = alloc::format!("B={}", "x".repeat(VARS_MAX));
        assert_eq!(
            v.hold(&[String::from("A=new"), big]),
            Err(Error::Full("B".into()))
        );
        assert_eq!((v.get("A"), v.value("B")), ("old", None));
        assert_eq!(v.size, 4);
        // Nothing is held any more.
        v.set("A", String::from("x")).unwrap();
        v.release();
        assert_eq!(v.get("A"), "x");
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
    fn a_shell_starts_with_pwd_and_oldpwd_exported() {
        let mut v = Vars::new("sh");
        v.import(b"HOME=/root\0");
        v.start("/tmp", false).unwrap();
        assert_eq!(v.environment_with(&[]), b"HOME=/root\0PWD=/tmp\0");
        assert_eq!(
            v.exported().collect::<Vec<_>>(),
            [
                ("HOME", Some("/root")),
                ("OLDPWD", None),
                ("PWD", Some("/tmp"))
            ]
        );
        // An imported `PWD` keeps its place with the directory's path; an
        // imported `OLDPWD` is kept if it names a directory.
        let mut v = Vars::new("sh");
        v.import(b"PWD=/elsewhere\0OLDPWD=/etc\0A=1\0");
        v.start("/tmp", true).unwrap();
        assert_eq!(v.environment_with(&[]), b"PWD=/tmp\0OLDPWD=/etc\0A=1\0");
        v.start("/tmp", false).unwrap();
        assert_eq!(v.environment_with(&[]), b"PWD=/tmp\0A=1\0");
        assert_eq!(v.value("OLDPWD"), None);
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
