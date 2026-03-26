-- Showcase 07: DevOps CI/CD Pipeline Orchestrator
-- AI agents managing build, test, deploy, and rollback workflows.
-- Demonstrates: permit_tree, schemas, type aliases, templates, directives,
-- tools (handler, source, output, directives, retry, cache, validate),
-- agents, agent_bundle, skills, flows (parallel, match, pipeline, fallback, on_error, run),
-- lessons, connect, tests.

permit_tree {
    ^llm {
        ^llm.query
    }
    ^exec {
        ^exec.run
    }
    ^net {
        ^net.read
        ^net.write
    }
    ^fs {
        ^fs.read
        ^fs.write
    }
}

-- ── Schemas ──────────────────────────────────────────────────────

schema BuildResult {
    commit_sha :: String
    branch :: String
    status :: String
    duration_seconds :: Int
    artifacts :: List<String>
    warnings :: List<String>
}

schema TestReport {
    total :: Int
    passed :: Int
    failed :: Int
    skipped :: Int
    coverage :: Float
    failures :: List<String>
}

schema DeploymentRecord {
    environment :: String
    version :: String
    timestamp :: String
    status :: String
    health_check :: String
    rollback_target :: Optional<String>
}

schema SecurityScan {
    vulnerabilities :: List<String>
    severity_counts :: String
    passed :: String
    blocking :: List<String>
}

-- ── Type Aliases ─────────────────────────────────────────────────

type Environment = Dev | Staging | Production | Canary
type BuildStatus = Success | Failed | Unstable | Aborted
type DeployStrategy = RollingUpdate | BlueGreen | Canary | Recreate

-- ── Templates ────────────────────────────────────────────────────

template %deploy_plan {
    section PRE_DEPLOY
    VERSION :: String                   <<version being deployed>>
    ENVIRONMENT :: String               <<target environment>>
    PREREQUISITES :: String * 3         <<Check | Status | Blocking?>>
    section DEPLOYMENT
    STRATEGY :: String                  <<deployment strategy with justification>>
    STEP :: String * 6                  <<Step # | Action | Command | Expected duration>>
    section POST_DEPLOY
    HEALTH_CHECK :: String * 3          <<Service | Endpoint | Expected response>>
    ROLLBACK_PLAN :: String             <<specific rollback procedure if health checks fail>>
    section COMMUNICATION
    NOTIFICATION :: String              <<stakeholder notification template>>
}

template %incident_postmortem {
    section TIMELINE
    EVENT :: String * 6                 <<Timestamp | Event | Actor | Impact>>
    section ANALYSIS
    ROOT_CAUSE :: String                <<root cause analysis>>
    CONTRIBUTING_FACTORS :: String * 3  <<Factor | How it contributed>>
    section ACTIONS
    ACTION_ITEM :: String * 4           <<Priority | Action | Owner | Deadline>>
}

-- ── Directives ───────────────────────────────────────────────────

directive %deployment_safety {
    <<SAFETY RULES: Never deploy directly to production without {min_staging_hours}h in staging.
    All deployments must have a rollback plan. Canary deployments must reach {canary_threshold}
    success rate before full rollout. Database migrations must be backward-compatible —
    deploy code first, migrate second. Never deploy on {no_deploy_days}.
    Feature flags for all user-facing changes. Health checks must pass within {health_timeout}s.>>
    params {
        min_staging_hours :: String = "2"
        canary_threshold :: String = "99.5%"
        no_deploy_days :: String = "Friday after 2pm, weekends, holidays"
        health_timeout :: String = "60"
    }
}

directive %security_gates {
    <<SECURITY: Block deployment if any {blocking_severity} vulnerabilities found.
    Require SAST, DAST, and dependency scan results. Container images must be signed.
    Secrets must never appear in build logs or artifacts. All API endpoints must have
    authentication. Check OWASP Top 10 compliance. License audit: block {blocked_licenses}.>>
    params {
        blocking_severity :: String = "critical or high"
        blocked_licenses :: String = "AGPL, SSPL"
    }
}

-- ── Tools ────────────────────────────────────────────────────────

tool #run_build {
    description: <<Execute the build pipeline for a given branch and commit. Compile code, run linting, generate artifacts, and capture build metrics. Return structured build result with status, duration, artifacts, and any warnings.>>
    requires: [^exec.run, ^fs.read]
    handler: "sh ./scripts/build.sh"
    retry: 2
    params {
        branch :: String
        commit :: String
    }
    returns :: String
}

tool #run_tests {
    description: <<Execute the full test suite: unit tests, integration tests, and end-to-end tests. Calculate code coverage. Identify flaky tests vs genuine failures. Return structured test report with pass/fail counts, coverage percentage, and failure details.>>
    requires: [^exec.run]
    handler: "sh ./scripts/test.sh"
    retry: 1
    params {
        build_artifacts :: String
        test_suite :: String
    }
    returns :: String
}

tool #security_scan {
    description: <<Run comprehensive security scanning: static analysis (SAST), dependency vulnerability check, container image scan, and secrets detection. Classify findings by severity and identify blocking issues that must be resolved before deployment.>>
    requires: [^exec.run, ^llm.query]
    directives: [%security_gates]
    validate: strict
    params {
        artifacts :: String
    }
    returns :: String
}

tool #plan_deployment {
    description: <<Create a detailed deployment plan for the target environment. Select deployment strategy based on risk level, change scope, and environment. Generate step-by-step procedure with commands, health checks, and rollback plan. Assess deployment risk.>>
    requires: [^llm.query]
    output: %deploy_plan
    directives: [%deployment_safety]
    validate: strict
    params {
        version :: String
        environment :: String
        test_results :: String
        security_results :: String
    }
    returns :: String
}

tool #execute_deploy {
    description: <<Execute the deployment to the target environment following the deployment plan. Apply infrastructure changes, update service configurations, and run health checks. Report deployment status at each step.>>
    requires: [^exec.run, ^net.write]
    directives: [%deployment_safety]
    handler: "sh ./scripts/deploy.sh"
    retry: 1
    params {
        plan :: String
        environment :: String
    }
    returns :: String
}

tool #rollback {
    description: <<Execute emergency rollback to the previous known-good version. Revert infrastructure changes, restore previous service configurations, and verify health. Document rollback reason and timeline.>>
    requires: [^exec.run, ^net.write]
    handler: "sh ./scripts/rollback.sh"
    retry: 2
    params {
        environment :: String
        target_version :: String
        reason :: String
    }
    returns :: String
}

tool #notify_team {
    description: <<Send deployment notifications to the team via configured channels.>>
    requires: [^net.write]
    handler: "http POST https://hooks.slack.example.com/deploy"
    retry: 3
    params {
        message :: String
        channel :: String
    }
    returns :: String
}

tool #write_postmortem {
    description: <<Generate a deployment postmortem report analyzing what happened, root cause, and action items. Use blameless postmortem format. Focus on systemic improvements, not individual mistakes.>>
    requires: [^llm.query]
    output: %incident_postmortem
    params {
        timeline :: String
        impact :: String
    }
    returns :: String
}

-- ── Skills ───────────────────────────────────────────────────────

skill $full_ci {
    description: <<Complete CI pipeline: build, test, and security scan in optimized sequence with quality gates between each stage.>>
    tools: [#run_build, #run_tests, #security_scan]
    strategy: <<Build first, then run tests and security scan in parallel — both need build artifacts but are independent of each other>>
    params {
        branch :: String
        commit :: String
    }
    returns :: String
}

-- ── Agents ───────────────────────────────────────────────────────

agent @builder {
    permits: [^exec.run, ^fs.read, ^llm.query]
    tools: [#run_build]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a build engineer. You compile code, resolve dependency issues, and produce clean build artifacts. You optimize build times and cache aggressively. You read build logs like a detective — the error is always in the details. When a build fails, you diagnose the root cause, not just the symptom.>>
}

agent @tester {
    permits: [^exec.run, ^llm.query]
    tools: [#run_tests]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a QA engineer. You run comprehensive test suites and analyze results. You distinguish between genuine failures and flaky tests. You track coverage trends and flag regressions. You believe that untested code is broken code — you just haven't found the bug yet.>>
}

agent @security_engineer {
    permits: [^exec.run, ^llm.query]
    tools: [#security_scan]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a security engineer. You scan code and artifacts for vulnerabilities with zero tolerance for critical findings. You understand that security is not a gate — it's a property of the system. You explain findings clearly so developers can fix them. You never approve a deployment with unresolved blocking vulnerabilities.>>
}

agent @deployer {
    permits: [^exec.run, ^net.write, ^llm.query]
    tools: [#plan_deployment, #execute_deploy, #rollback]
    skills: [$full_ci]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a deployment engineer with SRE experience. You plan deployments with the paranoia of someone who has been paged at 3 AM. Every deployment has a rollback plan. You verify health checks before declaring success. You follow the deployment safety rules religiously — no Friday deploys, no skipping staging, no YOLO pushes to production.>>
    memory: [~deployment_history, ~rollback_log]
}

agent @communicator {
    permits: [^net.write, ^llm.query]
    tools: [#notify_team, #write_postmortem]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a DevOps communications specialist. You keep the team informed about pipeline status, deployments, and incidents. Your messages are clear, concise, and actionable. You write blameless postmortems that drive systemic improvements. You believe that communication failures cause more outages than code bugs.>>
}

agent_bundle @devops_team {
    agents: [@builder, @tester, @security_engineer, @deployer, @communicator]
    fallbacks: @deployer ?> @builder
}

-- ── MCP Connections ──────────────────────────────────────────────

connect {
    github         "sse https://github.internal/mcp"
    slack          "stdio slack-mcp-server"
}

-- ── Lessons ──────────────────────────────────────────────────────

lesson "friday_deploy_incident" {
    context: <<Production deploy on Friday 4pm caused cascade failure — team was unavailable for weekend remediation>>
    rule: <<Never deploy to production after 2pm on Fridays or before holidays — if the deploy window is missed, wait for Monday morning>>
    severity: error
}

lesson "migration_rollback" {
    context: <<Database migration was not backward-compatible — rolling back code left the database in an inconsistent state>>
    rule: <<All database migrations must be backward-compatible: add columns first, deploy code, then remove old columns in a separate release>>
    severity: error
}

lesson "flaky_test_erosion" {
    context: <<Team started ignoring test failures because 30% of failures were flaky — real bug shipped to production>>
    rule: <<Quarantine flaky tests immediately — run them separately and fix within 48h. Never let flaky tests dilute trust in the test suite>>
    severity: warning
}

-- ── Flows ────────────────────────────────────────────────────────

-- Full CI/CD pipeline
flow deploy_pipeline(branch :: String, commit :: String, environment :: String) -> String {
    -- Step 1: Build
    build = @builder -> #run_build(branch, commit)

    -- Step 2: Test and security scan in parallel
    parallel {
        tests = @tester -> #run_tests(build, "full")
        security = @security_engineer -> #security_scan(build)
    }

    -- Step 3: Plan deployment
    plan = @deployer -> #plan_deployment(commit, environment, tests, security)

    -- Step 4: Execute deployment
    deployed = @deployer -> #execute_deploy(plan, environment)

    -- Step 5: Notify team
    notified = @communicator -> #notify_team(deployed, "deploys") on_error <<Notification skipped>>

    return deployed
}

-- Environment-specific deployment with match
flow deploy_to(branch :: String, commit :: String, env :: String) -> String {
    build = @builder -> #run_build(branch, commit)
    tests = @tester -> #run_tests(build, "full")

    result = match env {
        "production" => @deployer -> #plan_deployment(commit, "production", tests, "required")
        "staging" => @deployer -> #execute_deploy(tests, "staging")
        "canary" => @deployer -> #execute_deploy(tests, "canary")
        _ => @deployer -> #execute_deploy(tests, "dev")
    }

    return result
}

-- Emergency rollback flow
flow emergency_rollback(environment :: String, target :: String, reason :: String) -> String {
    rolled_back = @deployer -> #rollback(environment, target, reason)
    notified = @communicator -> #notify_team(rolled_back, "incidents") on_error <<Notification failed>>
    postmortem = @communicator -> #write_postmortem(rolled_back, reason)
    return postmortem
}

-- Quick CI check pipeline
flow ci_check(branch :: String, commit :: String) -> String {
    result = @builder -> #run_build(branch, commit) |> @tester -> #run_tests(result, "smoke")
    return result
}

-- Full deploy with rollback fallback
flow safe_deploy(branch :: String, commit :: String) -> String {
    build = @builder -> #run_build(branch, commit)
    tests = @tester -> #run_tests(build, "full")
    deployed = @deployer -> #execute_deploy(tests, "staging") ?> @deployer -> #rollback("staging", "previous", "deploy failed")
    return deployed
}

-- ── Tests ────────────────────────────────────────────────────────

test "build produces artifacts" {
    result = @builder -> #run_build("main", "abc1234")
    assert result
}

test "test suite reports coverage" {
    report = @tester -> #run_tests("build-artifacts", "unit")
    assert report
}

test "security scan catches vulnerabilities" {
    scan = @security_engineer -> #security_scan("container-image")
    assert scan
}

test "deployment plan includes rollback" {
    plan = @deployer -> #plan_deployment("v1.2.3", "staging", "all passed", "no blockers")
    assert plan
}

test "full pipeline completes" {
    result = run deploy_pipeline("main", "abc1234", "staging")
    assert result
}
