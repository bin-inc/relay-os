//! The TLB: dropping the entries of pages the page tables no longer map
//! (user-space gate §3.1: the page-table format and its caches stay under
//! `arch/`).

/// Drops the TLB's entry for the page at `virt`.
pub fn flush(virt: u64) {
    x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(virt));
}

/// Drops every entry of the TLB that is not global.
pub fn flush_all() {
    x86_64::instructions::tlb::flush_all();
}

/// The loader's reader of a source file's code without its comments.
#[cfg(test)]
#[path = "../../../boot/src/code.rs"]
mod code;

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// The kernel's source files outside `arch/`, with their paths.
    fn sources(dir: &Path, out: &mut Vec<(String, String)>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let path = e.unwrap().path();
            if path.is_dir() {
                if path.file_name().unwrap() != "arch" {
                    sources(&path, out);
                }
            } else if path.extension().is_some_and(|x| x == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                out.push((path.display().to_string(), text));
            }
        }
    }

    /// Whether `src` names the TLB other than through `arch::tlb` in its
    /// code, not its comments: a `tlb` path however it is imported, or the
    /// `invlpg` or `invpcid` instruction in any case.
    fn flushes_the_tlb(src: &str) -> bool {
        let code = &super::code::code(src);
        let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
        let lower = code.to_ascii_lowercase();
        lower.contains("invlpg")
            || lower.contains("invpcid")
            || code.match_indices("tlb").any(|(i, _)| {
                let before = code[..i].chars().next_back();
                let after = code[i + 3..].chars().next();
                !before.is_some_and(word)
                    && !after.is_some_and(word)
                    && !code[..i].ends_with("arch::")
            })
    }

    #[test]
    fn only_arch_flushes_the_tlb() {
        let mut files = Vec::new();
        sources(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        assert!(
            files.iter().any(|(p, _)| p.ends_with("mm/mod.rs")),
            "the walk found the sources"
        );
        for (path, text) in files {
            assert!(!flushes_the_tlb(&text), "{path} flushes the TLB itself");
        }
    }

    #[test]
    fn the_scan_sees_every_way_to_name_the_tlb() {
        // The prototype's review: a `tlb` imported in braces passed the
        // first scan, which looked for `instructions::tlb` only.
        for code in [
            "x86_64::instructions::tlb::flush(v);",
            "use x86_64::instructions::{interrupts, tlb};\ntlb::flush(v);",
            "use x86_64::instructions::tlb as t;",
            "unsafe { asm!(\"invlpg [{}]\", in(reg) v) };",
            // Milestone 2's plan 5's deferred finding: `invpcid` drops
            // entries too (and `invlpga`, `invlpgb` hold `invlpg`).
            "unsafe { asm!(\"invpcid {}, [{}]\", in(reg) kind, in(reg) d) };",
            // The prototype's review: the assembler takes either case.
            "unsafe { asm!(\"INVLPG [{}]\", in(reg) v) };",
            "unsafe { asm!(\"InvPcid {}, [{}]\", in(reg) kind, in(reg) d) };",
        ] {
            assert!(flushes_the_tlb(code), "{code}");
        }
        for code in [
            "arch::tlb::flush(virt);",
            "arch::tlb::flush_all();",
            "let tlbs = 1;",
            // Milestone 3's deferred minor: a comment is no code, whatever
            // it names.
            "// INVPCID would drop them all.\nlet x = 1;",
            "/// Like `invlpg`, through the tlb module.\nfn f() {}",
        ] {
            assert!(!flushes_the_tlb(code), "{code}");
        }
    }
}
