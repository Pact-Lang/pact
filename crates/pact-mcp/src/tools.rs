// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-01-15

use serde_json::{json, Value};

use pact_core::ast::stmt::DeclKind;
use pact_core::checker::Checker;
use pact_core::interpreter::value::Value as PactValue;
use pact_core::interpreter::Interpreter;
use pact_core::lexer::Lexer;
use pact_core::parser::Parser;
use pact_core::span::SourceMap;

pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

pub fn tool_definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "pact_check",
            description: "Validate a .pact source string for syntax and semantic errors. Returns 'OK' or a list of errors with locations.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "The .pact source code to validate" },
                    "filename": { "type": "string", "description": "Optional filename for error reporting", "default": "input.pact" }
                },
                "required": ["source"]
            }),
        },
        ToolDef {
            name: "pact_list",
            description: "List all declarations (agents, tools, flows, schemas, etc.) in a .pact source string.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "The .pact source code to inspect" }
                },
                "required": ["source"]
            }),
        },
        ToolDef {
            name: "pact_run",
            description: "Execute a flow from a .pact source string using mock dispatch. Returns the flow result.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "The .pact source code" },
                    "flow": { "type": "string", "description": "Name of the flow to execute" },
                    "args": { "type": "array", "items": { "type": "string" }, "description": "Arguments to pass to the flow", "default": [] }
                },
                "required": ["source", "flow"]
            }),
        },
        ToolDef {
            name: "pact_scaffold",
            description: "Generate a .pact file from a high-level description. Provide agent names, their tools, and permissions.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agents": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" },
                                "tools": { "type": "array", "items": { "type": "string" } },
                                "permissions": { "type": "array", "items": { "type": "string" } },
                                "description": { "type": "string" }
                            },
                            "required": ["name", "tools", "permissions"]
                        },
                        "description": "List of agents to generate"
                    }
                },
                "required": ["agents"]
            }),
        },
        ToolDef {
            name: "pact_validate_permissions",
            description: "Check if agents in a .pact source have safe, minimal permission boundaries. Returns warnings for overly broad permissions.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "The .pact source code to analyze" }
                },
                "required": ["source"]
            }),
        },
        ToolDef {
            name: "pact_to_mermaid",
            description: "Convert a .pact source string to a Mermaid agentflow diagram. Returns the agentflow text.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "The .pact source code to convert" }
                },
                "required": ["source"]
            }),
        },
        ToolDef {
            name: "pact_from_mermaid",
            description: "Convert a Mermaid agentflow diagram to .pact source code. Returns valid PACT source.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "The Mermaid agentflow diagram text to convert" }
                },
                "required": ["source"]
            }),
        },
    ]
}

pub fn handle_tool_call(name: &str, args: &Value) -> Result<String, String> {
    match name {
        "pact_check" => handle_check(args),
        "pact_list" => handle_list(args),
        "pact_run" => handle_run(args),
        "pact_scaffold" => handle_scaffold(args),
        "pact_validate_permissions" => handle_validate_permissions(args),
        "pact_to_mermaid" => handle_to_mermaid(args),
        "pact_from_mermaid" => handle_from_mermaid(args),
        _ => Err(format!("unknown tool: {name}")),
    }
}

fn handle_check(args: &Value) -> Result<String, String> {
    let source = args
        .get("source")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required argument: source".to_string())?;
    let filename = args
        .get("filename")
        .and_then(|v| v.as_str())
        .unwrap_or("input.pact");

    let mut sm = SourceMap::new();
    let id = sm.add(filename, source);
    let tokens = Lexer::new(source, id)
        .lex()
        .map_err(|e| format!("Lex error: {e}"))?;

    let (program, parse_errors) = Parser::new(&tokens).parse_collecting_errors();

    let check_errors = Checker::new().check(&program);

    if parse_errors.is_empty() && check_errors.is_empty() {
        return Ok("OK — no errors".to_string());
    }

    let mut messages = Vec::new();
    for e in &parse_errors {
        messages.push(format!("Parse error: {e}"));
    }
    for e in &check_errors {
        messages.push(format!("Check error: {e}"));
    }

    Ok(messages.join("\n"))
}

fn handle_list(args: &Value) -> Result<String, String> {
    let source = args
        .get("source")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required argument: source".to_string())?;

    let mut sm = SourceMap::new();
    let id = sm.add("input.pact", source);
    let tokens = Lexer::new(source, id)
        .lex()
        .map_err(|e| format!("Lex error: {e}"))?;
    let program = Parser::new(&tokens)
        .parse()
        .map_err(|e| format!("Parse error: {e}"))?;

    let mut lines = Vec::new();
    for decl in &program.decls {
        match &decl.kind {
            DeclKind::Agent(a) => {
                let tools: Vec<String> = a
                    .tools
                    .iter()
                    .filter_map(|e| match &e.kind {
                        pact_core::ast::expr::ExprKind::ToolRef(name) => Some(format!("#{name}")),
                        _ => None,
                    })
                    .collect();
                let permits: Vec<String> = a
                    .permits
                    .iter()
                    .filter_map(|e| match &e.kind {
                        pact_core::ast::expr::ExprKind::PermissionRef(segs) => {
                            Some(format!("^{}", segs.join(".")))
                        }
                        _ => None,
                    })
                    .collect();
                lines.push(format!(
                    "agent @{} — permits: [{}], tools: [{}]",
                    a.name,
                    permits.join(", "),
                    tools.join(", ")
                ));
            }
            DeclKind::AgentBundle(ab) => {
                let agents: Vec<String> = ab
                    .agents
                    .iter()
                    .filter_map(|e| match &e.kind {
                        pact_core::ast::expr::ExprKind::AgentRef(name) => Some(format!("@{name}")),
                        _ => None,
                    })
                    .collect();
                lines.push(format!(
                    "agent_bundle @{} — agents: [{}]",
                    ab.name,
                    agents.join(", ")
                ));
            }
            DeclKind::Flow(f) => {
                let params: Vec<String> = f.params.iter().map(|p| p.name.clone()).collect();
                let ret = f
                    .return_type
                    .as_ref()
                    .map(|_| " -> ...".to_string())
                    .unwrap_or_default();
                lines.push(format!("flow {}({}){}", f.name, params.join(", "), ret));
            }
            DeclKind::Schema(s) => {
                let fields: Vec<String> = s.fields.iter().map(|f| f.name.clone()).collect();
                lines.push(format!(
                    "schema {} — fields: [{}]",
                    s.name,
                    fields.join(", ")
                ));
            }
            DeclKind::TypeAlias(t) => {
                lines.push(format!("type {} = {}", t.name, t.variants.join(" | ")));
            }
            DeclKind::PermitTree(pt) => {
                let top_level: Vec<String> = pt
                    .nodes
                    .iter()
                    .map(|n| format!("^{}", n.path.join(".")))
                    .collect();
                lines.push(format!("permit_tree — [{}]", top_level.join(", ")));
            }
            DeclKind::Tool(t) => {
                let requires: Vec<String> = t
                    .requires
                    .iter()
                    .filter_map(|e| match &e.kind {
                        pact_core::ast::expr::ExprKind::PermissionRef(segs) => {
                            Some(format!("^{}", segs.join(".")))
                        }
                        _ => None,
                    })
                    .collect();
                lines.push(format!(
                    "tool #{} — requires: [{}]",
                    t.name,
                    requires.join(", ")
                ));
            }
            DeclKind::Skill(s) => {
                lines.push(format!("skill ${}", s.name));
            }
            DeclKind::Test(t) => {
                lines.push(format!("test \"{}\"", t.description));
            }
            DeclKind::Template(t) => {
                let entry_count = t.entries.len();
                lines.push(format!("template %{} — {} entries", t.name, entry_count));
            }
            DeclKind::Directive(d) => {
                let param_count = d.params.len();
                lines.push(format!("directive %{} — {} params", d.name, param_count));
            }
            DeclKind::Import(i) => {
                lines.push(format!("import \"{}\"", i.path));
            }
            DeclKind::Connect(c) => {
                let names: Vec<_> = c.servers.iter().map(|s| s.name.as_str()).collect();
                lines.push(format!("connect — MCP servers: {}", names.join(", ")));
            }
            DeclKind::Lesson(l) => {
                let sev = l.severity.as_deref().unwrap_or("info");
                lines.push(format!("lesson \"{}\" ({})", l.name, sev));
            }
            DeclKind::Compliance(c) => {
                let risk = c.risk.as_deref().unwrap_or("unspecified");
                lines.push(format!("compliance \"{}\" (risk: {})", c.name, risk));
            }
            DeclKind::Federation(_) => {}
        }
    }

    if lines.is_empty() {
        Ok("No declarations found.".to_string())
    } else {
        Ok(lines.join("\n"))
    }
}

fn handle_run(args: &Value) -> Result<String, String> {
    let source = args
        .get("source")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required argument: source".to_string())?;
    let flow_name = args
        .get("flow")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required argument: flow".to_string())?;
    let flow_args: Vec<PactValue> = args
        .get("args")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|v| PactValue::String(v.as_str().unwrap_or("").to_string()))
                .collect()
        })
        .unwrap_or_default();

    let mut sm = SourceMap::new();
    let id = sm.add("input.pact", source);
    let tokens = Lexer::new(source, id)
        .lex()
        .map_err(|e| format!("Lex error: {e}"))?;
    let program = Parser::new(&tokens)
        .parse()
        .map_err(|e| format!("Parse error: {e}"))?;

    let mut interp = Interpreter::new();
    let result = interp
        .run(&program, flow_name, flow_args)
        .map_err(|e| format!("Runtime error: {e}"))?;

    Ok(result.to_string())
}

fn handle_scaffold(args: &Value) -> Result<String, String> {
    let agents = args
        .get("agents")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "missing required argument: agents".to_string())?;

    // Collect all unique permissions and tools across all agents
    let mut all_permissions = std::collections::BTreeSet::new();
    let mut all_tools = std::collections::BTreeSet::new();

    struct AgentSpec {
        name: String,
        tools: Vec<String>,
        permissions: Vec<String>,
        description: Option<String>,
    }

    let mut agent_specs = Vec::new();
    for agent in agents {
        let name = agent
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "each agent must have a 'name'".to_string())?
            .to_string();
        let tools: Vec<String> = agent
            .get("tools")
            .and_then(|v| v.as_array())
            .ok_or_else(|| format!("agent '{name}' must have 'tools'"))?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        let permissions: Vec<String> = agent
            .get("permissions")
            .and_then(|v| v.as_array())
            .ok_or_else(|| format!("agent '{name}' must have 'permissions'"))?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        let description = agent
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from);

        for perm in &permissions {
            all_permissions.insert(perm.clone());
        }
        for tool in &tools {
            all_tools.insert(tool.clone());
        }

        agent_specs.push(AgentSpec {
            name,
            tools,
            permissions,
            description,
        });
    }

    let mut output = String::new();

    // Generate permit_tree
    if !all_permissions.is_empty() {
        // Group permissions by top-level category
        let mut tree: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for perm in &all_permissions {
            let parts: Vec<&str> = perm.split('.').collect();
            let root = parts[0].to_string();
            tree.entry(root).or_default().push(perm.clone());
        }

        output.push_str("permit_tree {\n");
        for (root, children) in &tree {
            output.push_str(&format!("    ^{root} {{\n"));
            for child in children {
                output.push_str(&format!("        ^{child}\n"));
            }
            output.push_str("    }\n");
        }
        output.push_str("}\n\n");
    }

    // Generate tool declarations
    for tool_name in &all_tools {
        // Find which permissions this tool needs by looking at which agents use it
        let mut tool_perms = std::collections::BTreeSet::new();
        for spec in &agent_specs {
            if spec.tools.contains(tool_name) {
                for perm in &spec.permissions {
                    tool_perms.insert(perm.clone());
                }
            }
        }
        let requires: Vec<String> = tool_perms.iter().map(|p| format!("^{p}")).collect();
        output.push_str(&format!("tool #{tool_name} {{\n"));
        output.push_str(&format!(
            "    description: <<Tool description for {tool_name}>>\n"
        ));
        output.push_str(&format!("    requires: [{}]\n", requires.join(", ")));
        output.push_str("    params {}\n");
        output.push_str("}\n\n");
    }

    // Generate agent declarations
    for spec in &agent_specs {
        let permits: Vec<String> = spec.permissions.iter().map(|p| format!("^{p}")).collect();
        let tools: Vec<String> = spec.tools.iter().map(|t| format!("#{t}")).collect();
        output.push_str(&format!("agent @{} {{\n", spec.name));
        output.push_str(&format!("    permits: [{}]\n", permits.join(", ")));
        output.push_str(&format!("    tools: [{}]\n", tools.join(", ")));
        if let Some(desc) = &spec.description {
            output.push_str(&format!("    prompt: <<{desc}>>\n"));
        }
        output.push_str("}\n\n");
    }

    // Generate a simple flow dispatching to the first agent's first tool
    if let Some(first) = agent_specs.first() {
        if let Some(first_tool) = first.tools.first() {
            output.push_str(&format!(
                "flow main() {{\n    result = @{} -> #{}()\n    return result\n}}\n",
                first.name, first_tool
            ));
        }
    }

    Ok(output)
}

fn handle_validate_permissions(args: &Value) -> Result<String, String> {
    let source = args
        .get("source")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required argument: source".to_string())?;

    let mut sm = SourceMap::new();
    let id = sm.add("input.pact", source);
    let tokens = Lexer::new(source, id)
        .lex()
        .map_err(|e| format!("Lex error: {e}"))?;
    let program = Parser::new(&tokens)
        .parse()
        .map_err(|e| format!("Parse error: {e}"))?;

    // Run the standard checker first
    let check_errors = Checker::new().check(&program);

    let mut warnings = Vec::new();

    // Report standard check errors
    for e in &check_errors {
        warnings.push(format!("Error: {e}"));
    }

    // Scan for overly broad permissions
    // A permission is considered "broad" if it has only one segment (e.g., ^net, ^fs, ^llm)
    // because it grants all sub-permissions
    let broad_roots = ["net", "fs", "llm", "db", "exec", "env", "sys"];

    for decl in &program.decls {
        if let DeclKind::Agent(a) = &decl.kind {
            for perm_expr in &a.permits {
                if let pact_core::ast::expr::ExprKind::PermissionRef(segs) = &perm_expr.kind {
                    if segs.len() == 1 {
                        let root = &segs[0];
                        if broad_roots.contains(&root.as_str()) {
                            warnings.push(format!(
                                "Warning: agent @{} has broad permission '^{}' — consider using more specific sub-permissions (e.g., ^{}.read, ^{}.write)",
                                a.name, root, root, root
                            ));
                        }
                    }
                }
            }
        }
    }

    if warnings.is_empty() {
        Ok("OK — all permissions look appropriately scoped.".to_string())
    } else {
        Ok(warnings.join("\n"))
    }
}

fn handle_to_mermaid(args: &Value) -> Result<String, String> {
    let source = args
        .get("source")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required argument: source".to_string())?;

    let mut sm = SourceMap::new();
    let id = sm.add("input.pact", source);
    let tokens = Lexer::new(source, id)
        .lex()
        .map_err(|e| format!("Lex error: {e}"))?;
    let program = Parser::new(&tokens)
        .parse()
        .map_err(|e| format!("Parse error: {e}"))?;

    Ok(pact_mermaid::agentflow_emit::pact_to_agentflow(&program))
}

fn handle_from_mermaid(args: &Value) -> Result<String, String> {
    let source = args
        .get("source")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing required argument: source".to_string())?;

    pact_mermaid::diagram_to_pact(source).map_err(|e| format!("Conversion error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_valid_source() {
        let args = json!({
            "source": "agent @g { permits: [^llm.query] tools: [#greet] }"
        });
        let result = handle_tool_call("pact_check", &args).unwrap();
        assert!(result.contains("OK"), "expected OK, got: {result}");
    }

    #[test]
    fn check_invalid_source() {
        let args = json!({
            "source": "agent { }"
        });
        let result = handle_tool_call("pact_check", &args).unwrap();
        assert!(
            result.contains("error") || result.contains("Error"),
            "expected error description, got: {result}"
        );
    }

    #[test]
    fn list_declarations() {
        let args = json!({
            "source": r#"
                agent @greeter {
                    permits: [^llm.query]
                    tools: [#greet]
                }
                flow hello(name :: String) -> String {
                    result = @greeter -> #greet(name)
                    return result
                }
                schema Report { title :: String, body :: String }
            "#
        });
        let result = handle_tool_call("pact_list", &args).unwrap();
        assert!(
            result.contains("agent @greeter"),
            "missing agent, got: {result}"
        );
        assert!(result.contains("flow hello"), "missing flow, got: {result}");
        assert!(
            result.contains("schema Report"),
            "missing schema, got: {result}"
        );
    }

    #[test]
    fn run_flow() {
        let args = json!({
            "source": r#"
                agent @g { permits: [^llm.query] tools: [#greet] }
                flow hello(name :: String) -> String {
                    result = @g -> #greet(name)
                    return result
                }
            "#,
            "flow": "hello",
            "args": ["world"]
        });
        let result = handle_tool_call("pact_run", &args).unwrap();
        // The mock dispatcher returns "greet_result"
        assert!(
            result.contains("greet_result"),
            "expected greet_result, got: {result}"
        );
    }

    #[test]
    fn scaffold_generates_valid_pact() {
        let args = json!({
            "agents": [
                {
                    "name": "researcher",
                    "tools": ["web_search"],
                    "permissions": ["net.read"],
                    "description": "Searches the web for information"
                },
                {
                    "name": "writer",
                    "tools": ["draft_report"],
                    "permissions": ["llm.query"],
                    "description": "Writes reports"
                }
            ]
        });
        let result = handle_tool_call("pact_scaffold", &args).unwrap();

        // The scaffold output should be valid PACT source
        let mut sm = SourceMap::new();
        let id = sm.add("scaffold.pact", &result);
        let tokens = Lexer::new(&result, id).lex();
        assert!(
            tokens.is_ok(),
            "scaffolded source should lex successfully, got error: {:?}\nSource:\n{result}",
            tokens.err()
        );
        let tokens = tokens.unwrap();
        let parse_result = Parser::new(&tokens).parse();
        assert!(
            parse_result.is_ok(),
            "scaffolded source should parse successfully, got error: {:?}\nSource:\n{result}",
            parse_result.err()
        );
    }

    #[test]
    fn validate_permissions_warns_broad() {
        let args = json!({
            "source": "agent @risky { permits: [^net] tools: [#web_search] }"
        });
        let result = handle_tool_call("pact_validate_permissions", &args).unwrap();
        assert!(
            result.contains("Warning") && result.contains("broad"),
            "expected broad permission warning, got: {result}"
        );
    }

    #[test]
    fn validate_permissions_ok_for_specific() {
        let args = json!({
            "source": "agent @safe { permits: [^llm.query] tools: [#greet] }"
        });
        let result = handle_tool_call("pact_validate_permissions", &args).unwrap();
        assert!(
            result.contains("OK"),
            "expected OK for specific permissions, got: {result}"
        );
    }

    #[test]
    fn to_mermaid_converts_pact() {
        let args = json!({
            "source": r#"
                agent @researcher {
                    permits: [^llm.query]
                    tools: [#search]
                }
                flow main(query :: String) -> String {
                    result = @researcher -> #search(query)
                    return result
                }
            "#
        });
        let result = handle_tool_call("pact_to_mermaid", &args).unwrap();
        assert!(
            result.contains("agentflow"),
            "expected agentflow header, got: {result}"
        );
        assert!(
            result.contains("researcher"),
            "expected agent in output, got: {result}"
        );
    }

    #[test]
    fn from_mermaid_converts_agentflow() {
        let args = json!({
            "source": r#"agentflow TB
agent researcher["Researcher"]
    search
end
researcher@{
    model: "claude-sonnet-4-20250514"
    permits: "llm.query"
}
"#
        });
        let result = handle_tool_call("pact_from_mermaid", &args).unwrap();
        assert!(
            result.contains("agent @researcher"),
            "expected agent declaration, got: {result}"
        );
    }

    #[test]
    fn roundtrip_pact_to_mermaid_to_pact() {
        let pact_source = r#"
            agent @writer {
                permits: [^llm.query]
                tools: [#draft]
            }
            flow write(topic :: String) -> String {
                result = @writer -> #draft(topic)
                return result
            }
        "#;

        // PACT -> Mermaid
        let to_args = json!({ "source": pact_source });
        let mermaid = handle_tool_call("pact_to_mermaid", &to_args).unwrap();
        assert!(mermaid.contains("agentflow"));

        // Mermaid -> PACT
        let from_args = json!({ "source": mermaid });
        let pact_back = handle_tool_call("pact_from_mermaid", &from_args).unwrap();
        assert!(
            pact_back.contains("agent @writer"),
            "roundtrip should preserve agent, got: {pact_back}"
        );
    }
}
