use std::{
    env,
    path::{Path, PathBuf},
    process::{self, Command},
};

const HELP: &str = "\
zornmesh xtask
Workspace automation for zornmesh.

Usage: cargo xtask <COMMAND>

Commands:
  check              Run workspace compile checks
  test               Run Rust and Bun tests
  lint               Run rustfmt and clippy
  docs               Build Rust documentation
  conformance        Run scaffold conformance smoke checks
  generate           Run code-generation tasks
  fixtures           Check fixture ownership and golden outputs
  release-preflight  Run release-preflight checks
  help               Print xtask help
";

fn main() {
    let code = match run(env::args().skip(1)) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{error}");
            1
        }
    };
    process::exit(code);
}

fn run(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let mut args = args.into_iter();
    let command = args.next().unwrap_or_else(|| "help".to_owned());
    let rest = args.collect::<Vec<_>>();
    let root = workspace_root()?;

    match command.as_str() {
        "--help" | "-h" | "help" => {
            print!("{HELP}");
            Ok(())
        }
        "check" => check(&root),
        "test" => test(&root),
        "lint" => lint(&root),
        "docs" => docs(&root),
        "conformance" => conformance(&root),
        "generate" => generate(),
        "fixtures" => fixtures(&root, &rest),
        "release-preflight" => release_preflight(&root),
        other => Err(format!("unknown xtask command '{other}'\n\n{HELP}")),
    }
}

fn check(root: &Path) -> Result<(), String> {
    require_tool("cargo")?;
    run_tool("cargo", &["check", "--workspace", "--all-targets"], root)
}

fn test(root: &Path) -> Result<(), String> {
    require_tool("cargo")?;
    require_tool("bun")?;
    run_tool("cargo", &["test", "--workspace", "--all-targets"], root)?;
    run_tool("bun", &["test"], &root.join("sdks/typescript"))?;
    run_tool("bun", &["test"], &root.join("apps/local-ui"))
}

fn lint(root: &Path) -> Result<(), String> {
    require_tool("cargo")?;
    require_tool("rustfmt")?;
    run_tool("cargo", &["fmt", "--all", "--check"], root)?;
    run_tool(
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
        root,
    )
}

fn docs(root: &Path) -> Result<(), String> {
    require_tool("cargo")?;
    run_tool("cargo", &["doc", "--workspace", "--no-deps"], root)
}

fn conformance(root: &Path) -> Result<(), String> {
    require_tool("cargo")?;
    run_tool(
        "cargo",
        &[
            "test",
            "-p",
            "zornmesh-proto",
            "--test",
            "envelope_round_trip",
        ],
        root,
    )?;
    run_tool(
        "cargo",
        &["test", "-p", "zornmesh", "--test", "golden_help"],
        root,
    )?;
    run_tool(
        "cargo",
        &["test", "-p", "zornmesh", "--test", "daemon_help"],
        root,
    )
}

fn generate() -> Result<(), String> {
    println!("zornmesh scaffold: no generation tasks registered");
    Ok(())
}

fn fixtures(root: &Path, args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("Usage: cargo xtask fixtures [--check]");
        return Ok(());
    }
    let check_only = args.is_empty() || args == ["--check"];
    if !check_only {
        return Err(format!(
            "unknown fixtures arguments: {}\nUsage: cargo xtask fixtures [--check]",
            args.join(" ")
        ));
    }

    for path in [
        "fixtures/cli/README.md",
        "fixtures/cli/daemon-help.stdout",
        "fixtures/cli/root-help.stdout",
        "fixtures/cli/trace-help.stdout",
        "fixtures/errors/README.md",
        "fixtures/errors/manifest.toml",
        "conformance/README.md",
        "conformance/manifest.toml",
        "conformance/ui/README.md",
        "conformance/ui/manifest.toml",
        "test-infra/README.md",
        "test-infra/manifest.toml",
        "fixtures/ui/README.md",
        "fixtures/ui/quality-readiness.json",
    ] {
        let path = root.join(path);
        if !path.is_file() {
            return Err(format!(
                "missing required fixture ownership file: {}",
                path.display()
            ));
        }
    }

    println!("zornmesh fixtures: ownership files present");
    Ok(())
}

fn release_preflight(root: &Path) -> Result<(), String> {
    check(root)?;
    test(root)?;
    lint(root)?;
    docs(root)?;
    conformance(root)
}

fn require_tool(tool: &str) -> Result<(), String> {
    let status = Command::new(tool)
        .arg("--version")
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .status()
        .map_err(|_| format!("missing required tool: {tool}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("required tool '{tool}' failed --version check"))
    }
}

fn run_tool(program: &str, args: &[&str], cwd: &Path) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .status()
        .map_err(|error| {
            format!(
                "failed to run {program} {} in {}: {error}",
                args.join(" "),
                cwd.display()
            )
        })?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{program} {} failed with status {status}",
            args.join(" ")
        ))
    }
}

fn workspace_root() -> Result<PathBuf, String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| "failed to resolve workspace root".to_owned())
}
