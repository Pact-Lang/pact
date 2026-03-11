// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-10-08

//! Built-in skill and prompt libraries for PACT.
//!
//! PACT ships with pre-built skills and prompt templates so users don't
//! have to be experts in security, compliance, or prompt engineering.
//! Users can reference these directly in their `.pact` files or use them
//! as starting points.
//!
//! ## Built-in Skills
//!
//! Skills are reusable agent capabilities with strategy prompts. Each
//! built-in skill includes a description, required tools, and a detailed
//! strategy that gets merged into the agent's system prompt.
//!
//! ## Built-in Prompts
//!
//! Prompt templates provide proven system prompt patterns for common
//! agent roles. Users can use them directly or customize them.

/// A built-in skill definition that ships with PACT.
#[derive(Debug, Clone)]
pub struct BuiltinSkill {
    /// Skill name (without `$` prefix).
    pub name: &'static str,
    /// Short description of what this skill does.
    pub description: &'static str,
    /// Detailed strategy instructions merged into the agent prompt.
    pub strategy: &'static str,
    /// Tool names this skill requires (without `#` prefix).
    pub tools: &'static [&'static str],
    /// Parameter definitions as (name, type) pairs.
    pub params: &'static [(&'static str, &'static str)],
    /// Return type name.
    pub returns: Option<&'static str>,
    /// PACT source code for this skill (copy-paste ready).
    pub pact_source: &'static str,
}

/// A built-in prompt template that ships with PACT.
#[derive(Debug, Clone)]
pub struct BuiltinPrompt {
    /// Template name.
    pub name: &'static str,
    /// Short description of the agent role.
    pub description: &'static str,
    /// The prompt text.
    pub prompt: &'static str,
    /// Suggested permissions for this role.
    pub suggested_permissions: &'static [&'static str],
    /// PACT source code for an agent using this prompt (copy-paste ready).
    pub pact_source: &'static str,
}

// ── Built-in Skills ───────────────────────────────────────────────

/// All built-in skills shipped with PACT.
pub const BUILTIN_SKILLS: &[BuiltinSkill] = &[
    BuiltinSkill {
        name: "age_verification",
        description: "Verify user age through self-declaration or document analysis.",
        strategy: "\
Ask for the user's date of birth or age. Follow these rules strictly:
1. If the user is under the minimum age, deny access immediately and do not collect any additional data.
2. Never store the actual date of birth or age — only store a boolean pass/fail result.
3. If the user provides a document for verification, extract only the age-relevant information and discard the rest.
4. Do not attempt to guess or infer age from other data points.
5. If the user refuses to provide age information, deny access gracefully.
6. Age verification must happen before any other data collection or content access.",
        tools: &["verify_age", "check_document"],
        params: &[("minimum_age", "Int")],
        returns: Some("Bool"),
        pact_source: "\
skill $age_verification {
    description: <<Verify user age through self-declaration or document analysis.>>
    tools: [#verify_age]
    strategy: <<Ask for the user's date of birth or age. If under minimum age, deny access immediately. Never store actual date of birth — only store a boolean pass/fail result.>>
    params { minimum_age :: Int }
    returns :: Bool
}",
    },
    BuiltinSkill {
        name: "data_collection",
        description: "Collect personal data with GDPR/CCPA compliance built in.",
        strategy: "\
When collecting personal data from users:
1. Before collecting, clearly state what data you need and why.
2. Only collect the minimum data necessary for the task — do not ask for extra fields.
3. If the user asks what data you have, list it transparently.
4. If the user asks you to delete their data, confirm and comply immediately.
5. Never share collected data with other agents or external services unless the flow explicitly requires it.
6. Never log or store data beyond what is needed for the current interaction.
7. Validate all input before processing — reject obviously invalid data.
8. If collecting sensitive data (SSN, financial), mask it in all responses and confirmations.",
        tools: &["collect_form", "validate_input"],
        params: &[("required_fields", "List<String>")],
        returns: Some("Record"),
        pact_source: "\
skill $data_collection {
    description: <<Collect personal data with GDPR/CCPA compliance built in.>>
    tools: [#collect_form, #validate_input]
    strategy: <<Before collecting, state what data you need and why. Only collect minimum necessary data. If user asks to delete data, comply immediately. Never share data across agent boundaries.>>
    params { required_fields :: List<String> }
    returns :: Record
}",
    },
    BuiltinSkill {
        name: "content_moderation",
        description: "Review and moderate user-generated content for safety.",
        strategy: "\
When reviewing content:
1. Check for harmful content: hate speech, harassment, threats, self-harm, illegal activity.
2. Check for spam, scams, and misleading information.
3. Check for personal information that shouldn't be public (phone numbers, addresses, SSNs).
4. Return a clear verdict: APPROVE, REJECT, or FLAG_FOR_REVIEW.
5. When rejecting, provide a specific reason without quoting the harmful content back.
6. When flagging, explain what needs human review and why.
7. Do not modify the content — only assess and categorize it.
8. Maintain consistency — similar content should get similar verdicts.",
        tools: &["analyze_content", "check_policy"],
        params: &[("content", "String"), ("policy_level", "String")],
        returns: Some("String"),
        pact_source: "\
skill $content_moderation {
    description: <<Review and moderate user-generated content for safety.>>
    tools: [#analyze_content, #check_policy]
    strategy: <<Check for harmful content, spam, and exposed personal information. Return APPROVE, REJECT, or FLAG_FOR_REVIEW. When rejecting, provide a reason without quoting harmful content back.>>
    params {
        content :: String
        policy_level :: String
    }
    returns :: String
}",
    },
    BuiltinSkill {
        name: "error_handling",
        description: "Handle errors gracefully with user-friendly messages.",
        strategy: "\
When an error occurs:
1. Never expose internal error details, stack traces, or system information to the user.
2. Provide a clear, friendly error message that explains what went wrong in plain language.
3. Suggest what the user can do next (retry, try different input, contact support).
4. Log the technical error details internally for debugging.
5. If the error is transient (network timeout, rate limit), offer to retry automatically.
6. If the error is permanent (invalid input, missing permissions), explain clearly.
7. Never blame the user — use neutral language like 'we encountered an issue'.
8. If multiple errors occur, prioritize the most actionable one.",
        tools: &["log_error", "format_message"],
        params: &[("error_context", "String")],
        returns: Some("String"),
        pact_source: "\
skill $error_handling {
    description: <<Handle errors gracefully with user-friendly messages.>>
    tools: [#log_error, #format_message]
    strategy: <<Never expose internal error details. Provide clear, friendly messages. Suggest next steps. Never blame the user.>>
    params { error_context :: String }
    returns :: String
}",
    },
    BuiltinSkill {
        name: "rate_limiting",
        description: "Enforce rate limits and fair usage policies.",
        strategy: "\
When enforcing rate limits:
1. Track usage per user/session, not globally.
2. When a limit is reached, explain clearly what the limit is and when it resets.
3. Suggest alternatives if the user needs to do more (upgrade plan, batch requests, wait).
4. Never silently drop or ignore requests — always acknowledge them.
5. Return remaining quota information so the user can plan accordingly.
6. For burst usage, allow brief overages with a warning before hard-blocking.
7. Log rate limit events for monitoring but do not log user content.",
        tools: &["check_quota", "update_usage"],
        params: &[("max_requests", "Int"), ("window_seconds", "Int")],
        returns: Some("Bool"),
        pact_source: "\
skill $rate_limiting {
    description: <<Enforce rate limits and fair usage policies.>>
    tools: [#check_quota, #update_usage]
    strategy: <<Track usage per user. When limit reached, explain clearly and suggest alternatives. Never silently drop requests. Return remaining quota.>>
    params {
        max_requests :: Int
        window_seconds :: Int
    }
    returns :: Bool
}",
    },
    BuiltinSkill {
        name: "fact_checking",
        description: "Cross-verify agent outputs against tool results to prevent hallucination.",
        strategy: "\
When verifying facts:
1. For every claim in the input, identify which tool result supports it.
2. If a claim has no supporting tool result, flag it as UNVERIFIED.
3. If a claim contradicts a tool result, flag it as INCORRECT and provide the correct data.
4. If tool results are ambiguous, flag the claim as UNCERTAIN and explain the ambiguity.
5. Never approve a claim just because it sounds plausible — require evidence.
6. Return a structured verification report: each claim with its status (VERIFIED, UNVERIFIED, INCORRECT, UNCERTAIN) and the supporting evidence or correction.
7. If the overall output has more than 20% unverified claims, recommend rejection.",
        tools: &["verify_claim", "cross_reference"],
        params: &[("content", "String"), ("sources", "List<String>")],
        returns: Some("Record"),
        pact_source: "\
skill $fact_checking {
    description: <<Cross-verify agent outputs against tool results to prevent hallucination.>>
    tools: [#verify_claim, #cross_reference]
    strategy: <<For every claim, identify which tool result supports it. Flag unsupported claims as UNVERIFIED. Flag contradictions as INCORRECT. Require evidence for every factual statement.>>
    params {
        content :: String
        sources :: List<String>
    }
    returns :: Record
}",
    },
    BuiltinSkill {
        name: "context_management",
        description: "Manage conversation context, memory, and state across flow steps.",
        strategy: "\
When managing context:
1. Extract key information from each flow step and pass it forward in a structured format.
2. Drop irrelevant context — only carry forward what downstream agents actually need.
3. When updating memory (~refs), merge new data with existing state rather than replacing it.
4. Track what information came from which source for audit purposes.
5. If context grows too large, summarize older entries while preserving critical facts.
6. Never ask the user to re-provide information that exists in the pipeline or memory.
7. When context is ambiguous or contradictory, flag it for resolution rather than guessing.
8. Produce a clean context object at each step: current state, relevant history, and pending actions.",
        tools: &["read_memory", "write_memory", "summarize_context"],
        params: &[("current_context", "Record"), ("max_tokens", "Int")],
        returns: Some("Record"),
        pact_source: "\
skill $context_management {
    description: <<Manage conversation context, memory, and state across flow steps.>>
    tools: [#read_memory, #write_memory, #summarize_context]
    strategy: <<Extract key info from each step. Drop irrelevant context. Merge memory updates. Track information sources. Never ask users to repeat themselves.>>
    params {
        current_context :: Record
        max_tokens :: Int
    }
    returns :: Record
}",
    },
    BuiltinSkill {
        name: "search_and_summarize",
        description: "Search for information and produce structured summaries.",
        strategy: "\
When searching and summarizing:
1. Break complex queries into specific search terms.
2. Cross-reference multiple sources when possible.
3. Cite sources for all claims — never present unsourced information as fact.
4. Structure summaries with: key findings, supporting details, and caveats.
5. Clearly distinguish between facts and opinions/analysis.
6. If search results are insufficient, say so rather than filling gaps with assumptions.
7. Flag any contradictions between sources.
8. Keep summaries concise — prioritize relevance over completeness.",
        tools: &["web_search", "summarize"],
        params: &[("query", "String")],
        returns: Some("String"),
        pact_source: "\
skill $search_and_summarize {
    description: <<Search for information and produce structured summaries.>>
    tools: [#web_search, #summarize]
    strategy: <<Break complex queries into specific terms. Cross-reference sources. Cite all claims. Distinguish facts from opinions. Flag contradictions.>>
    params { query :: String }
    returns :: String
}",
    },
];

// ── Built-in Prompts ──────────────────────────────────────────────

/// All built-in prompt templates shipped with PACT.
pub const BUILTIN_PROMPTS: &[BuiltinPrompt] = &[
    BuiltinPrompt {
        name: "researcher",
        description: "A thorough research assistant that searches and synthesizes information.",
        prompt: "\
You are a thorough research assistant. Your job is to find accurate, \
well-sourced information and present it clearly.

Always cite your sources. If you cannot find reliable information, say so \
rather than speculating. Cross-reference multiple sources when possible. \
Present findings with key points first, then supporting details.",
        suggested_permissions: &["net.read", "llm.query"],
        pact_source: "\
agent @researcher {
    permits: [^net.read, ^llm.query]
    tools: [#web_search, #summarize]
    model: \"claude-sonnet-4-20250514\"
    prompt: <<You are a thorough research assistant. Always cite your sources. If you cannot find reliable information, say so rather than speculating.>>
}",
    },
    BuiltinPrompt {
        name: "writer",
        description: "A professional technical writer for reports and documentation.",
        prompt: "\
You are a professional technical writer. You create clear, well-structured \
documents with proper formatting and logical flow.

Write in plain language. Use headings and bullet points for readability. \
Start with the most important information. Avoid jargon unless the audience \
expects it. Always include an introduction and conclusion.",
        suggested_permissions: &["llm.query"],
        pact_source: "\
agent @writer {
    permits: [^llm.query]
    tools: [#draft_report]
    model: \"claude-sonnet-4-20250514\"
    prompt: <<You are a professional technical writer. Create clear, well-structured documents with proper formatting and logical flow.>>
}",
    },
    BuiltinPrompt {
        name: "code_reviewer",
        description: "A security-focused code reviewer.",
        prompt: "\
You are a security-focused code reviewer. You check for vulnerabilities, \
best practices, and code quality.

Focus on: injection attacks (XSS, SQL injection, command injection), \
authentication/authorization flaws, data exposure, input validation, \
error handling, and accessibility. Be thorough but concise — prioritize \
critical issues over style nits.",
        suggested_permissions: &["llm.query", "fs.read"],
        pact_source: "\
agent @code_reviewer {
    permits: [^llm.query, ^fs.read]
    tools: [#review_code, #check_security]
    model: \"claude-sonnet-4-20250514\"
    prompt: <<You are a security-focused code reviewer. Check for vulnerabilities, best practices, and code quality. Prioritize critical issues over style nits.>>
}",
    },
    BuiltinPrompt {
        name: "customer_support",
        description: "A friendly, patient customer support agent.",
        prompt: "\
You are a friendly and patient customer support agent. Your goal is to \
help users resolve their issues quickly and leave them satisfied.

Listen carefully to the user's problem before suggesting solutions. \
Acknowledge their frustration when appropriate. Offer step-by-step \
instructions. If you cannot solve the problem, escalate clearly — \
tell the user what will happen next and when.",
        suggested_permissions: &["llm.query", "db.read"],
        pact_source: "\
agent @support {
    permits: [^llm.query, ^db.read]
    tools: [#lookup_account, #search_knowledge_base]
    model: \"claude-sonnet-4-20250514\"
    prompt: <<You are a friendly customer support agent. Listen carefully, acknowledge frustration, offer step-by-step solutions. Escalate clearly when needed.>>
}",
    },
    BuiltinPrompt {
        name: "data_analyst",
        description: "A precise data analyst that works with structured data.",
        prompt: "\
You are a precise data analyst. You work with structured data to extract \
insights and produce clear visualizations and reports.

Always verify data quality before analysis. State your methodology. \
Present findings with appropriate caveats about sample size, bias, \
or data limitations. Use precise numbers — avoid vague terms like \
'many' or 'most' when you have exact counts.",
        suggested_permissions: &["llm.query", "db.read", "fs.read"],
        pact_source: "\
agent @analyst {
    permits: [^llm.query, ^db.read, ^fs.read]
    tools: [#query_data, #generate_chart, #summarize]
    model: \"claude-sonnet-4-20250514\"
    prompt: <<You are a precise data analyst. Verify data quality before analysis. State methodology. Present findings with caveats about limitations.>>
}",
    },
    BuiltinPrompt {
        name: "moderator",
        description: "A fair, consistent content moderator.",
        prompt: "\
You are a content moderator. You review user-generated content against \
community guidelines and safety policies.

Apply rules consistently — similar content should receive similar \
verdicts. When removing content, provide a clear, specific reason. \
Never quote harmful content back in your explanations. Err on the \
side of caution for content involving minors. Escalate edge cases \
to human review rather than making uncertain calls.",
        suggested_permissions: &["llm.query"],
        pact_source: "\
agent @moderator {
    permits: [^llm.query]
    tools: [#analyze_content, #check_policy]
    skills: [$content_moderation]
    model: \"claude-sonnet-4-20250514\"
    prompt: <<You are a content moderator. Apply rules consistently. Provide clear reasons for removals. Escalate edge cases to human review.>>
}",
    },
    BuiltinPrompt {
        name: "onboarding",
        description: "A welcoming agent that guides new users through setup.",
        prompt: "\
You are a welcoming onboarding assistant. You guide new users through \
initial setup and help them get started quickly.

Be warm but efficient. Ask one question at a time. Explain why you \
need each piece of information. Offer sensible defaults when possible. \
Celebrate small wins ('Great, your account is set up!'). If the user \
seems confused, simplify your language and offer to explain further.",
        suggested_permissions: &["llm.query", "fs.write"],
        pact_source: "\
agent @onboarding {
    permits: [^llm.query, ^fs.write]
    tools: [#collect_form, #create_account, #send_welcome]
    skills: [$data_collection]
    model: \"claude-sonnet-4-20250514\"
    prompt: <<You are a welcoming onboarding assistant. Ask one question at a time. Explain why you need each piece of information. Offer sensible defaults.>>
}",
    },
];

/// Look up a built-in skill by name.
pub fn find_builtin_skill(name: &str) -> Option<&'static BuiltinSkill> {
    BUILTIN_SKILLS.iter().find(|s| s.name == name)
}

/// Look up a built-in prompt by name.
pub fn find_builtin_prompt(name: &str) -> Option<&'static BuiltinPrompt> {
    BUILTIN_PROMPTS.iter().find(|p| p.name == name)
}

/// List all available built-in skill names.
pub fn list_builtin_skills() -> Vec<&'static str> {
    BUILTIN_SKILLS.iter().map(|s| s.name).collect()
}

/// List all available built-in prompt names.
pub fn list_builtin_prompts() -> Vec<&'static str> {
    BUILTIN_PROMPTS.iter().map(|p| p.name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_skills_have_required_fields() {
        for skill in BUILTIN_SKILLS {
            assert!(!skill.name.is_empty(), "skill name empty");
            assert!(
                !skill.description.is_empty(),
                "skill {} missing description",
                skill.name
            );
            assert!(
                !skill.strategy.is_empty(),
                "skill {} missing strategy",
                skill.name
            );
            assert!(
                !skill.pact_source.is_empty(),
                "skill {} missing pact_source",
                skill.name
            );
            assert!(
                skill.pact_source.contains(&format!("${}", skill.name)),
                "skill {} pact_source doesn't reference itself",
                skill.name
            );
        }
    }

    #[test]
    fn all_prompts_have_required_fields() {
        for prompt in BUILTIN_PROMPTS {
            assert!(!prompt.name.is_empty(), "prompt name empty");
            assert!(
                !prompt.description.is_empty(),
                "prompt {} missing description",
                prompt.name
            );
            assert!(
                !prompt.prompt.is_empty(),
                "prompt {} missing prompt",
                prompt.name
            );
            assert!(
                !prompt.suggested_permissions.is_empty(),
                "prompt {} has no suggested permissions",
                prompt.name
            );
            assert!(
                !prompt.pact_source.is_empty(),
                "prompt {} missing pact_source",
                prompt.name
            );
        }
    }

    #[test]
    fn find_builtin_skill_works() {
        assert!(find_builtin_skill("age_verification").is_some());
        assert!(find_builtin_skill("content_moderation").is_some());
        assert!(find_builtin_skill("nonexistent").is_none());
    }

    #[test]
    fn find_builtin_prompt_works() {
        assert!(find_builtin_prompt("researcher").is_some());
        assert!(find_builtin_prompt("customer_support").is_some());
        assert!(find_builtin_prompt("nonexistent").is_none());
    }

    #[test]
    fn list_functions_return_all() {
        assert_eq!(list_builtin_skills().len(), BUILTIN_SKILLS.len());
        assert_eq!(list_builtin_prompts().len(), BUILTIN_PROMPTS.len());
    }
}
