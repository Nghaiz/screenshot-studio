//! `ssctl`: agent-verifiable CLI over `ss-core` (design-intent.md § `ssctl` surface).
//! Stub until phase 1.2: every invocation prints usage and exits 2 (usage error).

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("ssctl: no subcommands yet (phase 1.2 adds add-root, index, list, stats, ...)");
    eprintln!("usage: ssctl [--data-dir <path>] <subcommand> [--json]");
    ExitCode::from(2)
}
