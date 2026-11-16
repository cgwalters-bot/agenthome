use std::process::Command;

#[test]
fn cli_exposes_only_offline_evaluation() {
    let help = Command::new(env!("CARGO_BIN_EXE_agenthome"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains("summarize-eval"));
    assert!(help.contains("check-eval-readiness"));
    for removed in ["install-tools", "check-install"] {
        assert!(!help.contains(removed));
        let output = Command::new(env!("CARGO_BIN_EXE_agenthome"))
            .arg(removed)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("unrecognized subcommand")
        );
    }
}

#[test]
fn checked_in_readiness_fails_closed_without_output() {
    let output = Command::new(env!("CARGO_BIN_EXE_agenthome"))
        .args(["check-eval-readiness", "eval/readiness.json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("eval readiness blocked")
    );
}
