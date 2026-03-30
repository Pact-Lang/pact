-- Security Scanner — Built-in PACT Security Testing
-- Copyright (c) 2025-2026 Gabriel Lars Sabadin
-- Licensed under the MIT License. See LICENSE file in the project root.
--
-- Demonstrates PACT's built-in security scanning capabilities.
-- Three-tier permission model: passive recon, active scanning, exploit validation.
-- Automatic guardrails enforce scope limitation, authorized targets, and reporting.

-- ══════════════════════════════════════════════════════════════════
-- Permission Tree
-- ══════════════════════════════════════════════════════════════════

permit_tree {
    ^scan {
        ^scan.passive
        ^scan.active
        ^scan.exploit
    }
    ^llm {
        ^llm.query
    }
    ^net {
        ^net.read
    }
}

-- ══════════════════════════════════════════════════════════════════
-- Schemas
-- ══════════════════════════════════════════════════════════════════

schema Finding {
    vulnerability_type :: String
    severity :: String
    component :: String
    description :: String
    reproduction :: String
    remediation :: String
}

schema ScanReport {
    target :: String
    scan_type :: String
    findings :: List<Finding>
    summary :: String
}

type Severity = Critical | High | Medium | Low | Informational

-- ══════════════════════════════════════════════════════════════════
-- Templates
-- ══════════════════════════════════════════════════════════════════

template %scan_report {
    EXECUTIVE_SUMMARY :: String     <<High-level overview of scan results>>
    FINDINGS :: String * 10         <<Type | Severity | Component | Description | Remediation>>
    RISK_ASSESSMENT :: String       <<Overall risk posture and priority recommendations>>
}

-- ══════════════════════════════════════════════════════════════════
-- Tools — Passive Reconnaissance
-- ══════════════════════════════════════════════════════════════════

tool #check_headers {
    description: <<Analyze HTTP security headers of a target URL. Checks for HSTS, CSP, X-Frame-Options, and other security headers.>>
    requires: [^scan.passive]
    source: ^scan.headers(url)
    params { target_url :: String }
    returns :: String
}

tool #check_ssl {
    description: <<Analyze SSL/TLS certificate and configuration. Checks certificate validity, HSTS, and HTTP-to-HTTPS redirect.>>
    requires: [^scan.passive]
    source: ^scan.ssl(domain)
    params { target_url :: String }
    returns :: String
}

tool #enumerate_dns {
    description: <<Enumerate DNS records for a domain. Discovers A, AAAA, MX, TXT, CNAME, and NS records.>>
    requires: [^scan.passive]
    source: ^scan.dns(domain)
    params { target_url :: String }
    returns :: String
}

tool #fingerprint_tech {
    description: <<Detect web technologies, frameworks, and server software from HTTP responses and HTML patterns.>>
    requires: [^scan.passive]
    source: ^scan.technologies(url)
    params { target_url :: String }
    returns :: String
}

-- ══════════════════════════════════════════════════════════════════
-- Tools — Active Scanning
-- ══════════════════════════════════════════════════════════════════

tool #scan_ports {
    description: <<Scan common TCP ports on a target host to identify open services. Tests 19 common ports including HTTP, SSH, databases.>>
    requires: [^scan.active]
    source: ^scan.ports(host)
    params { target_url :: String }
    returns :: String
}

tool #probe_http {
    description: <<Probe HTTP endpoints for misconfigurations, exposed sensitive files (.env, .git), admin panels, and server info disclosure.>>
    requires: [^scan.active]
    source: ^scan.http(url)
    params { target_url :: String }
    returns :: String
}

-- ══════════════════════════════════════════════════════════════════
-- Tools — Analysis (LLM-powered)
-- ══════════════════════════════════════════════════════════════════

tool #analyze_findings {
    description: <<Analyze raw scan results and classify vulnerabilities by severity. Produce structured findings with reproduction steps and remediation guidance.>>
    requires: [^llm.query]
    output: %scan_report
    params { scan_results :: String, target_url :: String }
    returns :: ScanReport
}

tool #generate_remediation {
    description: <<Generate detailed remediation plan from scan findings. Prioritize by business impact and effort required.>>
    requires: [^llm.query]
    params { findings :: String }
    returns :: String
}

-- ══════════════════════════════════════════════════════════════════
-- Agents
-- ══════════════════════════════════════════════════════════════════

agent @recon {
    permits: [^scan.passive, ^llm.query]
    tools: [#check_headers, #check_ssl, #enumerate_dns, #fingerprint_tech]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a passive reconnaissance agent. Gather information about the target without sending intrusive traffic. Focus on publicly available data: DNS records, HTTP headers, SSL certificates, and technology fingerprints. Never attempt active exploitation.>>
}

agent @scanner {
    permits: [^scan.passive, ^scan.active, ^llm.query]
    tools: [#scan_ports, #probe_http, #check_headers]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are an active security scanner. Probe the target for open ports, misconfigurations, and exposed sensitive files. Document all findings with evidence. Stay within the authorized scope — only test the target provided.>>
}

agent @analyst {
    permits: [^llm.query]
    tools: [#analyze_findings, #generate_remediation]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a security analyst. Review scan results and produce a structured report with severity classifications (Critical/High/Medium/Low/Informational), reproduction steps, and actionable remediation guidance. Prioritize findings by business impact.>>
}

-- ══════════════════════════════════════════════════════════════════
-- Compliance
-- ══════════════════════════════════════════════════════════════════

compliance "security_engagement" {
    risk: high
    frameworks: [soc2]
    audit: full
    retention: "90d"
    review_interval: "30d"
    roles {
        approver: "security_lead"
        executor: "scanner_agent"
        auditor: "security_team"
    }
}

-- ══════════════════════════════════════════════════════════════════
-- Flows
-- ══════════════════════════════════════════════════════════════════

flow passive_recon(target :: String) -> String {
    -- Gather intelligence without active probing
    headers = @recon -> #check_headers(target)
    ssl = @recon -> #check_ssl(target)
    dns = @recon -> #enumerate_dns(target)
    tech = @recon -> #fingerprint_tech(target)
    report = @analyst -> #analyze_findings(headers, target)
    return report
}

flow full_scan(target :: String) -> String {
    -- Complete scan: passive recon + active probing + analysis
    headers = @recon -> #check_headers(target)
    ssl = @recon -> #check_ssl(target)
    dns = @recon -> #enumerate_dns(target)
    tech = @recon -> #fingerprint_tech(target)
    ports = @scanner -> #scan_ports(target)
    http = @scanner -> #probe_http(target)
    report = @analyst -> #analyze_findings(headers, target)
    remediation = @analyst -> #generate_remediation(report)
    return remediation
}

flow quick_headers(target :: String) -> String {
    -- Quick check: just HTTP security headers
    result = @recon -> #check_headers(target)
    return result
}

-- ══════════════════════════════════════════════════════════════════
-- Tests
-- ══════════════════════════════════════════════════════════════════

test "passive recon produces report" {
    result = passive_recon("https://example.com")
    assert result
}

test "full scan produces remediation" {
    result = full_scan("https://example.com")
    assert result
}

test "quick headers check" {
    result = quick_headers("https://example.com")
    assert result
}
