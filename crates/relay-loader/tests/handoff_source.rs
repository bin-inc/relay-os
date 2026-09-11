const FINAL_MAP_FROM_EBS: &str =
    "let mut final_map = unsafe { boot::exit_boot_services(Some(MemoryType::LOADER_DATA)) };";
const NORMALIZE_FINAL_MAP: &str = "memory::normalize_final_map(boot_info, &mut final_map)";
const TRANSITION: &str =
    "unsafe { transition(boot_info.cast::<BootInfo>(), cr3, stack_top, kernel.entry) }";

fn appears_in_order(source: &str, parts: &[&str]) -> bool {
    let mut cursor = 0;
    for part in parts {
        let Some(offset) = source[cursor..].find(part) else {
            return false;
        };
        cursor += offset + part.len();
    }
    true
}

fn normalize_final_map_implementation(source: &str) -> &str {
    let start = source
        .find("pub(crate) unsafe fn normalize_final_map")
        .unwrap();
    let end = source[start..].find("#[cfg(test)]").unwrap();
    &source[start..start + end]
}

fn boot_function(source: &str) -> &str {
    &source[..source.find("\nfn halt").unwrap()]
}

#[test]
fn production_handoff_uses_real_exit_boot_services_without_diagnostics() {
    let handoff = include_str!("../src/handoff.rs");
    let loader = include_str!("../src/lib.rs");
    let transition = include_str!("../src/transition.rs");

    let boot = boot_function(handoff);

    assert!(!boot.contains("diagnostic"));
    assert!(!loader.contains("diagnostic"));
    assert!(!transition.contains("transition_marker"));
    assert!(appears_in_order(
        boot,
        &[FINAL_MAP_FROM_EBS, NORMALIZE_FINAL_MAP, TRANSITION],
    ));

    let post_ebs = &boot[boot.find(FINAL_MAP_FROM_EBS).unwrap() + FINAL_MAP_FROM_EBS.len()..];
    assert!(!post_ebs.contains("boot::"));
    assert!(!post_ebs.contains("system::"));
    assert!(!post_ebs.contains("allocate_"));
}

#[test]
fn final_map_normalization_uses_the_post_ebs_operation_boundary() {
    let implementation = normalize_final_map_implementation(include_str!("../src/memory.rs"));

    assert!(!implementation.contains("diagnostic"));
    assert!(appears_in_order(
        implementation,
        &[
            "post_ebs::sort_final_map(map, MAX_MEMORY_REGIONS)",
            "normalize_memory_map(data, map)",
        ],
    ));
    assert!(!implementation.contains("map.sort();"));
}
