use relay_xtask::doctor::{CommandOutput, DoctorRunner};

struct Runner {
    missing_target: Option<&'static str>,
    failing_tool: Option<&'static str>,
}

impl Runner {
    fn ready() -> Self {
        Self {
            missing_target: None,
            failing_tool: None,
        }
    }
}

impl DoctorRunner for Runner {
    fn run(&mut self, program: &str, _args: &[&str]) -> Option<CommandOutput> {
        if self.failing_tool == Some(program) {
            return Some(CommandOutput::failure("tool failed"));
        }

        if program == "rustup" {
            let targets = ["x86_64-unknown-uefi", "x86_64-unknown-none"]
                .into_iter()
                .filter(|target| Some(*target) != self.missing_target)
                .collect::<Vec<_>>()
                .join("\n");
            return Some(CommandOutput::success(targets));
        }

        Some(CommandOutput::success(format!("{program} version")))
    }
}

#[test]
fn doctor_reports_every_required_tool() {
    let report = relay_xtask::doctor::inspect_with(|name| match name {
        "rustc" => Some("rustc 1.98.1"),
        "mke2fs" => Some("mke2fs 1.47.2"),
        "sgdisk" => Some("GPT fdisk 1.0.10"),
        "qemu-system-x86_64" => None,
        _ => None,
    });
    assert_eq!(report.missing, ["qemu-system-x86_64"]);
    assert!(!report.ready());
}

#[test]
fn doctor_reports_ready_when_commands_and_targets_succeed() {
    let report = relay_xtask::doctor::inspect_with_runner(&mut Runner::ready());

    assert!(report.ready());
    assert!(report.missing.is_empty());
}

#[test]
fn doctor_reports_missing_required_target() {
    let mut runner = Runner::ready();
    runner.missing_target = Some("x86_64-unknown-none");

    let report = relay_xtask::doctor::inspect_with_runner(&mut runner);

    assert_eq!(report.missing, ["x86_64-unknown-none"]);
}

#[test]
fn doctor_reports_nonzero_tool_command() {
    let mut runner = Runner::ready();
    runner.failing_tool = Some("qemu-system-x86_64");

    let report = relay_xtask::doctor::inspect_with_runner(&mut runner);

    assert_eq!(report.missing, ["qemu-system-x86_64"]);
}
