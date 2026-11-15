use serde_json::Value;
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run_cli(args: &[&str], stdin_data: Option<&str>) -> Output {
    let bin = env!("CARGO_BIN_EXE_smarter-diagrams");
    let mut cmd = Command::new(bin);
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    if stdin_data.is_some() {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }
    let mut child = cmd.spawn().expect("spawn smarter-diagrams");
    if let Some(input) = stdin_data {
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }
    child.wait_with_output().unwrap()
}

#[test]
fn cli_dump_themes_returns_valid_nonempty_builtin_catalog() {
    let out = run_cli(&["--dump-themes"], None);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let themes: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert!(!themes.is_empty(), "expected at least one built-in theme");
    assert!(
        themes
            .iter()
            .any(|t| t["id"].as_str() == Some("catppuccin-mocha")),
        "expected catppuccin-mocha in built-in themes"
    );
    for t in &themes {
        assert!(t["id"].as_str().is_some_and(|s| !s.is_empty()));
        assert!(t["label"].as_str().is_some_and(|s| !s.is_empty()));
        assert!(t["dark"].is_boolean());
        assert!(t["css"].is_string());
    }
}

#[test]
fn cli_formats_check_lint_view_step_and_theme_work_end_to_end() {
    let sample = concat!(
        "a: Client\n",
        "b: API\n",
        "c: Database\n",
        "a -> b: HTTPS\n",
        "b -> c: SQL\n",
        "view api_only { include a -> b }\n",
        "step s1 { include a -> b }\n",
    );

    let check = run_cli(&["-", "--format", "check"], Some(sample));
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    let check_msg = String::from_utf8_lossy(&check.stdout);
    assert!(check_msg.contains("3 nodes, 2 edges; geometry valid"));

    let lint = run_cli(&["-", "--format", "lint"], Some(sample));
    assert!(
        lint.status.success(),
        "{}",
        String::from_utf8_lossy(&lint.stderr)
    );
    let findings: Vec<Value> = serde_json::from_slice(&lint.stdout).unwrap();
    assert!(
        findings.is_empty(),
        "simple chain should lint cleanly: {findings:?}"
    );

    let view_svg = run_cli(&["-", "--view", "api_only"], Some(sample));
    assert!(
        view_svg.status.success(),
        "{}",
        String::from_utf8_lossy(&view_svg.stderr)
    );
    let view_text = String::from_utf8_lossy(&view_svg.stdout);
    assert!(view_text.contains("data-node=\"a\"") && !view_text.contains("data-node=\"c\""));

    let step_svg = run_cli(
        &["-", "--step", "s1", "--theme", "catppuccin-mocha"],
        Some(sample),
    );
    assert!(
        step_svg.status.success(),
        "{}",
        String::from_utf8_lossy(&step_svg.stderr)
    );
    let step_text = String::from_utf8_lossy(&step_svg.stdout);
    assert!(step_text.contains("data-explicit-theme=\"catppuccin-mocha\""));
    assert!(step_text.contains("data-node=\"a\"") && !step_text.contains("data-node=\"c\""));
}

#[test]
fn cli_edit_subcommand_and_output_flag_preserve_valid_diagram() {
    let source = "a: Client\nb: Server\na -> b\n";
    let ops = serde_json::to_string(&[smarter_diagrams::edit::DiagramEditOp::PinNode {
        node: "a".into(),
        pin: smarter_diagrams::edit::PinExpr::AbsoluteGrid { x: 4, y: 8 },
        size_lock: None,
    }])
    .unwrap();
    let edited = run_cli(&["edit", "--operations", &ops], Some(source));
    assert!(
        edited.status.success(),
        "{}",
        String::from_utf8_lossy(&edited.stderr)
    );
    let new_src = String::from_utf8_lossy(&edited.stdout);
    assert!(new_src.contains("a -> b"), "{new_src}");
    assert!(new_src.contains("@pin(grid: 4, 8)"), "{new_src}");

    let temp_dir = std::env::temp_dir();
    let out_path = temp_dir.join(format!(
        "smarter-diagrams-cli-test-{}.svg",
        std::process::id()
    ));
    let written = run_cli(&["-", "-o", out_path.to_str().unwrap()], Some(&new_src));
    assert!(
        written.status.success(),
        "{}",
        String::from_utf8_lossy(&written.stderr)
    );
    let file_svg = std::fs::read_to_string(&out_path).unwrap();
    let _ = std::fs::remove_file(&out_path);
    assert!(file_svg.contains("<svg") && file_svg.contains("data-node=\"a\""));
}

#[test]
fn cli_rejects_invalid_arguments_and_malformed_inputs() {
    let unknown_flag = run_cli(&["--does-not-exist"], None);
    assert!(!unknown_flag.status.success());

    let unknown_theme = run_cli(&["-", "--theme", "nonexistent-theme"], Some("a -> b\n"));
    assert!(!unknown_theme.status.success());
    assert!(
        String::from_utf8_lossy(&unknown_theme.stderr).contains("unknown theme"),
        "{}",
        String::from_utf8_lossy(&unknown_theme.stderr)
    );

    let conflict = run_cli(
        &["-", "--view", "v1", "--step", "s1"],
        Some("a -> b\nview v1 { include a }\nstep s1 { include a }\n"),
    );
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("cannot combine --view and --step"));

    let syntax_err = run_cli(&["-"], Some("a ->\n"));
    assert!(!syntax_err.status.success());
}
