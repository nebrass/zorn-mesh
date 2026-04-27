#![doc = "Command skeleton for the public zornmesh CLI."]

pub const ROOT_HELP: &str = include_str!("../../../fixtures/cli/root-help.stdout");
pub const TRACE_HELP: &str = include_str!("../../../fixtures/cli/trace-help.stdout");
pub const VERSION: &str = "zornmesh 0.1.0\n";

pub fn run(args: impl IntoIterator<Item = String>) -> i32 {
    let args = args.into_iter().collect::<Vec<_>>();
    match args.as_slice() {
        [] => {
            print!("{ROOT_HELP}");
            0
        }
        [flag] if flag == "--help" || flag == "-h" || flag == "help" => {
            print!("{ROOT_HELP}");
            0
        }
        [flag] if flag == "--version" || flag == "-V" => {
            print!("{VERSION}");
            0
        }
        [command] if command == "trace" => {
            print!("{TRACE_HELP}");
            0
        }
        [command, flag]
            if command == "trace" && (flag == "--help" || flag == "-h" || flag == "help") =>
        {
            print!("{TRACE_HELP}");
            0
        }
        [command, ..] => {
            eprintln!("E_UNSUPPORTED_COMMAND: unsupported zornmesh command '{command}'");
            64
        }
    }
}
