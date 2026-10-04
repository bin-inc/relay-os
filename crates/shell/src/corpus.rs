//! The bash corpus (programmable shell gate §11.2): every script in
//! `tests/corpus/` runs under the host's bash 5.2 and under the in-process
//! runner, read as `X | sh` reads its input (no trace), and must print the
//! same and end with the same status. Each starts in an empty directory of
//! its own, where it may make files by relative names: bash's under the
//! workspace's `target/corpus/`, the runner's `/tmp/corpus`. A script
//! writes nothing to standard error under bash (messages are tested
//! against bash apart, as bash names a script's line in them) and holds no
//! unquoted value with a blank in it (expansion never splits words here).
//! A missing bash fails the test.
#![cfg(test)]

use crate::testing::Harness;
use crate::{Bytes, Shell};
use alloc::string::String;
use alloc::vec::Vec;
use std::path::{Path, PathBuf};
use std::process::Command;
use vfs::Vfs;

/// The scripts, sorted by name.
fn scripts() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut scripts: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "sh"))
        .collect();
    scripts.sort();
    scripts
}

/// The environment both shells start with.
const ENVIRONMENT: &[u8] = b"LC_ALL=C\0PATH=/usr/bin:/bin\0";

/// What bash 5.2 prints for `script` and its status, in the C locale with
/// no startup files, in an empty directory.
fn bash(script: &Path) -> (i32, String) {
    let name = script.file_stem().unwrap();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/corpus")
        .join(std::format!("{}-{}", std::process::id(), name.display()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = Command::new("bash")
        .args(["--norc", "--noprofile"])
        .arg(script)
        .current_dir(&dir)
        .env_clear()
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap_or_else(|e| panic!("the host's bash is needed: {e}"));
    assert!(
        out.stderr.is_empty(),
        "{}: bash wrote to standard error: {}",
        script.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("bash's output is text");
    std::fs::remove_dir_all(&dir).unwrap();
    (out.status.code().expect("bash exited"), stdout)
}

/// What the in-process runner prints for `text` and its status, in an
/// empty directory, with bash's environment.
fn relay(text: &[u8]) -> (i32, String) {
    let mut h = Harness::new();
    h.dir("/tmp/corpus");
    h.vfs.chdir(b"/tmp/corpus").unwrap();
    let mut input = Bytes::new(text.to_vec());
    let status = Shell::new(&mut h.vfs, &mut h.console, &mut h.system)
        .with_environment(ENVIRONMENT)
        .run_input(&mut input);
    (status, h.console.take())
}

#[test]
fn the_host_s_bash_is_5_2() {
    let out = Command::new("bash")
        .arg("--version")
        .output()
        .unwrap_or_else(|e| panic!("the host's bash is needed: {e}"));
    let version = String::from_utf8_lossy(&out.stdout);
    assert!(version.contains("version 5.2."), "{version}");
}

#[test]
fn every_corpus_script_does_what_bash_does() {
    let scripts = scripts();
    assert!(!scripts.is_empty(), "no scripts in tests/corpus");
    let mut wrong = Vec::new();
    for script in &scripts {
        let want = bash(script);
        let got = relay(&std::fs::read(script).unwrap());
        if got != want {
            wrong.push(std::format!(
                "{}: bash {want:?}, relay-sh {got:?}",
                script.display()
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
