-- Created: 2025-10-12
-- Copyright (c) 2025-2026 Gabriel Lars Sabadin
-- Licensed under the MIT License. See LICENSE file in the project root.

-- Code Review Agent
--
-- An AI-powered code reviewer that reads source files, analyzes them
-- for quality, security, and performance issues, and produces a
-- structured review report. Demonstrates source providers, templates,
-- directives, on_error, and caching.

-- ── Permissions ─────────────────────────────────────────────────
permit_tree {
    ^llm {
        ^llm.query
    }
    ^fs {
        ^fs.read
        ^fs.glob
    }
}

-- ── Templates ───────────────────────────────────────────────────

template %review_report {
    FILE_PATH :: String             <<absolute path of the file reviewed>>
    LANGUAGE :: String              <<detected programming language>>
    OVERALL_GRADE :: String         <<A through F letter grade>>
    ISSUE :: String * 5             <<Severity | Category | Line | Description | Fix>>
    SUMMARY :: String               <<one paragraph overall assessment>>
}

-- ── Directives ──────────────────────────────────────────────────

directive %strict_security {
    <<REVIEW STYLE: You are a strict security auditor. Flag every potential
    vulnerability — SQL injection, XSS, path traversal, hardcoded secrets,
    insecure deserialization, missing input validation. Classify severity as
    CRITICAL, HIGH, MEDIUM, or LOW. Never let anything slide.>>
}

directive %friendly_mentor {
    <<REVIEW STYLE: You are a friendly senior engineer mentoring a junior
    developer. Explain *why* each issue matters, suggest concrete fixes with
    code snippets, and praise what was done well. Be encouraging but honest.
    Use severity labels: suggestion, warning, error.>>
}

-- ── Tools ───────────────────────────────────────────────────────

tool #find_sources {
    description: <<Find all source files matching a glob pattern in the project directory. Returns a list of file paths.>>
    requires: [^fs.glob]
    source: ^fs.glob(pattern)
    params {
        pattern :: String
    }
    returns :: List<String>
}

tool #read_file {
    description: <<Read the full contents of a source file from disk. Returns the file content as a string.>>
    requires: [^fs.read]
    source: ^fs.read(path)
    cache: "30m"
    params {
        path :: String
    }
    returns :: String
}

tool #analyze_code {
    description: <<Analyze source code for quality, security vulnerabilities, performance issues, and style violations. Produce a structured review using the review_report template.>>
    requires: [^llm.query]
    output: %review_report
    directives: [%strict_security]
    params {
        file_path :: String
        source_code :: String
    }
    returns :: String
}

-- ── Agents ──────────────────────────────────────────────────────

agent @reviewer {
    permits: [^fs.read, ^fs.glob, ^llm.query]
    tools: [#find_sources, #read_file, #analyze_code]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are an expert code reviewer with deep knowledge of security best practices, performance optimization, and clean code principles. Read the file, then produce a thorough review.>>
}

-- ── Flows ───────────────────────────────────────────────────────

flow review_file(path :: String) -> String {
    -- Read source code; if the file is unreadable, return a graceful error
    source_code = @reviewer -> #read_file(path) on_error "Error: could not read file"
    review = @reviewer -> #analyze_code(path, source_code)
    return review
}

flow review_project(glob_pattern :: String) -> String {
    -- Discover all matching files, then review each one
    files = @reviewer -> #find_sources(glob_pattern)
    report = @reviewer -> #analyze_code("project", files) on_error "No files matched the pattern"
    return report
}

-- ── Tests ───────────────────────────────────────────────────────

test "reviewer can read a file" {
    result = @reviewer -> #read_file("src/main.rs")
    assert result == "read_file_result"
}

test "reviewer can analyze code" {
    result = @reviewer -> #analyze_code("test.py", "print('hello')")
    assert result == "analyze_code_result"
}
