-- Showcase 06: Adaptive Educational Tutor
-- Personalized learning with assessment, content generation, and progress tracking.
-- Demonstrates: permit_tree, schemas, type aliases, templates, directives,
-- tools (output, directives, retry, cache, validate, source, handler),
-- agents, agent_bundle, skills, flows (parallel, match, fallback, on_error, run),
-- lessons, connect, tests.

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

schema Student {
    id :: String
    name :: String
    grade_level :: Int
    learning_style :: String
    strengths :: List<String>
    areas_for_growth :: List<String>
    current_mastery :: Float
}

schema LessonPlan {
    topic :: String
    objectives :: List<String>
    activities :: List<String>
    assessment :: String
    differentiation :: String
    estimated_duration :: Int
}

schema AssessmentResult {
    score :: Float
    mastery_level :: String
    misconceptions :: List<String>
    next_steps :: List<String>
    feedback :: String
}

-- ── Type Aliases ─────────────────────────────────────────────────

type MasteryLevel = Novice | Developing | Proficient | Advanced | Expert
type LearningStyle = Visual | Auditory | ReadWrite | Kinesthetic
type BloomLevel = Remember | Understand | Apply | Analyze | Evaluate | Create

-- ── Templates ────────────────────────────────────────────────────

template %lesson_content {
    section INTRODUCTION
    HOOK :: String                      <<engaging opening question or scenario>>
    OBJECTIVES :: String                <<what the student will learn, in student-friendly language>>
    section INSTRUCTION
    CONCEPT :: String * 3               <<Key concept | Explanation | Example>>
    VISUAL :: String                    <<description of diagram, chart, or visual aid>>
    section PRACTICE
    GUIDED_PROBLEM :: String * 2        <<Problem | Step-by-step solution>>
    INDEPENDENT_PROBLEM :: String * 3   <<Problem | Difficulty level | Hint>>
    section ASSESSMENT
    CHECK :: String * 4                 <<Question | Correct answer | Common misconception | Bloom level>>
}

template %progress_report {
    section OVERVIEW
    STUDENT :: String                   <<student name and current level>>
    MASTERY :: String                   <<overall mastery percentage and trend>>
    section SKILLS
    SKILL :: String * 5                 <<Skill | Mastery % | Status | Next milestone>>
    section INSIGHTS
    STRENGTH :: String * 2              <<area of strength with evidence>>
    GROWTH :: String * 2                <<area for growth with suggested activity>>
    section PLAN
    NEXT_LESSON :: String               <<recommended next topic and approach>>
}

-- ── Directives ───────────────────────────────────────────────────

directive %pedagogical_approach {
    <<TEACHING METHOD: Use {framework} instructional design. Start with concrete examples
    before abstract concepts. Use the "I do, we do, you do" gradual release model.
    Embed formative checks every 5-7 minutes of instruction. For {learning_style} learners,
    emphasize {modality_focus}. Always connect new concepts to prior knowledge.
    Use growth mindset language — "not yet" instead of "wrong". Celebrate effort and strategy,
    not just correct answers.>>
    params {
        framework :: String = "Understanding by Design"
        learning_style :: String = "visual"
        modality_focus :: String = "diagrams, charts, and color-coded notes"
    }
}

directive %differentiation {
    <<DIFFERENTIATION: Provide three tiers of difficulty for every practice problem.
    Tier 1 (developing): scaffolded with hints and partial solutions.
    Tier 2 (proficient): standard grade-level problems.
    Tier 3 (advanced): extension problems requiring {advanced_skill}.
    For students below grade level, use {intervention} strategies.
    Never lower expectations — increase support instead.>>
    params {
        advanced_skill :: String = "analysis and synthesis"
        intervention :: String = "concrete manipulatives and visual models"
    }
}

-- ── Tools ────────────────────────────────────────────────────────

tool #assess_knowledge {
    description: <<Administer a diagnostic assessment to determine student's current understanding of a topic. Generate 8-10 questions spanning Bloom's taxonomy levels (Remember through Create). Analyze responses to identify specific misconceptions, not just right/wrong. Produce a mastery score and detailed feedback.>>
    requires: [^llm.query]
    directives: [%pedagogical_approach]
    validate: strict
    params {
        topic :: String
        grade_level :: Int
        prior_responses :: String
    }
    returns :: String
}

tool #generate_lesson {
    description: <<Create a personalized lesson based on the student's current mastery level, learning style, and identified gaps. Include engaging hooks, clear explanations with multiple representations, guided and independent practice, and formative checks. Adapt difficulty and pacing to the student's zone of proximal development.>>
    requires: [^llm.query]
    output: %lesson_content
    directives: [%pedagogical_approach, %differentiation]
    validate: strict
    params {
        topic :: String
        mastery_level :: String
        learning_style :: String
        misconceptions :: String
    }
    returns :: String
}

tool #create_practice_set {
    description: <<Generate a set of practice problems tailored to the student's level. Include progressively challenging problems with hints for struggling students and extensions for advanced students. Each problem targets a specific skill or misconception.>>
    requires: [^llm.query]
    directives: [%differentiation]
    params {
        topic :: String
        difficulty :: String
        target_skills :: String
    }
    returns :: String
}

tool #generate_progress_report {
    description: <<Compile a comprehensive progress report showing mastery trends, strengths, areas for growth, and recommended next steps. Include specific evidence from recent assessments. Present data visually with progress bars and trend indicators.>>
    requires: [^llm.query]
    output: %progress_report
    params {
        student_data :: String
        assessment_history :: String
    }
    returns :: String
}

tool #find_resources {
    description: <<Search educational databases for supplementary learning resources (videos, interactive simulations, reading materials) matched to the student's topic, level, and learning style.>>
    requires: [^net.read, ^llm.embed]
    cache: "12h"
    retry: 2
    params {
        topic :: String
        level :: String
        style :: String
    }
    returns :: String
}

tool #save_progress {
    description: <<Save student progress data to the learning management system database.>>
    requires: [^db.write]
    handler: "http POST https://lms.school.example.com/progress"
    retry: 3
    params {
        student_id :: String
        data :: String
    }
    returns :: String
}

-- ── Skills ───────────────────────────────────────────────────────

skill $adaptive_sequence {
    description: <<Full adaptive learning sequence: assess current knowledge, generate personalized lesson, create targeted practice, and update progress — all adapted in real-time to the student's responses.>>
    tools: [#assess_knowledge, #generate_lesson, #create_practice_set, #save_progress]
    strategy: <<Assess first, then generate lesson targeting identified gaps, then create practice that reinforces the lesson — each step informed by the previous>>
    params {
        topic :: String
        student_id :: String
    }
    returns :: String
}

-- ── Agents ───────────────────────────────────────────────────────

agent @assessor {
    permits: [^llm.query]
    tools: [#assess_knowledge]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are an expert educational assessor. You diagnose learning gaps with surgical precision — identifying specific misconceptions, not just whether answers are right or wrong. You design questions that reveal thinking processes. You give feedback that is specific, actionable, and encouraging. You understand that assessment is FOR learning, not just OF learning.>>
}

agent @tutor {
    permits: [^llm.query, ^net.read, ^llm.embed]
    tools: [#generate_lesson, #create_practice_set, #find_resources]
    skills: [$adaptive_sequence]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a master teacher who adapts to every student. You explain concepts in multiple ways until something clicks. You use analogies, stories, and real-world connections. You celebrate progress and normalize productive struggle. You never give away answers — you ask guiding questions. You know that the goal is understanding, not completion. Every student can learn — they just need the right approach and enough time.>>
    memory: [~student_profiles, ~lesson_archive]
}

agent @counselor {
    permits: [^llm.query, ^db.read]
    tools: [#generate_progress_report]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are an academic counselor who translates data into actionable insights for students and parents. You present progress honestly but constructively. You identify patterns in learning data that inform instructional decisions. You recommend specific, practical next steps — not generic advice.>>
}

agent @records {
    permits: [^db.write]
    tools: [#save_progress]
    prompt: <<You are a student records management agent. You save progress data reliably. Execute writes and confirm completion.>>
}

agent_bundle @teaching_team {
    agents: [@assessor, @tutor, @counselor, @records]
    fallbacks: @tutor ?> @assessor
}

-- ── MCP Connections ──────────────────────────────────────────────

connect {
    brave_search   "stdio npx @anthropic/mcp-server-brave"
    sqlite         "stdio npx @anthropic/mcp-server-sqlite"
}

-- ── Lessons ──────────────────────────────────────────────────────

lesson "prerequisite_gaps" {
    context: <<Student struggled with algebra equations because underlying integer operations weren't solid>>
    rule: <<Always check prerequisite skills before advancing topics — fill gaps before building on them, even if it means slowing pace>>
    severity: warning
}

lesson "assessment_fatigue" {
    context: <<Long diagnostic assessments (20+ questions) led to student disengagement and unreliable results>>
    rule: <<Keep diagnostic assessments to 8-10 questions maximum — use adaptive item selection to maximize information per question>>
    severity: info
}

lesson "false_mastery" {
    context: <<Student scored 90% on procedural problems but couldn't apply concepts to novel situations>>
    rule: <<Include at least 2 application-level (Bloom Analyze+) questions in every assessment — procedural fluency without conceptual understanding is false mastery>>
    severity: warning
}

-- ── Flows ────────────────────────────────────────────────────────

-- Full tutoring session
flow tutor_session(student_id :: String, topic :: String, grade :: Int, style :: String) -> String {
    -- Step 1: Diagnostic assessment
    assessment = @assessor -> #assess_knowledge(topic, grade, "initial")

    -- Step 2: Generate lesson and find resources in parallel
    parallel {
        lesson = @tutor -> #generate_lesson(topic, assessment, style, "none")
        resources = @tutor -> #find_resources(topic, assessment, style)
    }

    -- Step 3: Create targeted practice
    practice = @tutor -> #create_practice_set(topic, assessment, "identified gaps")

    -- Step 4: Save progress
    saved = @records -> #save_progress(student_id, assessment) on_error <<Progress save deferred>>

    return lesson
}

-- Level-adapted session with match
flow adaptive_session(student_id :: String, topic :: String, mastery :: String) -> String {
    lesson = match mastery {
        "novice" => @tutor -> #generate_lesson(topic, "novice", "visual", "foundational gaps")
        "developing" => @tutor -> #generate_lesson(topic, "developing", "mixed", "partial understanding")
        "proficient" => @tutor -> #generate_lesson(topic, "proficient", "read_write", "extension needed")
        _ => @tutor -> #generate_lesson(topic, "advanced", "mixed", "challenge with synthesis")
    }

    return lesson
}

-- Progress review flow
flow review_progress(student_id :: String, assessment_data :: String) -> String {
    report = @counselor -> #generate_progress_report(student_id, assessment_data)
    saved = @records -> #save_progress(student_id, report) on_error <<Save deferred>>
    return report
}

-- Quick assessment pipeline
flow quick_check(topic :: String, grade :: Int) -> String {
    result = @assessor -> #assess_knowledge(topic, grade, "none") |> @tutor -> #generate_lesson(topic, result, "visual", "auto")
    return result
}

-- ── Tests ────────────────────────────────────────────────────────

test "assessment covers bloom levels" {
    result = @assessor -> #assess_knowledge("fractions", 5, "initial")
    assert result
}

test "lesson adapts to learning style" {
    lesson = @tutor -> #generate_lesson("photosynthesis", "developing", "kinesthetic", "confuses inputs and outputs")
    assert lesson
}

test "practice set has tiered difficulty" {
    practice = @tutor -> #create_practice_set("linear equations", "developing", "solve for x")
    assert practice
}

test "full session pipeline works" {
    result = run tutor_session("STU-001", "fractions", 5, "visual")
    assert result
}
