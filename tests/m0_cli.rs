//! M0 CLI integration tests.
//! These tests exercise the compiled `chub` binary via subprocess.

use std::process::Command;

fn chub_bin() -> Command {
    let bin = env!("CARGO_BIN_EXE_chub");
    Command::new(bin)
}

#[test]
fn cli_no_args_prints_usage() {
    let output = chub_bin().output().expect("failed to run chub");
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should print usage to stderr and exit successfully
    assert!(output.status.success(), "exit code should be 0");
    assert!(
        stderr.contains("chub"),
        "usage should mention 'chub': {stderr}"
    );
    assert!(
        stderr.contains("search"),
        "usage should mention 'search': {stderr}"
    );
}

#[test]
fn cli_help_shows_all_commands() {
    let output = chub_bin()
        .arg("--help")
        .output()
        .expect("failed to run chub --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stdout}{}", String::from_utf8_lossy(&output.stderr));

    for cmd in &[
        "search", "get", "annotate", "feedback", "update", "cache", "build",
    ] {
        assert!(
            combined.contains(cmd),
            "--help should list '{cmd}': {combined}"
        );
    }
    assert!(
        combined.contains("--json"),
        "--help should list '--json': {combined}"
    );
}

#[test]
fn cli_version_aliases_match_package_version() {
    let pkg_version = env!("CARGO_PKG_VERSION");

    // -V flag
    let output_v = chub_bin()
        .arg("-V")
        .output()
        .expect("failed to run chub -V");
    let stdout_v = String::from_utf8_lossy(&output_v.stdout);
    assert!(
        stdout_v.contains(pkg_version),
        "-V should contain '{pkg_version}': {stdout_v}"
    );

    // --cli-version flag
    let output_cv = chub_bin()
        .arg("--cli-version")
        .output()
        .expect("failed to run chub --cli-version");
    let stdout_cv = String::from_utf8_lossy(&output_cv.stdout);
    assert!(
        stdout_cv.contains(pkg_version),
        "--cli-version should contain '{pkg_version}': {stdout_cv}"
    );
}
