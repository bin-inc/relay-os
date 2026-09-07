use std::process::Command;

const REQUIRED_TOOLS: [&str; 4] = ["rustc", "mke2fs", "sgdisk", "qemu-system-x86_64"];
const REQUIRED_TARGETS: [&str; 2] = ["x86_64-unknown-uefi", "x86_64-unknown-none"];

pub struct ToolReport {
    pub missing: Vec<&'static str>,
}

pub struct CommandOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

impl CommandOutput {
    pub fn success(stdout: impl Into<String>) -> Self {
        Self {
            success: true,
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    pub fn failure(stderr: impl Into<String>) -> Self {
        Self {
            success: false,
            stdout: String::new(),
            stderr: stderr.into(),
        }
    }
}

pub trait DoctorRunner {
    fn run(&mut self, program: &str, args: &[&str]) -> Option<CommandOutput>;
}

impl ToolReport {
    pub fn ready(&self) -> bool {
        self.missing.is_empty()
    }
}

pub fn inspect_with<T, F>(mut probe: F) -> ToolReport
where
    F: FnMut(&str) -> Option<T>,
{
    ToolReport {
        missing: REQUIRED_TOOLS
            .iter()
            .filter(|tool| probe(tool).is_none())
            .copied()
            .collect(),
    }
}

pub fn inspect() -> ToolReport {
    let mut runner = SystemDoctorRunner;
    inspect_with_runner(&mut runner)
}

pub fn inspect_with_runner(runner: &mut impl DoctorRunner) -> ToolReport {
    let mut report = inspect_with(|tool| command_version(runner, tool));
    let installed_targets = installed_targets(runner);

    report.missing.extend(
        REQUIRED_TARGETS
            .iter()
            .filter(|target| {
                !installed_targets
                    .iter()
                    .any(|installed| installed == *target)
            })
            .copied(),
    );

    report
}

pub fn print_versions() {
    let mut runner = SystemDoctorRunner;
    for tool in REQUIRED_TOOLS {
        match command_version(&mut runner, tool) {
            Some(version) => println!("{tool}: {version}"),
            None => println!("{tool}: missing"),
        }
    }

    let installed_targets = installed_targets(&mut runner);
    for target in REQUIRED_TARGETS {
        let status = if installed_targets
            .iter()
            .any(|installed| installed == target)
        {
            "installed"
        } else {
            "missing"
        };
        println!("{target}: {status}");
    }
}

fn command_version(runner: &mut impl DoctorRunner, name: &str) -> Option<String> {
    let version_flag = if name == "mke2fs" { "-V" } else { "--version" };
    let output = runner.run(name, &[version_flag])?;
    if !output.success {
        return None;
    }

    output
        .stdout
        .lines()
        .next()
        .map(str::to_owned)
        .or_else(|| output.stderr.lines().next().map(str::to_owned))
}

fn installed_targets(runner: &mut impl DoctorRunner) -> Vec<String> {
    let Some(output) = runner.run("rustup", &["target", "list", "--installed"]) else {
        return Vec::new();
    };

    if !output.success {
        return Vec::new();
    }

    output.stdout.lines().map(str::to_owned).collect()
}

struct SystemDoctorRunner;

impl DoctorRunner for SystemDoctorRunner {
    fn run(&mut self, program: &str, args: &[&str]) -> Option<CommandOutput> {
        let output = Command::new(program).args(args).output().ok()?;
        Some(CommandOutput {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}
