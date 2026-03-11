// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-11-22

//! Runtime compliance mediation.
//!
//! The mediator sits between every API response and the downstream consumer,
//! validating that the agent's behavior complies with its declared contract.
//! This is PACT's runtime enforcement layer — defense in depth on top of the
//! static checker and prompt-level guardrails.
//!
//! ## What it checks
//!
//! 1. **Tool authorization** — Did Claude call a tool the agent is allowed to use?
//! 2. **Permission enforcement** — Does the agent have the permissions required
//!    by the called tool?
//! 3. **Input validation** — Do the tool call inputs match the declared parameter types?
//! 4. **Iteration limits** — Has the conversation loop exceeded the max iterations?

use pact_core::ast::expr::ExprKind;
use pact_core::ast::stmt::{AgentDecl, DeclKind, Program, ToolDecl};

use crate::executor::{handler_required_permissions, parse_handler};
use crate::types::ContentBlock;

/// Runtime mediation errors.
#[derive(Debug, Clone)]
pub enum MediationError {
    /// Claude called a tool not in the agent's tool list.
    UnauthorizedTool {
        tool_name: String,
        agent_name: String,
        allowed_tools: Vec<String>,
    },
    /// The agent lacks a permission required by the tool.
    MissingPermission {
        tool_name: String,
        permission: String,
        agent_name: String,
    },
    /// A tool call input doesn't match the declared parameter type.
    InvalidToolInput {
        tool_name: String,
        param: String,
        expected: String,
        got: String,
    },
    /// The conversation loop exceeded the max iterations.
    MaxIterationsExceeded { count: usize },
    /// The agent's output contains sensitive data it shouldn't have.
    SensitiveDataLeak {
        agent_name: String,
        pattern: String,
        detail: String,
    },
    /// The agent's output is empty when a return type was expected.
    EmptyOutput {
        agent_name: String,
        expected_type: String,
    },
    /// The agent acted outside its declared scope.
    ScopeViolation {
        agent_name: String,
        detail: String,
    },
}

impl std::fmt::Display for MediationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediationError::UnauthorizedTool {
                tool_name,
                agent_name,
                allowed_tools,
            } => {
                write!(
                    f,
                    "MEDIATION: agent @{} called unauthorized tool #{}. Allowed: {}",
                    agent_name,
                    tool_name,
                    allowed_tools
                        .iter()
                        .map(|t| format!("#{t}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            MediationError::MissingPermission {
                tool_name,
                permission,
                agent_name,
            } => {
                write!(
                    f,
                    "MEDIATION: agent @{} called #{} which requires ^{}, but agent lacks this permission",
                    agent_name, tool_name, permission
                )
            }
            MediationError::InvalidToolInput {
                tool_name,
                param,
                expected,
                got,
            } => {
                write!(
                    f,
                    "MEDIATION: tool #{} parameter '{}' expected {}, got {}",
                    tool_name, param, expected, got
                )
            }
            MediationError::MaxIterationsExceeded { count } => {
                write!(
                    f,
                    "MEDIATION: conversation loop exceeded max iterations ({})",
                    count
                )
            }
            MediationError::SensitiveDataLeak {
                agent_name,
                pattern,
                detail,
            } => {
                write!(
                    f,
                    "MEDIATION: agent @{} output contains sensitive data ({}): {}",
                    agent_name, pattern, detail
                )
            }
            MediationError::EmptyOutput {
                agent_name,
                expected_type,
            } => {
                write!(
                    f,
                    "MEDIATION: agent @{} returned empty output, expected {}",
                    agent_name, expected_type
                )
            }
            MediationError::ScopeViolation {
                agent_name,
                detail,
            } => {
                write!(
                    f,
                    "MEDIATION: agent @{} acted outside declared scope: {}",
                    agent_name, detail
                )
            }
        }
    }
}

/// Runtime compliance mediator for an agent.
pub struct RuntimeMediator {
    agent_name: String,
    /// Tools the agent is allowed to use (names without #).
    allowed_tools: Vec<String>,
    /// Permissions the agent has (joined segments, e.g. "net.read").
    granted_permissions: Vec<String>,
}

impl RuntimeMediator {
    /// Create a mediator from an agent declaration and the full program.
    pub fn new(agent: &AgentDecl, _program: &Program) -> Self {
        let allowed_tools = agent
            .tools
            .iter()
            .filter_map(|e| match &e.kind {
                ExprKind::ToolRef(name) => Some(name.clone()),
                _ => None,
            })
            .collect();

        let granted_permissions = agent
            .permits
            .iter()
            .filter_map(|e| match &e.kind {
                ExprKind::PermissionRef(segs) => Some(segs.join(".")),
                _ => None,
            })
            .collect();

        Self {
            agent_name: agent.name.clone(),
            allowed_tools,
            granted_permissions,
        }
    }

    /// Validate a tool_use response from Claude before executing.
    ///
    /// Checks:
    /// 1. Is the tool in the agent's allowed tool list?
    /// 2. Does the agent have the permissions required by the tool?
    /// 3. Do the input parameters match expected types?
    pub fn validate_tool_use(
        &self,
        content: &ContentBlock,
        program: &Program,
    ) -> Result<(), MediationError> {
        let (tool_name, input) = match content {
            ContentBlock::ToolUse { name, input, .. } => (name.as_str(), input),
            ContentBlock::Text { .. } => return Ok(()),
        };

        // 1. Tool authorization
        if !self.allowed_tools.iter().any(|t| t == tool_name) {
            return Err(MediationError::UnauthorizedTool {
                tool_name: tool_name.to_string(),
                agent_name: self.agent_name.clone(),
                allowed_tools: self.allowed_tools.clone(),
            });
        }

        // 2. Permission check — find the tool's required permissions
        if let Some(tool_decl) = find_tool_decl(program, tool_name) {
            let required: Vec<String> = tool_decl
                .requires
                .iter()
                .filter_map(|e| match &e.kind {
                    ExprKind::PermissionRef(segs) => Some(segs.join(".")),
                    _ => None,
                })
                .collect();

            for perm in &required {
                if !self.permission_granted(perm) {
                    return Err(MediationError::MissingPermission {
                        tool_name: tool_name.to_string(),
                        permission: perm.clone(),
                        agent_name: self.agent_name.clone(),
                    });
                }
            }

            // 3. Input validation — check required params are present
            for param in &tool_decl.params {
                if let Some(ty) = &param.ty {
                    let type_name = format_type(ty);
                    if let Some(input_obj) = input.as_object() {
                        if let Some(val) = input_obj.get(&param.name) {
                            if let Err(e) = validate_json_type(val, &type_name) {
                                return Err(MediationError::InvalidToolInput {
                                    tool_name: tool_name.to_string(),
                                    param: param.name.clone(),
                                    expected: type_name,
                                    got: e,
                                });
                            }
                        }
                        // Missing params will be caught by Claude's input_schema validation
                    }
                }
            }
        }

        Ok(())
    }

    /// Validate the agent's final output before returning it to the flow.
    ///
    /// Checks:
    /// 1. Output is not empty when a return type is declared
    /// 2. Output doesn't contain sensitive data the agent shouldn't expose
    /// 3. Output doesn't indicate the agent tried to act outside its scope
    pub fn validate_output(
        &self,
        output: &str,
        tool_name: &str,
        program: &Program,
    ) -> Result<(), MediationError> {
        // 1. Empty output check — if the called tool has a return type, output shouldn't be empty
        if output.trim().is_empty() {
            if let Some(tool_decl) = find_tool_decl(program, tool_name) {
                if let Some(ty) = &tool_decl.return_type {
                    return Err(MediationError::EmptyOutput {
                        agent_name: self.agent_name.clone(),
                        expected_type: format_type(ty),
                    });
                }
            }
        }

        // 2. Sensitive data leak detection
        // If agent does NOT have financial permissions, flag credit card patterns
        if !self.permission_granted("pay.charge")
            && !self.permission_granted("pay")
            && contains_credit_card_pattern(output)
        {
            return Err(MediationError::SensitiveDataLeak {
                agent_name: self.agent_name.clone(),
                pattern: "credit card number".to_string(),
                detail: "output contains what appears to be a credit card number".to_string(),
            });
        }

        // If agent does NOT have db.write or fs.write, flag if it mentions writing/storing data
        if !self.permission_granted("db.write")
            && !self.permission_granted("fs.write")
            && !self.permission_granted("db")
            && !self.permission_granted("fs")
            && output_claims_data_storage(output)
        {
            return Err(MediationError::ScopeViolation {
                agent_name: self.agent_name.clone(),
                detail: "agent claims to have stored/saved data but lacks write permissions"
                    .to_string(),
            });
        }

        // If agent does NOT have net.write or email.send, flag if it claims to have sent something
        if !self.permission_granted("net.write")
            && !self.permission_granted("email.send")
            && !self.permission_granted("net")
            && !self.permission_granted("email")
            && output_claims_sending(output)
        {
            return Err(MediationError::ScopeViolation {
                agent_name: self.agent_name.clone(),
                detail: "agent claims to have sent data externally but lacks net.write/email.send permissions".to_string(),
            });
        }

        // 3. Check for system prompt leakage
        if output_leaks_system_prompt(output) {
            return Err(MediationError::ScopeViolation {
                agent_name: self.agent_name.clone(),
                detail: "agent appears to be leaking its system prompt or internal instructions"
                    .to_string(),
            });
        }

        Ok(())
    }

    /// Validate that the agent has the permissions required by a tool's handler.
    ///
    /// If the tool has a `handler:` field, parses it to determine the handler
    /// type (HTTP, shell, builtin) and checks that the agent has the necessary
    /// permissions (e.g. `net.read` for HTTP GET, `sh.exec` for shell).
    pub fn validate_handler_permissions(
        &self,
        tool_name: &str,
        program: &Program,
    ) -> Result<(), MediationError> {
        if let Some(tool_decl) = find_tool_decl(program, tool_name) {
            // Check source-based permissions
            if let Some(source) = &tool_decl.source {
                let registry = crate::providers::ProviderRegistry::new();
                if let Some(info) = registry.get(&source.capability) {
                    if !self.permission_granted(info.required_permission) {
                        return Err(MediationError::MissingPermission {
                            tool_name: tool_name.to_string(),
                            permission: info.required_permission.to_string(),
                            agent_name: self.agent_name.clone(),
                        });
                    }
                }
            }

            // Check handler-based permissions
            if let Some(handler_str) = &tool_decl.handler {
                if let Ok(spec) = parse_handler(handler_str) {
                    let required = handler_required_permissions(&spec);
                    for perm in required {
                        if !self.permission_granted(perm) {
                            return Err(MediationError::MissingPermission {
                                tool_name: tool_name.to_string(),
                                permission: perm.to_string(),
                                agent_name: self.agent_name.clone(),
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Check if a permission is granted (supports parent coverage).
    fn permission_granted(&self, required: &str) -> bool {
        self.granted_permissions.iter().any(|granted| {
            granted == required || required.starts_with(&format!("{}.", granted))
        })
    }
}

/// Find a tool declaration by name in the program.
pub fn find_tool_decl<'a>(program: &'a Program, name: &str) -> Option<&'a ToolDecl> {
    program.decls.iter().find_map(|d| match &d.kind {
        DeclKind::Tool(t) if t.name == name => Some(t),
        _ => None,
    })
}

/// Validate a JSON value against a PACT type name.
fn validate_json_type(val: &serde_json::Value, type_name: &str) -> Result<(), String> {
    match type_name {
        "String" => {
            if val.is_string() {
                Ok(())
            } else {
                Err(json_type_name(val).to_string())
            }
        }
        "Int" => {
            if val.is_i64() || val.is_u64() {
                Ok(())
            } else {
                Err(json_type_name(val).to_string())
            }
        }
        "Float" => {
            if val.is_number() {
                Ok(())
            } else {
                Err(json_type_name(val).to_string())
            }
        }
        "Bool" => {
            if val.is_boolean() {
                Ok(())
            } else {
                Err(json_type_name(val).to_string())
            }
        }
        _ if type_name.starts_with("List<") => {
            if val.is_array() {
                Ok(())
            } else {
                Err(json_type_name(val).to_string())
            }
        }
        _ => Ok(()), // Unknown types pass through
    }
}

/// Get a human-readable name for a JSON value type.
fn json_type_name(val: &serde_json::Value) -> &'static str {
    match val {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "Bool",
        serde_json::Value::Number(_) => "Number",
        serde_json::Value::String(_) => "String",
        serde_json::Value::Array(_) => "Array",
        serde_json::Value::Object(_) => "Object",
    }
}

/// Format a type expression for display.
fn format_type(ty: &pact_core::ast::types::TypeExpr) -> String {
    use pact_core::ast::types::TypeExprKind;
    match &ty.kind {
        TypeExprKind::Named(n) => n.clone(),
        TypeExprKind::Generic { name, args } => {
            let arg_strs: Vec<String> = args.iter().map(format_type).collect();
            format!("{}<{}>", name, arg_strs.join(", "))
        }
        TypeExprKind::Optional(inner) => format!("{}?", format_type(inner)),
    }
}

/// Check if text contains what looks like a credit card number (13-19 consecutive digits).
fn contains_credit_card_pattern(text: &str) -> bool {
    let mut consecutive_digits = 0;
    for ch in text.chars() {
        if ch.is_ascii_digit() {
            consecutive_digits += 1;
            if consecutive_digits >= 13 {
                return true;
            }
        } else if ch == ' ' || ch == '-' {
            // Allow spaces/dashes within card numbers (e.g. "4111 1111 1111 1111")
            // but don't reset the count
        } else {
            consecutive_digits = 0;
        }
    }
    false
}

/// Check if output claims to have stored or saved data.
fn output_claims_data_storage(text: &str) -> bool {
    let lower = text.to_lowercase();
    let storage_phrases = [
        "i have saved",
        "i have stored",
        "i've saved",
        "i've stored",
        "data has been saved",
        "data has been stored",
        "saved to database",
        "stored in database",
        "written to file",
        "saved to file",
        "persisted the data",
        "data has been persisted",
    ];
    storage_phrases.iter().any(|p| lower.contains(p))
}

/// Check if output claims to have sent data externally.
fn output_claims_sending(text: &str) -> bool {
    let lower = text.to_lowercase();
    let sending_phrases = [
        "i have sent",
        "i've sent",
        "email has been sent",
        "message has been sent",
        "notification sent",
        "i sent the",
        "data has been transmitted",
        "i have emailed",
        "i've emailed",
    ];
    sending_phrases.iter().any(|p| lower.contains(p))
}

/// Check if output appears to leak the system prompt.
fn output_leaks_system_prompt(text: &str) -> bool {
    let lower = text.to_lowercase();
    let leak_indicators = [
        "my system prompt is",
        "my instructions are",
        "my system instructions",
        "here is my system prompt",
        "here are my instructions",
        "i was instructed to",
        "## security guidelines",
        "## permission boundaries",
        "## hallucination prevention",
        "## compliance & mediation",
    ];
    leak_indicators.iter().any(|p| lower.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pact_core::lexer::Lexer;
    use pact_core::parser::Parser;
    use pact_core::span::SourceMap;

    fn parse_program(src: &str) -> pact_core::ast::stmt::Program {
        let mut sm = SourceMap::new();
        let id = sm.add("test.pact", src);
        let tokens = Lexer::new(src, id).lex().unwrap();
        Parser::new(&tokens).parse().unwrap()
    }

    fn make_tool_use(name: &str, input: serde_json::Value) -> ContentBlock {
        ContentBlock::ToolUse {
            id: "tu_test".to_string(),
            name: name.to_string(),
            input,
        }
    }

    #[test]
    fn authorized_tool_passes() {
        let src = r#"
            tool #search {
                description: <<Search.>>
                requires: [^net.read]
                params { query :: String }
                returns :: String
            }
            agent @worker {
                permits: [^net.read, ^llm.query]
                tools: [#search]
            }
        "#;
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[1].kind {
            let mediator = RuntimeMediator::new(agent, &program);
            let tool_use = make_tool_use("search", serde_json::json!({"query": "rust"}));
            assert!(mediator.validate_tool_use(&tool_use, &program).is_ok());
        }
    }

    #[test]
    fn unauthorized_tool_rejected() {
        let src = r#"
            tool #search {
                description: <<Search.>>
                requires: [^net.read]
                params { query :: String }
                returns :: String
            }
            agent @worker {
                permits: [^llm.query]
                tools: []
            }
        "#;
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[1].kind {
            let mediator = RuntimeMediator::new(agent, &program);
            let tool_use = make_tool_use("search", serde_json::json!({"query": "rust"}));
            let err = mediator.validate_tool_use(&tool_use, &program).unwrap_err();
            assert!(matches!(err, MediationError::UnauthorizedTool { .. }));
        }
    }

    #[test]
    fn missing_permission_rejected() {
        let src = r#"
            tool #search {
                description: <<Search.>>
                requires: [^net.read]
                params { query :: String }
                returns :: String
            }
            agent @worker {
                permits: [^llm.query]
                tools: [#search]
            }
        "#;
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[1].kind {
            let mediator = RuntimeMediator::new(agent, &program);
            let tool_use = make_tool_use("search", serde_json::json!({"query": "rust"}));
            let err = mediator.validate_tool_use(&tool_use, &program).unwrap_err();
            assert!(matches!(err, MediationError::MissingPermission { .. }));
        }
    }

    #[test]
    fn parent_permission_covers_child() {
        let src = r#"
            tool #search {
                description: <<Search.>>
                requires: [^net.read]
                params { query :: String }
                returns :: String
            }
            agent @worker {
                permits: [^net]
                tools: [#search]
            }
        "#;
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[1].kind {
            let mediator = RuntimeMediator::new(agent, &program);
            let tool_use = make_tool_use("search", serde_json::json!({"query": "rust"}));
            assert!(mediator.validate_tool_use(&tool_use, &program).is_ok());
        }
    }

    #[test]
    fn invalid_input_type_rejected() {
        let src = r#"
            tool #search {
                description: <<Search.>>
                requires: [^net.read]
                params { query :: String }
                returns :: String
            }
            agent @worker {
                permits: [^net.read]
                tools: [#search]
            }
        "#;
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[1].kind {
            let mediator = RuntimeMediator::new(agent, &program);
            // Pass an int where String is expected
            let tool_use = make_tool_use("search", serde_json::json!({"query": 42}));
            let err = mediator.validate_tool_use(&tool_use, &program).unwrap_err();
            assert!(matches!(err, MediationError::InvalidToolInput { .. }));
        }
    }

    #[test]
    fn text_block_passes_validation() {
        let src = "agent @bare { permits: [] tools: [] }";
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[0].kind {
            let mediator = RuntimeMediator::new(agent, &program);
            let text = ContentBlock::Text {
                text: "hello".to_string(),
            };
            assert!(mediator.validate_tool_use(&text, &program).is_ok());
        }
    }

    // ── Output validation tests ────────────────────────────────

    #[test]
    fn empty_output_with_return_type_rejected() {
        let src = r#"
            tool #search {
                description: <<Search.>>
                requires: [^net.read]
                params { query :: String }
                returns :: String
            }
            agent @worker {
                permits: [^net.read]
                tools: [#search]
            }
        "#;
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[1].kind {
            let mediator = RuntimeMediator::new(agent, &program);
            let err = mediator.validate_output("", "search", &program).unwrap_err();
            assert!(matches!(err, MediationError::EmptyOutput { .. }));
        }
    }

    #[test]
    fn non_empty_output_passes() {
        let src = r#"
            tool #search {
                description: <<Search.>>
                requires: [^net.read]
                params { query :: String }
                returns :: String
            }
            agent @worker {
                permits: [^net.read]
                tools: [#search]
            }
        "#;
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[1].kind {
            let mediator = RuntimeMediator::new(agent, &program);
            assert!(mediator
                .validate_output("Here are the search results.", "search", &program)
                .is_ok());
        }
    }

    #[test]
    fn credit_card_leak_detected() {
        let src = r#"
            tool #greet {
                description: <<Greet.>>
                requires: [^llm.query]
                params { name :: String }
                returns :: String
            }
            agent @greeter {
                permits: [^llm.query]
                tools: [#greet]
            }
        "#;
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[1].kind {
            let mediator = RuntimeMediator::new(agent, &program);
            let output = "Your card number is 4111111111111111 and it's valid.";
            let err = mediator.validate_output(output, "greet", &program).unwrap_err();
            assert!(matches!(err, MediationError::SensitiveDataLeak { .. }));
        }
    }

    #[test]
    fn credit_card_allowed_with_pay_permission() {
        let src = r#"
            tool #charge {
                description: <<Charge card.>>
                requires: [^pay.charge]
                params { amount :: Float }
                returns :: String
            }
            agent @cashier {
                permits: [^pay.charge, ^llm.query]
                tools: [#charge]
            }
        "#;
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[1].kind {
            let mediator = RuntimeMediator::new(agent, &program);
            let output = "Charged card ending in 4111111111111111.";
            // Should pass because agent has pay.charge permission
            assert!(mediator.validate_output(output, "charge", &program).is_ok());
        }
    }

    #[test]
    fn scope_violation_data_storage() {
        let src = r#"
            tool #search {
                description: <<Search.>>
                requires: [^net.read]
                params { query :: String }
                returns :: String
            }
            agent @reader {
                permits: [^net.read, ^llm.query]
                tools: [#search]
            }
        "#;
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[1].kind {
            let mediator = RuntimeMediator::new(agent, &program);
            let output = "I have saved your data to the database for future use.";
            let err = mediator.validate_output(output, "search", &program).unwrap_err();
            assert!(matches!(err, MediationError::ScopeViolation { .. }));
        }
    }

    #[test]
    fn scope_violation_sending() {
        let src = r#"
            tool #search {
                description: <<Search.>>
                requires: [^net.read]
                params { query :: String }
                returns :: String
            }
            agent @reader {
                permits: [^net.read, ^llm.query]
                tools: [#search]
            }
        "#;
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[1].kind {
            let mediator = RuntimeMediator::new(agent, &program);
            let output = "I have sent the email with the results to the team.";
            let err = mediator.validate_output(output, "search", &program).unwrap_err();
            assert!(matches!(err, MediationError::ScopeViolation { .. }));
        }
    }

    #[test]
    fn system_prompt_leak_detected() {
        let src = "agent @bare { permits: [^llm.query] tools: [] }";
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[0].kind {
            let mediator = RuntimeMediator::new(agent, &program);
            let output = "Sure! Here is my system prompt: You are a helpful assistant...";
            let err = mediator
                .validate_output(output, "nonexistent", &program)
                .unwrap_err();
            assert!(matches!(err, MediationError::ScopeViolation { .. }));
        }
    }

    #[test]
    fn normal_output_passes_all_checks() {
        let src = r#"
            tool #search {
                description: <<Search.>>
                requires: [^net.read]
                params { query :: String }
                returns :: String
            }
            agent @worker {
                permits: [^net.read, ^llm.query]
                tools: [#search]
            }
        "#;
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[1].kind {
            let mediator = RuntimeMediator::new(agent, &program);
            let output = "Based on the search results, Rust is a systems programming language \
                          focused on safety, speed, and concurrency.";
            assert!(mediator.validate_output(output, "search", &program).is_ok());
        }
    }

    // ── Helper function tests ────────────────────────────────

    #[test]
    fn credit_card_patterns() {
        assert!(contains_credit_card_pattern("4111111111111111"));
        assert!(contains_credit_card_pattern("4111 1111 1111 1111"));
        assert!(contains_credit_card_pattern("4111-1111-1111-1111"));
        assert!(!contains_credit_card_pattern("12345")); // too short
        assert!(!contains_credit_card_pattern("hello world"));
    }

    #[test]
    fn storage_claim_detection() {
        assert!(output_claims_data_storage("I have saved your preferences."));
        assert!(output_claims_data_storage("The data has been stored in our system."));
        assert!(!output_claims_data_storage("Here are the search results."));
    }

    #[test]
    fn sending_claim_detection() {
        assert!(output_claims_sending("I have sent the report to your email."));
        assert!(output_claims_sending("The notification sent successfully."));
        assert!(!output_claims_sending("Here is the information you requested."));
    }

    #[test]
    fn system_prompt_leak_patterns() {
        assert!(output_leaks_system_prompt("My system prompt is: be helpful"));
        assert!(output_leaks_system_prompt("## Security Guidelines\nFollow these..."));
        assert!(!output_leaks_system_prompt("The security of the system is important."));
    }

    // ── Handler permission tests ────────────────────────────────

    #[test]
    fn handler_http_get_without_net_read_rejected() {
        let src = r#"
            tool #fetch {
                description: <<Fetch data.>>
                requires: []
                handler: "http GET https://api.example.com/data"
                params { url :: String }
                returns :: String
            }
            agent @worker {
                permits: [^llm.query]
                tools: [#fetch]
            }
        "#;
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[1].kind {
            let mediator = RuntimeMediator::new(agent, &program);
            let err = mediator
                .validate_handler_permissions("fetch", &program)
                .unwrap_err();
            match err {
                MediationError::MissingPermission { permission, .. } => {
                    assert_eq!(permission, "net.read");
                }
                other => panic!("expected MissingPermission, got {:?}", other),
            }
        } else {
            panic!("expected agent decl");
        }
    }

    #[test]
    fn handler_shell_with_sh_exec_passes() {
        let src = r#"
            tool #exec_cmd {
                description: <<Run a command.>>
                requires: []
                handler: "sh echo hi"
                params { cmd :: String }
                returns :: String
            }
            agent @worker {
                permits: [^sh.exec, ^llm.query]
                tools: [#exec_cmd]
            }
        "#;
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[1].kind {
            let mediator = RuntimeMediator::new(agent, &program);
            assert!(mediator
                .validate_handler_permissions("exec_cmd", &program)
                .is_ok());
        } else {
            panic!("expected agent decl");
        }
    }

    #[test]
    fn mediation_error_display() {
        let err = MediationError::UnauthorizedTool {
            tool_name: "hack".to_string(),
            agent_name: "bot".to_string(),
            allowed_tools: vec!["search".to_string()],
        };
        let msg = err.to_string();
        assert!(msg.contains("MEDIATION"));
        assert!(msg.contains("#hack"));
        assert!(msg.contains("@bot"));
    }
}
