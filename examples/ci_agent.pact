-- Created: 2026-03-02
-- Copyright (c) 2026 Gabriel Lars Sabadin
-- Licensed under the MIT License. See LICENSE file in the project root.

-- CI/CD Pipeline Agent
--
-- Runs tests, checks code style, builds the project, and reports
-- results in a structured format. Demonstrates ^sh.exec permissions,
-- on_error, retry for flaky tests, parallel execution, templates,
-- and agent bundles.

-- ── Permissions ─────────────────────────────────────────────────
permit_tree {
    ^llm {
        ^llm.query
    }
    ^sh {
        ^sh.exec
    }
}

-- ── Templates ───────────────────────────────────────────────────

template %ci_report {
    PROJECT :: String               <<project name and branch>>
    COMMIT :: String                <<short commit hash and message>>
    STEP_RESULT :: String * 4       <<Step Name | Status (pass/fail) | Duration | Details>>
    OVERALL :: String               <<PASS or FAIL with summary>>
}

-- ── Tools ───────────────────────────────────────────────────────

tool #run_tests {
    description: <<Run the project test suite via shell command. Returns stdout and stderr combined. Exit code 0 means all tests passed.>>
    requires: [^sh.exec]
    handler: "sh cargo test --workspace 2>&1"
    retry: 2
    params {
        project_dir :: String
    }
    returns :: String
}

tool #check_lint {
    description: <<Run the linter and code style checker. Returns a list of warnings and errors found. Zero issues means the code is clean.>>
    requires: [^sh.exec]
    handler: "sh cargo clippy --workspace -- -D warnings 2>&1"
    params {
        project_dir :: String
    }
    returns :: String
}

tool #build_project {
    description: <<Build the project in release mode. Returns build output including any compiler warnings or errors.>>
    requires: [^sh.exec]
    handler: "sh cargo build --release 2>&1"
    params {
        project_dir :: String
    }
    returns :: String
}

tool #check_format {
    description: <<Check code formatting against project standards. Returns a diff of formatting changes needed, or empty if already formatted.>>
    requires: [^sh.exec]
    handler: "sh cargo fmt --all -- --check 2>&1"
    params {
        project_dir :: String
    }
    returns :: String
}

tool #generate_report {
    description: <<Generate a structured CI report from individual step results. Summarize pass/fail status, timing, and any issues found.>>
    requires: [^llm.query]
    output: %ci_report
    params {
        test_result :: String
        lint_result :: String
        build_result :: String
        format_result :: String
    }
    returns :: String
}

-- ── Agents ──────────────────────────────────────────────────────

agent @runner {
    permits: [^sh.exec]
    tools: [#run_tests, #check_lint, #build_project, #check_format]
    prompt: <<You are a CI runner. Execute build and test commands precisely. Report exact output — never summarize or omit error messages. If a command fails, capture the full error output.>>
}

agent @reporter {
    permits: [^llm.query]
    tools: [#generate_report]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a CI report generator. Take raw build and test outputs and produce a clear, structured report. Highlight failures prominently. Include actionable information for developers to fix issues.>>
}

agent_bundle @ci_pipeline {
    agents: [@runner, @reporter]
}

-- ── Flows ───────────────────────────────────────────────────────

flow run_pipeline(project_dir :: String) -> String {
    -- Run independent checks in parallel
    checks = parallel {
        @runner -> #run_tests(project_dir) on_error "TESTS FAILED: see output",
        @runner -> #check_lint(project_dir) on_error "LINT FAILED: see output",
        @runner -> #check_format(project_dir) on_error "FORMAT CHECK FAILED: see output"
    }

    -- Build depends on checks passing
    build_out = @runner -> #build_project(project_dir) on_error "BUILD FAILED: see output"

    -- Generate the final CI report
    report = @reporter -> #generate_report(checks, checks, build_out, checks)

    return report
}

flow quick_check(project_dir :: String) -> String {
    -- Fast feedback: just tests and lint in parallel
    results = parallel {
        @runner -> #run_tests(project_dir),
        @runner -> #check_lint(project_dir)
    }
    return results
}

-- ── Tests ───────────────────────────────────────────────────────

test "runner can execute tests" {
    result = @runner -> #run_tests("/tmp/project")
    assert result == "run_tests_result"
}

test "lint check produces output" {
    result = @runner -> #check_lint("/tmp/project")
    assert result == "check_lint_result"
}

test "report generation works" {
    result = @reporter -> #generate_report("pass", "pass", "pass", "pass")
    assert result == "generate_report_result"
}
