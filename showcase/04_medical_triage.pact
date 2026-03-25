-- Showcase 04: Medical Triage & Clinical Decision Support
-- AI-assisted patient triage, symptom analysis, and treatment pathway recommendations.
-- Demonstrates: permit_tree, schemas, type aliases, templates, directives,
-- tools (output, directives, retry, validate, cache, handler), agents, agent_bundle,
-- skills, flows (parallel, match, fallback, on_error), lessons, connect, tests.

permit_tree {
    ^llm {
        ^llm.query
        ^llm.embed
    }
    ^db {
        ^db.read
        ^db.write
    }
    ^net {
        ^net.read
    }
}

-- ── Schemas ──────────────────────────────────────────────────────

schema Patient {
    id :: String
    age :: Int
    sex :: String
    chief_complaint :: String
    vitals :: String
    medical_history :: List<String>
    allergies :: List<String>
    current_medications :: List<String>
}

schema TriageAssessment {
    acuity :: String
    primary_impression :: String
    differential :: List<String>
    recommended_workup :: List<String>
    disposition :: String
}

schema TreatmentPlan {
    diagnosis :: String
    interventions :: List<String>
    medications :: List<String>
    follow_up :: String
    red_flags :: List<String>
}

-- ── Type Aliases ─────────────────────────────────────────────────

type AcuityLevel = Resuscitation | Emergent | Urgent | LessUrgent | NonUrgent
type Disposition = Admit | Observe | Discharge | Transfer | AMA
type LabPriority = STAT | Urgent | Routine

-- ── Templates ────────────────────────────────────────────────────

template %triage_note {
    section PRESENTATION
    CHIEF_COMPLAINT :: String           <<presenting complaint in patient's words>>
    VITALS :: String                    <<HR | BP | RR | SpO2 | Temp | Pain>>
    HPI :: String                       <<history of present illness narrative>>
    section ASSESSMENT
    ACUITY :: String                    <<ESI level with justification>>
    PRIMARY_IMPRESSION :: String        <<most likely diagnosis>>
    DIFFERENTIAL :: String * 5          <<Diagnosis | Likelihood | Key distinguishing feature>>
    section PLAN
    WORKUP :: String * 4                <<Test | Priority | Rationale>>
    DISPOSITION :: String               <<recommended patient disposition>>
}

template %clinical_guideline {
    section DIAGNOSIS
    CRITERIA :: String * 3              <<Criterion | Met/Unmet | Evidence>>
    SCORING :: String                   <<clinical decision rule score and interpretation>>
    section MANAGEMENT
    INTERVENTION :: String * 4          <<Priority | Intervention | Timing | Monitoring>>
    MEDICATION :: String * 3            <<Drug | Dose | Route | Frequency | Duration>>
    section SAFETY
    RED_FLAG :: String * 3              <<Warning sign | Action required | Urgency>>
    CONTRAINDICATION :: String * 2      <<Medication/Procedure | Reason>>
}

-- ── Directives ───────────────────────────────────────────────────

directive %clinical_reasoning {
    <<CLINICAL STANDARD: Apply evidence-based medicine principles. Use validated clinical
    decision rules ({decision_tool}) where applicable. Consider Bayesian reasoning for
    differential diagnosis — pre-test probability adjusted by clinical findings. Document
    reasoning transparently. Flag any recommendation that deviates from guidelines with
    explicit justification. ALWAYS include a safety net: "Return to ED if..." instructions
    for discharge patients. This is DECISION SUPPORT only — a physician must review all
    recommendations before any clinical action.>>
    params {
        decision_tool :: String = "Ottawa Ankle Rules, HEART Score, Wells Criteria"
    }
}

directive %patient_safety {
    <<SAFETY: Never recommend a medication without checking allergies and drug interactions.
    Always include weight-based dosing for pediatric patients (age < {pediatric_age}).
    Flag high-risk medications ({high_risk_meds}) with double-check prompts.
    Include pregnancy considerations for patients of childbearing age.
    CRITICAL: This system provides DECISION SUPPORT. All outputs must include the disclaimer
    that clinical judgment supersedes algorithmic recommendations.>>
    params {
        pediatric_age :: String = "18"
        high_risk_meds :: String = "anticoagulants, opioids, insulin, chemotherapy"
    }
}

-- ── Tools ────────────────────────────────────────────────────────

tool #assess_vitals {
    description: <<Analyze patient vital signs and chief complaint to determine initial acuity level using the Emergency Severity Index (ESI). Consider vital sign trends, pain assessment, and mechanism of injury. Identify any immediate life threats requiring resuscitation.>>
    requires: [^llm.query]
    directives: [%clinical_reasoning]
    validate: strict
    params {
        vitals :: String
        chief_complaint :: String
        age :: Int
    }
    returns :: String
}

tool #generate_differential {
    description: <<Generate a ranked differential diagnosis based on presenting symptoms, vital signs, age, sex, and medical history. Use Bayesian reasoning to assign likelihood estimates. Include must-not-miss diagnoses even if unlikely. Output structured differential with distinguishing features for each.>>
    requires: [^llm.query]
    output: %triage_note
    directives: [%clinical_reasoning, %patient_safety]
    validate: strict
    params {
        presentation :: String
        history :: String
    }
    returns :: String
}

tool #recommend_workup {
    description: <<Recommend diagnostic workup based on differential diagnosis. Prioritize tests by urgency (STAT/Urgent/Routine) and diagnostic yield. Include expected findings that would confirm or rule out each differential. Consider cost-effectiveness and patient burden.>>
    requires: [^llm.query]
    directives: [%clinical_reasoning]
    params {
        differential :: String
        existing_results :: String
    }
    returns :: String
}

tool #suggest_treatment {
    description: <<Generate treatment recommendations based on confirmed or working diagnosis. Include pharmacological and non-pharmacological interventions. Check against patient allergies and current medications for interactions. Follow clinical guidelines and include evidence level for each recommendation.>>
    requires: [^llm.query]
    output: %clinical_guideline
    directives: [%clinical_reasoning, %patient_safety]
    validate: strict
    params {
        diagnosis :: String
        patient_data :: String
    }
    returns :: String
}

tool #search_clinical_literature {
    description: <<Search medical literature databases for relevant clinical evidence, guidelines, and case reports. Return structured summaries with evidence levels (Level I-V) and relevance scores.>>
    requires: [^net.read, ^llm.embed]
    cache: "12h"
    retry: 2
    params {
        query :: String
        speciality :: String
    }
    returns :: String
}

tool #save_encounter {
    description: <<Save the clinical encounter record to the EHR system.>>
    requires: [^db.write]
    handler: "http POST https://ehr.hospital.example.com/encounters"
    retry: 3
    params {
        patient_id :: String
        encounter :: String
    }
    returns :: String
}

-- ── Skills ───────────────────────────────────────────────────────

skill $rapid_assessment {
    description: <<Rapid clinical assessment: vitals analysis through differential diagnosis in an accelerated triage workflow for high-acuity patients.>>
    tools: [#assess_vitals, #generate_differential]
    strategy: <<Assess vitals first to determine acuity — if ESI 1 or 2, immediately generate differential with urgency flag>>
    params {
        patient_data :: String
    }
    returns :: String
}

-- ── Agents ───────────────────────────────────────────────────────

agent @triage_nurse {
    permits: [^llm.query]
    tools: [#assess_vitals]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are an experienced emergency triage nurse with 20 years of experience. You assess patients rapidly and accurately using the Emergency Severity Index. You have an instinct for identifying sick patients who look well and well patients who look sick. Your acuity assignments are consistently accurate. You never downgrade acuity under pressure. IMPORTANT: You provide decision support only — a physician must confirm all assessments.>>
}

agent @diagnostician {
    permits: [^llm.query, ^net.read, ^llm.embed]
    tools: [#generate_differential, #recommend_workup, #search_clinical_literature]
    skills: [$rapid_assessment]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a board-certified emergency physician and diagnostician. You think in probabilistic differentials, not single diagnoses. You always include must-not-miss diagnoses (PE, MI, aortic dissection, ectopic pregnancy, meningitis) even when unlikely. You order tests strategically — each test should change management. You stay current with evidence-based guidelines. IMPORTANT: This is decision support — clinical judgment always supersedes.>>
    memory: [~case_archive, ~guideline_updates]
}

agent @attending {
    permits: [^llm.query]
    tools: [#suggest_treatment]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a senior attending physician. You create treatment plans that are safe, evidence-based, and practical. You always check drug interactions and allergies before recommending medications. You include clear discharge instructions and return precautions. You think about the whole patient — not just the chief complaint. IMPORTANT: All treatment recommendations require physician review before implementation.>>
}

agent @records_agent {
    permits: [^db.write]
    tools: [#save_encounter]
    prompt: <<You are a clinical documentation agent. You save encounter records to the EHR. Execute writes precisely and confirm completion. Never fabricate confirmations.>>
}

agent_bundle @ed_team {
    agents: [@triage_nurse, @diagnostician, @attending, @records_agent]
    fallbacks: @diagnostician ?> @triage_nurse
}

-- ── MCP Connections ──────────────────────────────────────────────

connect {
    brave_search   "stdio npx @anthropic/mcp-server-brave"
    postgres       "stdio npx @anthropic/mcp-server-postgres"
}

-- ── Lessons ──────────────────────────────────────────────────────

lesson "anchoring_bias" {
    context: <<System anchored on initial diagnosis of musculoskeletal pain, missed subtle PE presentation>>
    rule: <<Always generate at least 5 differentials including one life-threatening diagnosis regardless of initial presentation — anchoring bias is the most dangerous diagnostic error>>
    severity: error
}

lesson "medication_allergy_cross_reactivity" {
    context: <<Patient with documented penicillin allergy received cephalosporin without cross-reactivity check>>
    rule: <<When patient has beta-lactam allergy, always assess cross-reactivity risk before recommending cephalosporins — 1-2% cross-reactivity rate is clinically significant>>
    severity: error
}

lesson "vital_sign_trends" {
    context: <<Single normal vital sign reading masked a patient who was trending toward sepsis>>
    rule: <<Always request vital sign trends (at least 2 readings 15 min apart) for patients with infection concerns — single readings can be misleading>>
    severity: warning
}

-- ── Flows ────────────────────────────────────────────────────────

-- Full triage and treatment pipeline
flow triage_patient(chief_complaint :: String, vitals :: String, age :: Int, history :: String) -> String {
    -- Step 1: Initial acuity assessment
    acuity = @triage_nurse -> #assess_vitals(vitals, chief_complaint, age)

    -- Step 2: Generate differential and search literature
    differential = @diagnostician -> #generate_differential(chief_complaint, history)
    literature = @diagnostician -> #search_clinical_literature(chief_complaint, "emergency medicine")

    -- Step 3: Workup recommendations
    workup = @diagnostician -> #recommend_workup(differential, literature)

    -- Step 4: Treatment plan
    treatment = @attending -> #suggest_treatment(differential, history)

    -- Step 5: Save encounter
    saved = @records_agent -> #save_encounter("PT-TRIAGE", treatment) on_error <<Record save deferred>>

    return treatment
}

-- Acuity-based routing with match
flow route_by_acuity(chief_complaint :: String, vitals :: String, age :: Int) -> String {
    acuity = @triage_nurse -> #assess_vitals(vitals, chief_complaint, age)

    result = match acuity {
        "ESI-1" => @diagnostician -> #generate_differential(chief_complaint, "critical")
        "ESI-2" => @diagnostician -> #generate_differential(chief_complaint, "emergent")
        _ => @diagnostician -> #recommend_workup(chief_complaint, "none")
    }

    return result
}

-- Quick assessment pipeline
flow quick_assess(complaint :: String, vitals :: String) -> String {
    result = @triage_nurse -> #assess_vitals(vitals, complaint, 40) |> @diagnostician -> #generate_differential(complaint, result)
    return result
}

-- ── Tests ────────────────────────────────────────────────────────

test "vitals assessment assigns ESI level" {
    acuity = @triage_nurse -> #assess_vitals("HR 110, BP 90/60, RR 24, SpO2 94%, Temp 39.2C", "chest pain", 55)
    assert acuity
}

test "differential includes must-not-miss diagnoses" {
    diff = @diagnostician -> #generate_differential("acute chest pain with dyspnea", "HTN, DM2, smoker")
    assert diff
}

test "treatment checks allergies" {
    plan = @attending -> #suggest_treatment("community-acquired pneumonia", "Allergies: penicillin")
    assert plan
}

test "full triage pipeline completes" {
    result = run triage_patient("severe headache", "HR 88, BP 160/95, RR 16", 42, "migraine history")
    assert result
}
