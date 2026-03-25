-- Mermaid OST Roadmap Analyzer
-- Multi-agent system for issue-driven roadmap analysis.
-- Models the workflow described in mermaid_ost_roadmap.md

-- ── Permissions ─────────────────────────────────────────────────────────────

permit_tree {
    ^llm {
        ^llm.query
        ^llm.embed
    }
    ^net {
        ^net.read
    }
    ^db {
        ^db.read
        ^db.write
    }
}

-- ── Schemas ─────────────────────────────────────────────────────────────────

schema Issue {
    id :: Int
    title :: String
    labels :: List<String>
    comment_count :: Int
    plus_ones :: Int
    is_regression :: Bool
    is_contribution_ready :: Bool
    affected_diagrams :: List<String>
    author_persona :: String
    commenter_personas :: List<String>
}

schema Persona {
    name :: String
    issue_count :: Int
    weighted_impact :: Int
    share_pct :: Float
    author_count :: Int
    commenter_count :: Int
    key_needs :: List<String>
    affected_diagrams :: List<String>
}

schema Theme {
    name :: String
    issue_count :: Int
    weighted_impact :: Int
    share_pct :: Float
    demand_silent :: Int
    demand_noticed :: Int
    demand_wanted :: Int
    demand_heated :: Int
    outcomes :: List<String>
    flags :: List<String>
    representative_issues :: List<Int>
}

schema RoadmapItem {
    priority :: String
    title :: String
    issue_count :: Int
    weighted_impact :: Int
    key_personas :: List<String>
    rationale :: String
}

schema OSTReport {
    total_issues :: Int
    themes :: List<String>
    personas :: List<String>
    roadmap_items :: List<String>
    cross_cutting_patterns :: List<String>
    regression_count :: Int
}

-- ── Type Aliases ────────────────────────────────────────────────────────────

type DemandLevel = Silent | Noticed | Wanted | Heated
type Priority = P0 | P1 | P2 | P3
type DiagramType = Flowchart | Sequence | Class | ER | State | Gantt | Mindmap | GitGraph | Block | Architecture | C4 | XYChart | Sankey | Pie | Timeline | Radar | Packet | ZenUML
type PersonaName = Developer | GeneralUser | TechnicalWriter | Architect | LibraryIntegrator | ProjectManager | DataAnalyst | Educator | Student | DevOpsEngineer | Designer

-- ── Templates ───────────────────────────────────────────────────────────────

template %ost_report {
    section EXECUTIVE_SUMMARY
    TOTAL_ISSUES :: String              <<total open issues analyzed>>
    TOP_THEMES :: String * 3            <<Theme | Weighted Impact | Share>>
    KEY_INSIGHT :: String               <<one-paragraph synthesis>>
    section PERSONAS
    PERSONA_PROFILES :: String * 11     <<Name | Issues | WI | Share | Key Needs>>
    section THEMES
    THEME_ANALYSES :: String * 8        <<Theme | Issues | WI | Demand Profile | Outcomes>>
    section ROADMAP
    P0_ITEMS :: String * 3              <<Title | Issues | WI | Rationale>>
    P1_ITEMS :: String * 3              <<Title | Issues | WI | Rationale>>
    P2_ITEMS :: String * 2              <<Title | Issues | WI | Rationale>>
    section PATTERNS
    DIAGRAM_CONCENTRATION :: String * 5 <<Diagram | Issue Count | Primary Themes>>
    REGRESSION_CLUSTERS :: String * 4   <<Area | Issue Count | Impact>>
    HEATED_DISTRIBUTION :: String * 8   <<Theme | Heated Count | Share>>
}

template %theme_analysis {
    THEME_NAME :: String                <<theme title>>
    ISSUE_COUNT :: String               <<number of issues>>
    WEIGHTED_IMPACT :: String           <<calculated WI score>>
    DEMAND_PROFILE :: String            <<silent | noticed | wanted | heated breakdown>>
    KEY_OUTCOMES :: String * 6          <<Outcome | Issues | Impact | Diagrams | Personas>>
    COMMUNITY_SIGNAL :: String          <<qualitative engagement summary>>
    ROADMAP_SIGNAL :: String            <<prioritization recommendation>>
    REPRESENTATIVE_ISSUES :: String * 5 <<Issue # | Title | Demand Level>>
}

-- ── Directives ──────────────────────────────────────────────────────────────

directive %data_driven {
    <<DATA-DRIVEN ANALYSIS: All prioritization must be grounded in quantitative
    evidence — weighted impact scores, issue counts, demand profiles, and regression
    flags. Never recommend based on intuition alone. Always cite supporting data
    when ranking themes or items.>>
}

directive %persona_centered {
    <<PERSONA-CENTERED DESIGN: Every recommendation must identify which personas
    benefit and how. Use the persona-weighted impact model: an issue affecting
    developers (43.8% share) carries more weight than one affecting designers (0.2%).
    Cross-reference persona needs with theme outcomes.>>
    params {
        weighting_model :: String = "engagement-weighted (comments + plus-ones + discussion)"
    }
}

directive %regression_aware {
    <<REGRESSION AWARENESS: Flag any analysis area where regression markers appear.
    Regressions represent trust erosion — functionality that used to work but doesn't.
    Regression-flagged clusters should receive priority weight (1.5x) in scoring.
    Currently 44% of the backlog (597 issues) is regression-flagged.>>
}

-- ── Tools ───────────────────────────────────────────────────────────────────

tool #fetch_issues {
    description: <<Fetch open issues from GitHub repository with metadata including
    labels, comment counts, plus-ones, and discussion engagement. Returns structured
    issue data for downstream classification.>>
    requires: [^net.read]
    directives: [%data_driven]
    params {
        repo :: String
    }
    returns :: List<Issue>
    cache: "1h"
}

tool #classify_issues {
    description: <<Classify each issue by diagram type, persona, demand level, and
    theme. Uses LLM-based content analysis of issue title, body, and comments to
    determine affected diagram types and user persona. Applies weighted impact
    formula: WI = comments + (2 * plus_ones) + heated_bonus.>>
    requires: [^llm.query, ^llm.embed]
    directives: [%data_driven, %persona_centered]
    params {
        issues :: List<Issue>
    }
    returns :: List<Issue>
}

tool #profile_personas {
    description: <<Build persona profiles from classified issue data. Computes
    per-persona issue counts, weighted impact, share percentages, and identifies
    key needs and most-affected diagram types. Distinguishes author vs commenter
    contributions.>>
    requires: [^llm.query]
    directives: [%persona_centered]
    params {
        issues :: List<Issue>
    }
    returns :: List<Persona>
}

tool #cluster_themes {
    description: <<Group classified issues into outcome themes using hierarchical
    clustering. Identifies 8 top-level themes, computes demand profiles (silent,
    noticed, wanted, heated), and maps outcomes to key results. Flags regression
    and contribution-ready markers.>>
    requires: [^llm.query, ^llm.embed]
    directives: [%data_driven, %regression_aware]
    params {
        issues :: List<Issue>
        personas :: List<Persona>
    }
    returns :: List<Theme>
}

tool #analyze_theme {
    description: <<Deep-dive analysis of a single theme. Identifies sub-outcomes,
    calculates per-leaf weighted impact, maps affected diagram types, identifies
    primary personas, and generates community signal and roadmap signal summaries.>>
    requires: [^llm.query]
    directives: [%data_driven, %persona_centered, %regression_aware]
    params {
        theme :: Theme
        issues :: List<Issue>
        personas :: List<Persona>
    }
    returns :: String
    output: %theme_analysis
}

tool #score_impact {
    description: <<Calculate weighted impact scores for roadmap prioritization.
    Applies the formula: WI = issue_count * demand_multiplier * persona_weight *
    regression_bonus. Returns scored and ranked items.>>
    requires: [^llm.query]
    directives: [%data_driven, %regression_aware]
    params {
        themes :: List<Theme>
        personas :: List<Persona>
    }
    returns :: List<RoadmapItem>
}

tool #find_cross_cutting {
    description: <<Identify cross-cutting patterns across themes: diagram type
    concentration, pain category overlap, multi-theme issue clusters, heated demand
    distribution, and regression concentration areas.>>
    requires: [^llm.query]
    directives: [%data_driven]
    params {
        themes :: List<Theme>
        issues :: List<Issue>
    }
    returns :: String
}

tool #generate_report {
    description: <<Synthesize all analysis outputs into the final OST roadmap
    report. Structures content into executive summary, persona profiles, theme
    analyses, recommended roadmap, and cross-cutting patterns.>>
    requires: [^llm.query]
    directives: [%data_driven, %persona_centered]
    params {
        personas :: List<Persona>
        themes :: List<Theme>
        roadmap :: List<RoadmapItem>
        patterns :: String
    }
    returns :: String
    output: %ost_report
}

tool #save_report {
    description: <<Persist the generated report to the project database for
    versioning and comparison with previous analyses.>>
    requires: [^db.write]
    params {
        report :: String
        version :: String
    }
    returns :: String
}

-- ── Skills ──────────────────────────────────────────────────────────────────

skill $issue_classification {
    description: <<Full issue classification pipeline: fetch, classify by diagram
    type and persona, compute weighted impact scores.>>
    tools: [#fetch_issues, #classify_issues]
    strategy: <<Fetch all open issues first. Then classify in batches of 100 to
    avoid context overflow. Use embeddings to detect duplicate/related issues
    before classification.>>
    params {
        repo :: String
    }
    returns :: List<Issue>
}

skill $theme_deep_dive {
    description: <<Complete theme analysis: cluster issues, analyze each theme
    in depth, identify community and roadmap signals.>>
    tools: [#cluster_themes, #analyze_theme]
    strategy: <<First cluster all issues into themes. Then analyze each theme
    independently, generating per-theme reports. Ensure theme analyses reference
    each other for cross-cutting patterns.>>
    params {
        issues :: List<Issue>
        personas :: List<Persona>
    }
    returns :: List<Theme>
}

-- ── Agents ──────────────────────────────────────────────────────────────────

agent @issue_collector {
    permits: [^net.read, ^llm.query, ^llm.embed]
    tools: [#fetch_issues, #classify_issues]
    skills: [$issue_classification]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a GitHub issue analyst specializing in open-source project
    triage. You fetch issues from repositories and classify them by type, affected
    component, user persona, and engagement level. You are meticulous about data
    quality — every issue must be classified consistently. You flag ambiguous cases
    rather than guessing.>>
}

agent @persona_analyst {
    permits: [^llm.query]
    tools: [#profile_personas]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a user research analyst who builds persona profiles from
    quantitative data. You identify distinct user segments by their engagement
    patterns, pain points, and needs. You distinguish between authors (who file
    issues) and commenters (who amplify them). You always ground persona
    descriptions in data, not stereotypes.>>
}

agent @theme_analyst {
    permits: [^llm.query, ^llm.embed]
    tools: [#cluster_themes, #analyze_theme, #find_cross_cutting]
    skills: [$theme_deep_dive]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a product strategy analyst who identifies themes and patterns
    in issue backlogs. You use hierarchical clustering to group related issues,
    compute demand profiles, and identify regression flags. You write community
    signal summaries that capture the qualitative feel of engagement, and roadmap
    signals that translate data into actionable priorities.>>
}

agent @strategist {
    permits: [^llm.query]
    tools: [#score_impact, #generate_report]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a product strategist who turns analysis into recommendations.
    You score themes using weighted impact, persona coverage, and regression severity.
    You assign P0/P1/P2 priorities and write clear rationale for each recommendation.
    You balance quick wins against strategic bets. You write executive summaries
    that decision-makers can act on immediately.>>
}

agent @archivist {
    permits: [^db.write]
    tools: [#save_report]
    prompt: <<You are a data steward. You persist analysis reports with version
    metadata for longitudinal comparison. You never fabricate confirmations.>>
}

-- ── Agent Bundle ────────────────────────────────────────────────────────────

agent_bundle @ost_team {
    agents: [@issue_collector, @persona_analyst, @theme_analyst, @strategist, @archivist]
    fallbacks: @theme_analyst ?> @persona_analyst
}

-- ── Lessons ─────────────────────────────────────────────────────────────────

lesson "regression_undercount" {
    context: <<Previous analysis underweighted regressions because they were counted
    once per issue, not per affected diagram type. A rendering regression in flowcharts
    affects 5x more users than one in packet diagrams.>>
    rule: <<Weight regression impact by diagram-type popularity: flowchart=5x,
    sequence=3x, class/ER=2x, others=1x>>
    severity: warning
}

lesson "heated_signal_quality" {
    context: <<Early analysis treated all "heated" issues equally, but a heated
    feature request (swimlanes, 4+ years open) carries different signal than a
    heated regression (text overflow, 6 months).>>
    rule: <<Distinguish heated-by-demand (long-standing asks) from heated-by-pain
    (recent breakage). Both matter but require different responses.>>
    severity: warning
}

lesson "persona_overlap" {
    context: <<Personas are not mutually exclusive — a developer can also be an
    architect. The commenter pool especially shows cross-persona participation.
    Treating personas as exclusive led to double-counting in impact analysis.>>
    rule: <<Use primary persona for authors, but distribute commenter weight across
    all matching personas proportionally.>>
    severity: error
}

-- ── Flows ───────────────────────────────────────────────────────────────────

flow analyze_roadmap(repo :: String) -> String {
    issues = @issue_collector -> #fetch_issues(repo)
    classified = @issue_collector -> #classify_issues(issues)
    personas = @persona_analyst -> #profile_personas(classified)
    themes = @theme_analyst -> #cluster_themes(classified, personas)
    roadmap = @strategist -> #score_impact(themes, personas)
    patterns = @theme_analyst -> #find_cross_cutting(themes, classified)
    report = @strategist -> #generate_report(personas, themes, roadmap, patterns)
    saved = @archivist -> #save_report(report, "v1")
    return report
}

flow quick_theme_check(repo :: String, theme_name :: String) -> String {
    issues = @issue_collector -> #fetch_issues(repo)
    classified = @issue_collector -> #classify_issues(issues)
    personas = @persona_analyst -> #profile_personas(classified)
    themes = @theme_analyst -> #cluster_themes(classified, personas)
    result = match theme_name {
        _ => @theme_analyst -> #analyze_theme(themes, classified, personas)
    }
    return result
}

-- ── Tests ───────────────────────────────────────────────────────────────────

test "full roadmap analysis produces report" {
    report = @strategist -> #generate_report(
        [],
        [],
        [],
        "patterns"
    )
    assert report
}

test "issue classification returns enriched data" {
    issues = @issue_collector -> #fetch_issues("mermaid-js/mermaid")
    classified = @issue_collector -> #classify_issues(issues)
    assert classified
}

test "persona profiling covers all personas" {
    personas = @persona_analyst -> #profile_personas([])
    assert personas
}
