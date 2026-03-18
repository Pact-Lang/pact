// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Integration tests for the PACT CLI binary (`pact`).
//!
//! Each test invokes the CLI via `cargo run -p pact-lang --` to exercise
//! real command-line behaviour end-to-end.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Return the workspace root (two levels up from this crate's manifest dir).
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // crates/
        .unwrap()
        .parent() // workspace root
        .unwrap()
        .to_path_buf()
}

/// Return the path to an example file relative to the workspace root.
fn example(name: &str) -> PathBuf {
    workspace_root().join("examples").join(name)
}

/// Build a `Command` that runs the `pact` binary through Cargo.
fn pact_cmd() -> Command {
    let mut cmd = Command::new("cargo");
    cmd.arg("run")
        .arg("-p")
        .arg("pact-lang")
        .arg("--quiet")
        .arg("--")
        .current_dir(workspace_root());
    cmd
}

/// Create a unique temporary directory for each test invocation.
///
/// Uses PID + a monotonic counter + thread ID to avoid collisions when
/// tests run in parallel within the same process.
fn make_temp_dir(prefix: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tid = format!("{:?}", std::thread::current().id());
    let dir = std::env::temp_dir().join(format!(
        "pact-test-{}-{}-{}-{}",
        prefix,
        std::process::id(),
        unique,
        tid.replace(|c: char| !c.is_alphanumeric(), "")
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("failed to create temp dir");
    dir
}

/// Remove a temp directory (best-effort).
fn cleanup_temp_dir(dir: &Path) {
    let _ = fs::remove_dir_all(dir);
}

// ─── 1. pact check — valid file ─────────────────────────────────────────────

#[test]
fn check_valid_hello_agent() {
    let output = pact_cmd()
        .arg("check")
        .arg(example("hello_agent.pact"))
        .output()
        .expect("failed to execute pact check");

    assert!(
        output.status.success(),
        "pact check should succeed for hello_agent.pact.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

// ─── 2. pact check — invalid file ───────────────────────────────────────────

#[test]
fn check_invalid_file_fails() {
    let tmp = make_temp_dir("invalid");
    let bad_file = tmp.join("broken.pact");
    fs::write(&bad_file, "this is not valid pact @@@ {{{}}}").unwrap();

    let output = pact_cmd()
        .arg("check")
        .arg(&bad_file)
        .output()
        .expect("failed to execute pact check");

    assert!(
        !output.status.success(),
        "pact check should fail for an invalid file"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    // The error output should contain something useful (error message, not empty)
    assert!(
        !stderr.trim().is_empty() || !String::from_utf8_lossy(&output.stdout).trim().is_empty(),
        "error output should not be empty for an invalid file"
    );

    cleanup_temp_dir(&tmp);
}

// ─── 3. pact build — creates artifacts ──────────────────────────────────────

#[test]
fn build_creates_output_artifacts() {
    let tmp = make_temp_dir("build");
    let out_dir = tmp.join("pact-out");

    let output = pact_cmd()
        .arg("build")
        .arg(example("hello_agent.pact"))
        .arg("--out-dir")
        .arg(&out_dir)
        .output()
        .expect("failed to execute pact build");

    assert!(
        output.status.success(),
        "pact build should succeed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // The output directory should exist and contain files
    assert!(
        out_dir.exists(),
        "output directory should be created by pact build"
    );

    let entries: Vec<_> = fs::read_dir(&out_dir)
        .expect("failed to read output dir")
        .collect();
    assert!(
        !entries.is_empty(),
        "pact build should produce at least one artifact in the output directory"
    );

    cleanup_temp_dir(&tmp);
}

// ─── 4. pact run — execute a flow with mock dispatch ────────────────────────

#[test]
fn run_hello_flow_with_mock_dispatch() {
    let output = pact_cmd()
        .arg("run")
        .arg(example("hello_agent.pact"))
        .arg("--flow")
        .arg("hello")
        .arg("--dispatch")
        .arg("mock")
        .arg("--args")
        .arg("world")
        .output()
        .expect("failed to execute pact run");

    assert!(
        output.status.success(),
        "pact run should succeed with mock dispatch.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // The mock dispatcher should produce some output
    assert!(
        !stdout.trim().is_empty(),
        "pact run should produce output when executing a flow"
    );
}

// ─── 5. pact test — run tests in a .pact file ──────────────────────────────

#[test]
fn test_hello_agent_tests_pass() {
    let output = pact_cmd()
        .arg("test")
        .arg(example("hello_agent.pact"))
        .output()
        .expect("failed to execute pact test");

    assert!(
        output.status.success(),
        "pact test should succeed for hello_agent.pact.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should report test results
    assert!(
        stdout.contains("pass") || stdout.contains("PASS") || stdout.contains("ok") || stdout.contains("1"),
        "pact test output should indicate passing tests, got: {}",
        stdout,
    );
}

// ─── 6. pact fmt — format a file ───────────────────────────────────────────

#[test]
fn fmt_produces_valid_output() {
    let output = pact_cmd()
        .arg("fmt")
        .arg(example("hello_agent.pact"))
        .output()
        .expect("failed to execute pact fmt");

    assert!(
        output.status.success(),
        "pact fmt should succeed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Formatted output should still be valid PACT — it should contain key constructs
    assert!(
        stdout.contains("tool") || stdout.contains("agent") || stdout.contains("flow"),
        "pact fmt output should contain PACT constructs, got: {}",
        stdout,
    );
}

// ─── 7. pact doc — generate documentation ──────────────────────────────────

#[test]
fn doc_generates_expected_sections() {
    let output = pact_cmd()
        .arg("doc")
        .arg(example("hello_agent.pact"))
        .output()
        .expect("failed to execute pact doc");

    assert!(
        output.status.success(),
        "pact doc should succeed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Documentation should contain Markdown headers and mention key declarations
    assert!(
        stdout.contains('#'),
        "pact doc output should contain Markdown headers"
    );
    assert!(
        stdout.contains("greet") || stdout.contains("greeter") || stdout.contains("hello"),
        "pact doc output should reference declarations from the file, got: {}",
        stdout,
    );
}

// ─── 8. pact to-mermaid — export to Mermaid ─────────────────────────────────

#[test]
fn to_mermaid_contains_flowchart() {
    let output = pact_cmd()
        .arg("to-mermaid")
        .arg(example("hello_agent.pact"))
        .output()
        .expect("failed to execute pact to-mermaid");

    assert!(
        output.status.success(),
        "pact to-mermaid should succeed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("flowchart") || stdout.contains("graph") || stdout.contains("-->"),
        "pact to-mermaid output should contain Mermaid diagram syntax, got: {}",
        stdout,
    );
}

// ─── 9. pact from-mermaid — import a .mmd file ─────────────────────────────

#[test]
fn from_mermaid_produces_pact_constructs() {
    let output = pact_cmd()
        .arg("from-mermaid")
        .arg(example("incident_response.agentflow.mmd"))
        .output()
        .expect("failed to execute pact from-mermaid");

    assert!(
        output.status.success(),
        "pact from-mermaid should succeed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // The generated PACT should contain typical constructs
    assert!(
        stdout.contains("tool") || stdout.contains("agent") || stdout.contains("flow"),
        "pact from-mermaid output should contain PACT constructs (tool, agent, or flow), got: {}",
        stdout,
    );
}

// ─── 10. pact init — create a new file with minimal template ────────────────

#[test]
fn init_creates_valid_pact_file() {
    let tmp = make_temp_dir("init");
    let new_file = tmp.join("new_project.pact");

    let output = pact_cmd()
        .arg("init")
        .arg(&new_file)
        .output()
        .expect("failed to execute pact init");

    assert!(
        output.status.success(),
        "pact init should succeed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    assert!(
        new_file.exists(),
        "pact init should create the specified file"
    );

    let content = fs::read_to_string(&new_file).expect("failed to read init output");
    assert!(
        content.contains("tool") && content.contains("agent") && content.contains("flow"),
        "init template should contain tool, agent, and flow declarations, got: {}",
        content,
    );

    // Verify the generated file passes pact check
    let check_output = pact_cmd()
        .arg("check")
        .arg(&new_file)
        .output()
        .expect("failed to execute pact check on init output");

    assert!(
        check_output.status.success(),
        "pact check should pass on the file created by pact init.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr),
    );

    cleanup_temp_dir(&tmp);
}

// ─── 11. pact list — list declarations from a file ──────────────────────────

#[test]
fn list_declarations_from_file() {
    let output = pact_cmd()
        .arg("list")
        .arg("declarations")
        .arg("--file")
        .arg(example("hello_agent.pact"))
        .output()
        .expect("failed to execute pact list");

    assert!(
        output.status.success(),
        "pact list should succeed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should list the declarations defined in hello_agent.pact
    assert!(
        stdout.contains("greet") || stdout.contains("greeter") || stdout.contains("hello"),
        "pact list should show declarations from the file, got: {}",
        stdout,
    );
}

// ─── 12. pact check — complex file (website_builder.pact) ───────────────────

#[test]
fn check_complex_website_builder() {
    let output = pact_cmd()
        .arg("check")
        .arg(example("website_builder.pact"))
        .output()
        .expect("failed to execute pact check");

    assert!(
        output.status.success(),
        "pact check should succeed for the complex website_builder.pact.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

// ─── Bonus: pact check — research_flow.pact (schema + type alias + tests) ──

#[test]
fn check_research_flow_with_schema_and_types() {
    let output = pact_cmd()
        .arg("check")
        .arg(example("research_flow.pact"))
        .output()
        .expect("failed to execute pact check");

    assert!(
        output.status.success(),
        "pact check should succeed for research_flow.pact (has schema, type alias, tests).\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

// ─── Bonus: pact check — nonexistent file gives useful error ────────────────

#[test]
fn check_nonexistent_file_fails_with_error() {
    let output = pact_cmd()
        .arg("check")
        .arg("/tmp/does_not_exist_at_all.pact")
        .output()
        .expect("failed to execute pact check");

    assert!(
        !output.status.success(),
        "pact check should fail for a nonexistent file"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("failed to read") || stderr.contains("No such file") || stderr.contains("not found"),
        "error message should indicate the file was not found, got: {}",
        stderr,
    );
}

// ─── Bonus: pact test — research_flow tests pass ────────────────────────────

#[test]
fn test_research_flow_tests_pass() {
    let output = pact_cmd()
        .arg("test")
        .arg(example("research_flow.pact"))
        .output()
        .expect("failed to execute pact test");

    assert!(
        output.status.success(),
        "pact test should succeed for research_flow.pact.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

// ─── Bonus: pact init — refuses to overwrite existing file ──────────────────

#[test]
fn init_refuses_to_overwrite_existing_file() {
    let tmp = make_temp_dir("init-exists");
    let existing = tmp.join("existing.pact");
    fs::write(&existing, "-- already here").unwrap();

    let output = pact_cmd()
        .arg("init")
        .arg(&existing)
        .output()
        .expect("failed to execute pact init");

    assert!(
        !output.status.success(),
        "pact init should fail when the file already exists"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("already exists"),
        "error should mention file already exists, got: {}",
        stderr,
    );

    cleanup_temp_dir(&tmp);
}
