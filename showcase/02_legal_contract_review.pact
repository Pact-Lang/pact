-- Showcase 02: Legal Contract Review Pipeline
-- AI-powered contract analysis with compliance checking and risk assessment.
-- Demonstrates: permit_tree, schemas, type aliases, templates, directives,
-- tools (handler, source, output, retry, validate, cache), agents, agent_bundle,
-- skills, flows (match, fallback, on_error, run), lessons, connect, tests.

permit_tree {
    ^llm {
        ^llm.query
        ^llm.embed
    }
    ^fs {
        ^fs.read
        ^fs.write
    }
    ^db {
        ^db.read
        ^db.write
    }
}

-- ── Schemas ──────────────────────────────────────────────────────

schema Contract {
    id :: String
    title :: String
    parties :: List<String>
    effective_date :: String
    expiry_date :: String
    value :: Float
    jurisdiction :: String
    status :: String
}

schema RiskFinding {
    clause_ref :: String
    risk_level :: String
    description :: String
    recommendation :: String
    precedent :: Optional<String>
}

schema ComplianceResult {
    regulation :: String
    status :: String
    gaps :: List<String>
    remediation :: String
}

-- ── Type Aliases ─────────────────────────────────────────────────

type RiskLevel = Critical | High | Medium | Low | Informational
type ContractType = NDA | MSA | SaaS | Employment | Vendor | License
type Jurisdiction = US_Federal | EU_GDPR | UK | California | Singapore

-- ── Templates ────────────────────────────────────────────────────

template %risk_report {
    section EXECUTIVE_SUMMARY
    OVERALL_RISK :: String              <<one-line risk assessment with color code>>
    KEY_CONCERNS :: String * 3          <<top risk findings requiring immediate attention>>
    section CLAUSE_ANALYSIS
    CLAUSE_REVIEW :: String * 10        <<Clause # | Risk Level | Issue | Recommendation>>
    section COMPLIANCE
    REGULATION_CHECK :: String * 5      <<Regulation | Status | Gaps>>
    section RECOMMENDATIONS
    PRIORITY_ACTION :: String * 5       <<Priority | Action | Deadline | Owner>>
}

template %redline_format {
    section CHANGES
    MODIFICATION :: String * 8          <<Clause | Original | Proposed | Rationale>>
    section ADDITIONS
    NEW_CLAUSE :: String * 3            <<Title | Text | Purpose>>
    section DELETIONS
    REMOVED :: String * 2               <<Clause | Reason for removal>>
}

-- ── Directives ───────────────────────────────────────────────────

directive %legal_precision {
    <<LEGAL STANDARD: All analysis must reference specific clause numbers and page locations.
    Use precise legal terminology. Distinguish between "shall" (mandatory) and "may" (permissive).
    Flag ambiguous language that could lead to disputes. Cite relevant case law or regulatory
    guidance where applicable. Jurisdiction: {jurisdiction}. Governing law framework: {framework}.>>
    params {
        jurisdiction :: String = "US Federal"
        framework :: String = "UCC Article 2"
    }
}

directive %compliance_matrix {
    <<COMPLIANCE: Check against {primary_regulation} requirements. Cross-reference with
    {secondary_regulation} where applicable. For data handling clauses, verify alignment with
    breach notification timelines, data subject rights, and cross-border transfer mechanisms.
    Flag any clause that creates unlimited liability or waives statutory protections.>>
    params {
        primary_regulation :: String = "GDPR"
        secondary_regulation :: String = "CCPA"
    }
}

-- ── Tools ────────────────────────────────────────────────────────

tool #extract_clauses {
    description: <<Parse a contract document and extract all numbered clauses with their full text. Identify clause types (liability, indemnification, termination, IP, confidentiality, data protection, force majeure, dispute resolution). Return structured clause data with references.>>
    requires: [^llm.query, ^fs.read]
    source: ^fs.read_file(path)
    validate: strict
    params {
        path :: String
    }
    returns :: String
}

tool #assess_risk {
    description: <<Perform comprehensive risk analysis on extracted contract clauses. Evaluate each clause for legal risk, financial exposure, operational impact, and compliance gaps. Assign risk levels (Critical/High/Medium/Low/Informational) with specific reasoning. Identify missing standard protections.>>
    requires: [^llm.query]
    output: %risk_report
    directives: [%legal_precision]
    validate: strict
    params {
        clauses :: String
        contract_type :: String
    }
    returns :: String
}

tool #check_compliance {
    description: <<Check contract clauses against regulatory requirements. Verify GDPR data processing terms, CCPA consumer rights provisions, SOX financial controls, and industry-specific regulations. Produce a compliance matrix with pass/fail status for each requirement.>>
    requires: [^llm.query]
    directives: [%legal_precision, %compliance_matrix]
    params {
        clauses :: String
        jurisdiction :: String
    }
    returns :: String
}

tool #generate_redlines {
    description: <<Generate specific contract modifications to address identified risks. Propose precise replacement language for problematic clauses, suggest new protective clauses, and recommend deletions. Each change must include legal rationale and impact assessment.>>
    requires: [^llm.query]
    output: %redline_format
    directives: [%legal_precision]
    params {
        risk_report :: String
        compliance_report :: String
    }
    returns :: String
}

tool #search_precedents {
    description: <<Search legal databases for relevant precedents and case law related to specific contract clauses or risk findings. Return case citations with relevance summaries.>>
    requires: [^llm.query, ^db.read]
    cache: "24h"
    retry: 2
    params {
        query :: String
        jurisdiction :: String
    }
    returns :: String
}

tool #save_review {
    description: <<Save the contract review report to the database for audit trail.>>
    requires: [^db.write]
    handler: "http POST https://api.legal-db.example.com/reviews"
    retry: 3
    params {
        contract_id :: String
        report :: String
    }
    returns :: String
}

-- ── Skills ───────────────────────────────────────────────────────

skill $due_diligence {
    description: <<Full legal due diligence: extract clauses, assess risk, and check compliance in a coordinated review cycle.>>
    tools: [#extract_clauses, #assess_risk, #check_compliance]
    strategy: <<Extract clauses first, then run risk assessment and compliance check — both need the extracted clauses but are independent of each other>>
    params {
        document_path :: String
        jurisdiction :: String
    }
    returns :: String
}

-- ── Agents ───────────────────────────────────────────────────────

agent @analyst {
    permits: [^llm.query, ^fs.read]
    tools: [#extract_clauses]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a contract analyst with 15 years of experience in commercial law. You read contracts with the precision of a forensic accountant. You never miss a buried clause, a hidden obligation, or an ambiguous definition. Extract every clause faithfully — do not summarize or paraphrase the contract language.>>
}

agent @risk_counsel {
    permits: [^llm.query, ^db.read]
    tools: [#assess_risk, #search_precedents]
    skills: [$due_diligence]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a senior risk counsel at a top-tier law firm. You evaluate contracts like a chess player — thinking three moves ahead about what could go wrong. You quantify risk where possible and always provide actionable recommendations. You cite precedents to support your analysis.>>
    memory: [~case_law_index, ~past_reviews]
}

agent @compliance_officer {
    permits: [^llm.query]
    tools: [#check_compliance]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a regulatory compliance officer specializing in data protection and financial regulations. You maintain a mental map of GDPR, CCPA, SOX, HIPAA, and industry-specific requirements. You produce clear pass/fail assessments with specific regulatory references.>>
}

agent @redline_drafter {
    permits: [^llm.query]
    tools: [#generate_redlines]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a contract negotiation specialist. You draft precise, enforceable replacement language that protects your client while remaining commercially reasonable. Every proposed change has a clear rationale. You never propose changes that would make the contract one-sided — fairness builds trust and closes deals faster.>>
}

agent @records {
    permits: [^db.write]
    tools: [#save_review]
    prompt: <<You are a legal records management agent. You persist review reports to the compliance database. Execute operations precisely and confirm completion.>>
}

agent_bundle @legal_team {
    agents: [@analyst, @risk_counsel, @compliance_officer, @redline_drafter, @records]
    fallbacks: @risk_counsel ?> @analyst
}

-- ── MCP Connections ──────────────────────────────────────────────

connect {
    filesystem     "stdio npx @anthropic/mcp-server-filesystem"
    postgres       "stdio npx @anthropic/mcp-server-postgres"
}

-- ── Lessons ──────────────────────────────────────────────────────

lesson "indemnification_caps" {
    context: <<A vendor contract with uncapped indemnification led to a $2M exposure that wasn't caught in review>>
    rule: <<Always flag indemnification clauses without monetary caps or carve-outs as Critical risk>>
    severity: error
}

lesson "jurisdiction_mismatch" {
    context: <<Contract specified Delaware law but operations were in EU, creating GDPR enforcement gaps>>
    rule: <<Cross-reference governing law jurisdiction against actual operational geography and applicable data protection regimes>>
    severity: warning
}

lesson "auto_renewal_traps" {
    context: <<SaaS contract auto-renewed for 3 years because 90-day notice window was missed>>
    rule: <<Flag auto-renewal clauses with notice periods exceeding 30 days as High risk and recommend calendar reminders>>
    severity: warning
}

-- ── Flows ────────────────────────────────────────────────────────

-- Full contract review pipeline
flow review_contract(document_path :: String, contract_type :: String, jurisdiction :: String) -> String {
    -- Step 1: Extract and structure all clauses
    clauses = @analyst -> #extract_clauses(document_path)

    -- Step 2: Risk assessment and compliance check (independent, can be parallel)
    parallel {
        risk = @risk_counsel -> #assess_risk(clauses, contract_type)
        compliance = @compliance_officer -> #check_compliance(clauses, jurisdiction)
    }

    -- Step 3: Generate redline suggestions based on findings
    redlines = @redline_drafter -> #generate_redlines(risk, compliance)

    -- Step 4: Persist the review (with error recovery)
    saved = @records -> #save_review(document_path, redlines) on_error <<Save skipped — database unavailable>>

    return redlines
}

-- Jurisdiction-specific review with match
flow jurisdiction_review(document_path :: String, jurisdiction :: String) -> String {
    clauses = @analyst -> #extract_clauses(document_path)

    result = match jurisdiction {
        "EU" => @compliance_officer -> #check_compliance(clauses, "EU_GDPR")
        "California" => @compliance_officer -> #check_compliance(clauses, "CCPA")
        "Singapore" => @compliance_officer -> #check_compliance(clauses, "PDPA")
        _ => @compliance_officer -> #check_compliance(clauses, "US_Federal")
    }

    return result
}

-- Quick risk scan with fallback
flow quick_risk_scan(document_path :: String) -> String {
    clauses = @analyst -> #extract_clauses(document_path)
    risk = @risk_counsel -> #assess_risk(clauses, "general") ?> @analyst -> #extract_clauses(document_path)
    return risk
}

-- ── Tests ────────────────────────────────────────────────────────

test "clause extraction produces structured output" {
    clauses = @analyst -> #extract_clauses("contracts/sample_nda.pdf")
    assert clauses
}

test "risk assessment assigns severity levels" {
    risk = @risk_counsel -> #assess_risk("Sample clause: unlimited indemnification", "NDA")
    assert risk
}

test "compliance check covers GDPR" {
    compliance = @compliance_officer -> #check_compliance("Data processing clause text", "EU_GDPR")
    assert compliance
}

test "full review pipeline produces redlines" {
    result = run review_contract("contracts/sample.pdf", "SaaS", "EU")
    assert result
}
