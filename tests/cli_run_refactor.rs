use std::process::Command;

fn run_tokenslim(args: &[&str]) -> std::process::Output {
    let bin = env!("CARGO_BIN_EXE_tokenslim");
    Command::new(bin)
        .args(args)
        .output()
        .expect("failed to launch tokenslim")
}

#[test]
fn cli_no_args_prints_usage_and_exits_success() {
    let output = run_tokenslim(&[]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("USAGE:"));
    assert!(stdout.contains("tokenslim"));
}

#[test]
fn cli_implicit_run_still_works_after_run_cli_split() {
    let output = if cfg!(windows) {
        run_tokenslim(&["cmd", "/C", "echo", "hello_run"])
    } else {
        run_tokenslim(&["sh", "-lc", "printf hello_run"])
    };
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hello_run"));
}

#[test]
fn cli_explicit_run_still_works_after_run_cli_split() {
    let output = if cfg!(windows) {
        run_tokenslim(&["run", "cmd", "/C", "echo", "hello_explicit"])
    } else {
        run_tokenslim(&["run", "sh", "-lc", "printf hello_explicit"])
    };
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hello_explicit"));
}
