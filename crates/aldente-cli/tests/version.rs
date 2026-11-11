//! End-to-end check of `aldente --version` against the built binary.

use std::process::Command;

#[test]
fn version_flag_prints_version_and_exits_zero() {
    let output = Command::new(env!("CARGO_BIN_EXE_aldente"))
        .arg("--version")
        .output()
        .expect("aldente binary should start");

    assert!(output.status.success(), "exit status: {}", output.status);
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert_eq!(
        stdout.trim_end(),
        format!("aldente {}", env!("CARGO_PKG_VERSION"))
    );
}
