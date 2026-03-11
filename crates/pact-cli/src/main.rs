// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-08-01

//! CLI frontend for the PACT language.
//!
//! Provides commands:
//!
//! - `pact init [file]` — scaffold a new `.pact` project file
//! - `pact check <file>` — lex, parse, and type-check a `.pact` file
//! - `pact build <file>` — compile to TOML configs, Markdown prompts, and Claude JSON
//! - `pact run <file> --flow <name>` — execute a flow with mock agents
//! - `pact test <file>` — run all test declarations
//! - `pact playground` — interactive REPL for experimenting with PACT
//! - `pact doc <file>` — generate Markdown documentation from a `.pact` file

use clap::{Parser, Subcommand};
use miette::{IntoDiagnostic, Result, WrapErr};
use std::fs;

use pact_build::config::{BuildConfig, Target};
use pact_core::ast::expr::ExprKind;
use pact_core::ast::stmt::DeclKind;
use pact_core::ast::types::{TypeExpr, TypeExprKind};
use pact_core::checker::Checker;
use pact_core::interpreter::value::Value;
use pact_core::interpreter::{Interpreter, MockDispatcher};
use pact_core::lexer::Lexer;
use pact_core::parser::Parser as PactParser;
use pact_core::span::SourceMap;

#[derive(Parser)]
#[command(
    name = "pact",
    version,
    about = "PACT — Programmable Agent Contract Toolkit",
    long_about = "A logic engine and compiler for AI agents.\nConsumes .pact files defining agents, flows, schemas, and permissions."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold a new .pact project file.
    Init {
        /// Output file path.
        #[arg(default_value = "main.pact")]
        file: String,

        /// Template to use: "minimal", "full", or "research".
        #[arg(long, default_value = "minimal")]
        template: String,
    },

    /// Type-check a .pact file without executing it.
    Check {
        /// Path to the .pact file.
        file: String,

        /// Watch for changes and re-check automatically.
        #[arg(long)]
        watch: bool,
    },

    /// Compile a .pact file into deployment artifacts (TOML, Markdown, JSON).
    Build {
        /// Path to the .pact file.
        file: String,

        /// Output directory for generated artifacts.
        #[arg(long, default_value = "./pact-out")]
        out_dir: String,

        /// Target platform (claude).
        #[arg(long, default_value = "claude")]
        target: String,

        /// Watch for changes and re-build automatically.
        #[arg(long)]
        watch: bool,
    },

    /// Execute a flow from a .pact file.
    Run {
        /// Path to the .pact file.
        file: String,

        /// Name of the flow to execute.
        #[arg(long)]
        flow: String,

        /// Arguments to pass to the flow (as strings).
        #[arg(long, value_delimiter = ',')]
        args: Option<Vec<String>>,

        /// Dispatch mode: mock, claude, openai, or ollama.
        #[arg(long, default_value = "mock")]
        dispatch: String,

        /// Enable streaming output (prints text token-by-token).
        #[arg(long)]
        stream: bool,
    },

    /// Run all test declarations in a .pact file.
    Test {
        /// Path to the .pact file.
        file: String,
    },

    /// Convert a Mermaid diagram to a .pact file.
    FromMermaid {
        /// Path to the .mmd file.
        file: String,

        /// Output .pact file path (default: prints to stdout).
        #[arg(long, short)]
        output: Option<String>,
    },

    /// Convert a .pact file to a Mermaid diagram.
    ToMermaid {
        /// Path to the .pact file.
        file: String,

        /// Output .mmd file path (default: prints to stdout).
        #[arg(long, short)]
        output: Option<String>,
    },

    /// Interactive REPL for experimenting with PACT.
    Playground {
        /// Optional .pact file to preload.
        #[arg(long)]
        load: Option<String>,
    },

    /// List built-in skills and prompt templates, or declarations in a .pact file.
    List {
        /// What to list: "skills", "prompts", "all", or "declarations".
        #[arg(default_value = "all")]
        what: String,
        /// Optional .pact file to inspect.
        #[arg(long)]
        file: Option<String>,
    },

    /// Format a .pact file.
    Fmt {
        /// Path to the .pact file.
        file: String,

        /// Write formatted output back to the file (in-place).
        #[arg(long)]
        write: bool,
    },

    /// Generate Markdown documentation from a .pact file.
    Doc {
        /// Path to the .pact file.
        file: String,

        /// Output .md file path (default: prints to stdout).
        #[arg(long, short)]
        output: Option<String>,
    },
}

fn main() -> Result<()> {
    // Install miette's fancy error handler
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .context_lines(3)
                .build(),
        )
    }))
    .ok();

    let cli = Cli::parse();

    match cli.command {
        Command::Init { file, template } => cmd_init(&file, &template),
        Command::Check { file, watch } => cmd_check(&file, watch),
        Command::Build {
            file,
            out_dir,
            target,
            watch,
        } => cmd_build(&file, &out_dir, &target, watch),
        Command::Run {
            file,
            flow,
            args,
            dispatch,
            stream,
        } => cmd_run(&file, &flow, args, &dispatch, stream),
        Command::Test { file } => cmd_test(&file),
        Command::FromMermaid { file, output } => cmd_from_mermaid(&file, output.as_deref()),
        Command::ToMermaid { file, output } => cmd_to_mermaid(&file, output.as_deref()),
        Command::Playground { load } => cmd_playground(load.as_deref()),
        Command::List { what, file } => cmd_list(&what, file.as_deref()),
        Command::Fmt { file, write } => cmd_fmt(&file, write),
        Command::Doc { file, output } => cmd_doc(&file, output.as_deref()),
    }
}

/// Load and lex+parse+check a .pact file. Returns the program and source map on success.
fn load_and_check(path: &str) -> Result<(pact_core::ast::stmt::Program, SourceMap)> {
    let source = fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read '{path}'"))?;

    let mut source_map = SourceMap::new();
    let source_id = source_map.add(path, &source);

    // Lex
    let tokens = Lexer::new(&source, source_id).lex().map_err(|e| {
        miette::Report::new(e).with_source_code(source_map.miette_source(source_id))
    })?;

    // Parse (with error recovery to report all parse errors)
    let (program, parse_errors) = PactParser::new(&tokens).parse_collecting_errors();
    if !parse_errors.is_empty() {
        for error in &parse_errors {
            let report = miette::Report::new(error.clone())
                .with_source_code(source_map.miette_source(source_id));
            eprintln!("{:?}", report);
        }
        return Err(miette::miette!(
            "{} parse error(s) found",
            parse_errors.len()
        ));
    }

    // Check
    let errors = Checker::new().check(&program);
    if !errors.is_empty() {
        for error in &errors {
            let report = miette::Report::new(error.clone())
                .with_source_code(source_map.miette_source(source_id));
            eprintln!("{:?}", report);
        }
        return Err(miette::miette!("{} semantic error(s) found", errors.len()));
    }

    Ok((program, source_map))
}

/// `pact init [file] [--template <name>]` — scaffold a new .pact project file.
fn cmd_init(file: &str, template: &str) -> Result<()> {
    let path = std::path::Path::new(file);
    if path.exists() {
        return Err(miette::miette!(
            "file already exists, use --force to overwrite"
        ));
    }

    let content = match template {
        "minimal" => TEMPLATE_MINIMAL,
        "full" => TEMPLATE_FULL,
        "research" => TEMPLATE_RESEARCH,
        other => {
            return Err(miette::miette!(
                "unknown template '{}'. Use: minimal, full, or research",
                other
            ))
        }
    };

    fs::write(file, content)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to write '{file}'"))?;

    println!("Created '{file}' (template: {template})");
    Ok(())
}

const TEMPLATE_MINIMAL: &str = r#"-- Generated by `pact init`

tool #greet {
    description: <<Generate a friendly greeting.>>
    requires: [^llm.query]
    params {
        name :: String
    }
    returns :: String
}

agent @assistant {
    permits: [^llm.query]
    tools: [#greet]
    prompt: <<You are a helpful assistant.>>
}

flow main(input :: String) -> String {
    result = @assistant -> #greet(input)
    return result
}
"#;

const TEMPLATE_FULL: &str = r#"-- Generated by `pact init`

permit_tree {
    ^llm {
        ^llm.query
    }
    ^net {
        ^net.read
        ^net.write
    }
}

tool #search {
    description: <<Search for information.>>
    requires: [^net.read]
    params {
        query :: String
    }
    returns :: List<String>
}

tool #analyze {
    description: <<Analyze the provided content.>>
    requires: [^llm.query]
    params {
        content :: String
    }
    returns :: String
}

agent @researcher {
    permits: [^net.read, ^llm.query]
    tools: [#search, #analyze]
    prompt: <<You are a thorough research assistant. Search for information and provide detailed analysis.>>
}

schema Report {
    title :: String
    body :: String
    sources :: List<String>
}

flow research(topic :: String) -> String {
    results = @researcher -> #search(topic)
    analysis = @researcher -> #analyze(results)
    return analysis
}

test "research flow works" {
    result = @researcher -> #search("test query")
    assert result == "search_result"
}
"#;

const TEMPLATE_RESEARCH: &str = r#"-- Generated by `pact init`

permit_tree {
    ^llm {
        ^llm.query
    }
    ^net {
        ^net.read
        ^net.write
    }
}

tool #search {
    description: <<Search the web for information about a given query.>>
    requires: [^net.read]
    params {
        query :: String
    }
    returns :: List<String>
}

tool #summarize {
    description: <<Summarize the provided content into a concise paragraph.>>
    requires: [^llm.query]
    params {
        content :: String
    }
    returns :: String
}

tool #draft_report {
    description: <<Draft a structured report from the provided summary.>>
    requires: [^llm.query]
    params {
        summary :: String
    }
    returns :: String
}

agent @researcher {
    permits: [^net.read, ^llm.query]
    tools: [#search, #summarize]
    prompt: <<You are a thorough research assistant. Search for information and provide detailed, well-sourced summaries.>>
}

agent @writer {
    permits: [^llm.query]
    tools: [#draft_report]
    prompt: <<You are a professional technical writer. Create clear, well-structured reports.>>
}

schema Report {
    title :: String
    body :: String
    sources :: List<String>
}

flow research_and_report(topic :: String) -> String {
    -- Step 1: Search for information
    search_results = @researcher -> #search(topic)

    -- Step 2: Summarize findings
    summary = @researcher -> #summarize(search_results)

    -- Step 3: Draft a report
    report = @writer -> #draft_report(summary)

    return report
}

flow safe_search(query :: String) -> String {
    result = @researcher -> #search(query) ?> @writer -> #draft_report(query)
    return result
}

test "research flow produces output" {
    result = @researcher -> #search("AI safety")
    assert result == "search_result"
}

test "writer can draft reports" {
    report = @writer -> #draft_report("test summary")
    assert report == "draft_report_result"
}
"#;

/// Watch a file for modifications, re-running the given action on each change.
fn watch_file(path: &str, mut action: impl FnMut()) -> Result<()> {
    use notify::{recommended_watcher, EventKind, RecursiveMode, Watcher};
    use std::sync::mpsc;

    println!(
        "Watching '{}' for changes... (Ctrl+C to stop)\n",
        path
    );

    let (tx, rx) = mpsc::channel();
    let mut watcher = recommended_watcher(move |res: std::result::Result<notify::Event, notify::Error>| {
        if let Ok(event) = res {
            if matches!(event.kind, EventKind::Modify(_)) {
                let _ = tx.send(());
            }
        }
    })
    .into_diagnostic()
    .wrap_err("failed to create file watcher")?;

    watcher
        .watch(std::path::Path::new(path), RecursiveMode::NonRecursive)
        .into_diagnostic()
        .wrap_err("failed to watch file")?;

    while let Ok(()) = rx.recv() {
        // Small debounce
        std::thread::sleep(std::time::Duration::from_millis(100));
        // Drain any queued events
        while rx.try_recv().is_ok() {}

        print!("\x1B[2J\x1B[H"); // Clear screen
        action();
        println!("\nWatching '{}' for changes... (Ctrl+C to stop)", path);
    }

    Ok(())
}

/// `pact check <file>` — lex, parse, check, and report.
fn cmd_check(path: &str, watch: bool) -> Result<()> {
    let _ = load_and_check(path)?;
    println!("OK — no errors found in '{path}'");

    if watch {
        let path_owned = path.to_string();
        watch_file(path, move || {
            match load_and_check(&path_owned) {
                Ok(_) => println!("OK — no errors found in '{}'", path_owned),
                Err(e) => eprintln!("{:?}", e),
            }
        })?;
    }

    Ok(())
}

/// `pact build <file>` — compile to output artifacts.
fn cmd_build(path: &str, out_dir: &str, target_str: &str, watch: bool) -> Result<()> {
    let (program, _sm) = load_and_check(path)?;

    let target = Target::parse(target_str)
        .ok_or_else(|| miette::miette!("unknown target '{}'. Supported: claude", target_str))?;

    let config = BuildConfig::new(path, out_dir, target);

    pact_build::build(&program, &config)
        .into_diagnostic()
        .wrap_err("build failed")?;

    println!("Built to '{out_dir}/' (target: {target_str})");

    // List generated files
    list_output_files(out_dir, 0);

    if watch {
        let path_owned = path.to_string();
        let out_dir_owned = out_dir.to_string();
        let target_str_owned = target_str.to_string();
        watch_file(path, move || {
            match load_and_check(&path_owned) {
                Ok((program, _sm)) => {
                    let target = match Target::parse(&target_str_owned) {
                        Some(t) => t,
                        None => {
                            eprintln!("unknown target '{}'", target_str_owned);
                            return;
                        }
                    };
                    let config = BuildConfig::new(&path_owned, &out_dir_owned, target);
                    match pact_build::build(&program, &config) {
                        Ok(()) => {
                            println!("Built to '{out_dir_owned}/' (target: {target_str_owned})");
                            list_output_files(&out_dir_owned, 0);
                        }
                        Err(e) => eprintln!("build failed: {e}"),
                    }
                }
                Err(e) => eprintln!("{:?}", e),
            }
        })?;
    }

    Ok(())
}

/// Recursively list files in a directory with indentation.
fn list_output_files(dir: &str, depth: usize) {
    if let Ok(entries) = fs::read_dir(dir) {
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let indent = "  ".repeat(depth);
            if entry.path().is_dir() {
                println!("{indent}{name}/");
                list_output_files(&entry.path().to_string_lossy(), depth + 1);
            } else {
                println!("{indent}{name}");
            }
        }
    }
}

/// `pact run <file> --flow <name> [--dispatch mock|claude]` — execute a flow.
fn cmd_run(
    path: &str,
    flow_name: &str,
    args: Option<Vec<String>>,
    dispatch: &str,
    stream: bool,
) -> Result<()> {
    let (program, _sm) = load_and_check(path)?;

    let arg_values: Vec<Value> = args
        .unwrap_or_default()
        .into_iter()
        .map(Value::String)
        .collect();

    // Streaming mode: connect directly to the Anthropic streaming API
    // and print text deltas in real-time.
    if stream {
        if dispatch != "claude" {
            return Err(miette::miette!(
                "--stream is only supported with --dispatch claude"
            ));
        }
        return cmd_run_stream(path, flow_name, &arg_values, &program);
    }

    let mut interpreter = match dispatch {
        "mock" => Interpreter::with_dispatcher(Box::new(MockDispatcher)),
        "claude" => {
            let dispatcher = pact_dispatch::ClaudeDispatcher::from_env()
                .map_err(|e| miette::miette!("{e}"))?;
            Interpreter::with_dispatcher(Box::new(dispatcher))
        }
        "openai" => {
            let dispatcher = pact_dispatch::OpenAIDispatcher::from_env()
                .map_err(|e| miette::miette!("{e}"))?;
            Interpreter::with_dispatcher(Box::new(dispatcher))
        }
        "ollama" => {
            let dispatcher = pact_dispatch::OllamaDispatcher::from_env()
                .map_err(|e| miette::miette!("{e}"))?;
            Interpreter::with_dispatcher(Box::new(dispatcher))
        }
        other => {
            return Err(miette::miette!(
                "unknown dispatch mode '{}'. Use: mock, claude, openai, ollama",
                other
            ))
        }
    };

    match interpreter.run(&program, flow_name, arg_values) {
        Ok(result) => {
            println!("\n=> {result}");
            Ok(())
        }
        Err(e) => Err(miette::miette!("{e}")),
    }
}

/// Execute a flow with streaming output from the Claude API.
///
/// Sends the request with streaming enabled and prints text deltas
/// to stderr in real-time as they arrive.
fn cmd_run_stream(
    _path: &str,
    flow_name: &str,
    arg_values: &[Value],
    program: &pact_core::ast::stmt::Program,
) -> Result<()> {
    use pact_build::emit_claude::build_agent_request;
    use pact_build::emit_markdown::generate_agent_prompt;
    use pact_dispatch::StreamEvent;
    use std::io::Write;

    // Find the flow and its first agent dispatch
    let flow = program.decls.iter().find_map(|d| match &d.kind {
        DeclKind::Flow(f) if f.name == flow_name => Some(f),
        _ => None,
    });
    let flow = flow.ok_or_else(|| miette::miette!("flow '{}' not found", flow_name))?;

    // Find the first agent dispatch expression in the flow body.
    // Flow body is Vec<Expr>; dispatches are AgentDispatch or Let bindings
    // wrapping an AgentDispatch.
    let (agent_name, tool_name) = find_dispatch_in_exprs(&flow.body)
        .ok_or_else(|| miette::miette!("no agent dispatch found in flow '{}'", flow_name))?;

    // Find the agent declaration
    let agent_decl = program.decls.iter().find_map(|d| match &d.kind {
        DeclKind::Agent(a) if a.name == agent_name => Some(a),
        _ => None,
    });
    let agent_decl =
        agent_decl.ok_or_else(|| miette::miette!("agent '{}' not found", agent_name))?;

    // Format the user message from args
    let user_message = pact_dispatch::convert::format_tool_call_message(&tool_name, arg_values);
    let mut request = build_agent_request(agent_decl, program, &user_message);
    request.system = Some(generate_agent_prompt(agent_decl, program));

    // Create the client and run the stream
    let client = pact_dispatch::client::AnthropicClient::from_env()
        .map_err(|e| miette::miette!("{e}"))?;

    let rt = tokio::runtime::Runtime::new()
        .into_diagnostic()
        .wrap_err("failed to create tokio runtime")?;

    rt.block_on(async {
        let mut rx = client
            .send_message_stream(&request)
            .await
            .map_err(|e| miette::miette!("{e}"))?;

        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::TextDelta(text) => {
                    eprint!("{}", text);
                    std::io::stderr().flush().ok();
                }
                StreamEvent::ToolUseStart { name, .. } => {
                    eprintln!("\n[stream] tool call: #{name}");
                }
                StreamEvent::MessageDone { stop_reason } => {
                    eprintln!("\n[stream] done (stop_reason: {stop_reason:?})");
                }
                _ => {}
            }
        }

        Ok(())
    })
}

/// Walk a slice of expressions to find the first `AgentDispatch`, returning
/// `(agent_name, tool_name)`. Handles both bare dispatches and `Let` bindings.
fn find_dispatch_in_exprs(
    exprs: &[pact_core::ast::expr::Expr],
) -> Option<(String, String)> {
    use pact_core::ast::expr::ExprKind;

    for expr in exprs {
        match &expr.kind {
            ExprKind::AgentDispatch { agent, tool, .. } => {
                let agent_name = match &agent.kind {
                    ExprKind::AgentRef(name) => name.clone(),
                    _ => continue,
                };
                let tool_name = match &tool.kind {
                    ExprKind::ToolRef(name) => name.clone(),
                    _ => continue,
                };
                return Some((agent_name, tool_name));
            }
            ExprKind::Assign { value, .. } => {
                if let ExprKind::AgentDispatch { agent, tool, .. } = &value.kind {
                    let agent_name = match &agent.kind {
                        ExprKind::AgentRef(name) => name.clone(),
                        _ => continue,
                    };
                    let tool_name = match &tool.kind {
                        ExprKind::ToolRef(name) => name.clone(),
                        _ => continue,
                    };
                    return Some((agent_name, tool_name));
                }
            }
            _ => {}
        }
    }
    None
}

/// `pact list [skills|prompts|all|declarations] [--file <path>]` — list built-in skills and prompts, or file declarations.
fn cmd_list(what: &str, file: Option<&str>) -> Result<()> {
    if let Some(path) = file {
        return cmd_list_declarations(path);
    }
    if what == "declarations" {
        return Err(miette::miette!(
            "the 'declarations' target requires --file <path>"
        ));
    }

    use pact_build::builtins::{BUILTIN_PROMPTS, BUILTIN_SKILLS};

    let show_skills = what == "all" || what == "skills";
    let show_prompts = what == "all" || what == "prompts";

    if !show_skills && !show_prompts {
        return Err(miette::miette!(
            "unknown list target '{}'. Use: skills, prompts, or all",
            what
        ));
    }

    if show_skills {
        println!("Built-in Skills ({} available):\n", BUILTIN_SKILLS.len());
        for skill in BUILTIN_SKILLS {
            println!("  ${:<25} {}", skill.name, skill.description);
        }
        println!();
        println!("Use `pact list skills` and copy the PACT source into your .pact file.");
        println!("Or define your own: skill $name {{ description: <<...>> tools: [...] strategy: <<...>> }}");
        println!();
    }

    if show_prompts {
        println!(
            "Built-in Prompt Templates ({} available):\n",
            BUILTIN_PROMPTS.len()
        );
        for prompt in BUILTIN_PROMPTS {
            let perms = prompt.suggested_permissions.join(", ");
            println!("  {:<25} {} [{}]", prompt.name, prompt.description, perms);
        }
        println!();
        println!(
            "These are ready-to-use agent prompts. Copy the PACT source into your .pact file."
        );
        println!();
    }

    Ok(())
}

/// Format a `TypeExpr` into a human-readable string.
fn format_type_expr(ty: &TypeExpr) -> String {
    match &ty.kind {
        TypeExprKind::Named(name) => name.clone(),
        TypeExprKind::Generic { name, args } => {
            let args_str: Vec<String> = args.iter().map(format_type_expr).collect();
            format!("{}<{}>", name, args_str.join(", "))
        }
        TypeExprKind::Optional(inner) => format!("{}?", format_type_expr(inner)),
    }
}

/// Extract a permission name from a PermissionRef expression.
fn format_permission(expr: &pact_core::ast::expr::Expr) -> String {
    match &expr.kind {
        ExprKind::PermissionRef(segments) => format!("!{}", segments.join(".")),
        _ => format!("{:?}", expr.kind),
    }
}

/// Extract a tool/agent/skill reference name from an expression.
fn format_ref(expr: &pact_core::ast::expr::Expr) -> String {
    match &expr.kind {
        ExprKind::ToolRef(name) => format!("#{name}"),
        ExprKind::AgentRef(name) => format!("@{name}"),
        ExprKind::SkillRef(name) => format!("${name}"),
        _ => format!("{:?}", expr.kind),
    }
}

/// `pact list declarations --file <path>` — list all declarations in a .pact file.
fn cmd_list_declarations(path: &str) -> Result<()> {
    let (program, _sm) = load_and_check(path)?;

    println!("Declarations in '{path}':\n");

    let mut agents = Vec::new();
    let mut bundles = Vec::new();
    let mut tools = Vec::new();
    let mut flows = Vec::new();
    let mut schemas = Vec::new();
    let mut type_aliases = Vec::new();
    let mut tests = Vec::new();
    let mut skills = Vec::new();

    for decl in &program.decls {
        match &decl.kind {
            DeclKind::Agent(a) => agents.push(a),
            DeclKind::AgentBundle(b) => bundles.push(b),
            DeclKind::Tool(t) => tools.push(t),
            DeclKind::Flow(f) => flows.push(f),
            DeclKind::Schema(s) => schemas.push(s),
            DeclKind::TypeAlias(t) => type_aliases.push(t),
            DeclKind::Test(t) => tests.push(t),
            DeclKind::Skill(s) => skills.push(s),
            DeclKind::PermitTree(_) => {} // Not listed individually
            DeclKind::Template(_) => {}   // Listed separately if needed
            DeclKind::Directive(_) => {}  // Listed separately if needed
            DeclKind::Import(_) => {}     // Resolved by loader
        }
    }

    if !agents.is_empty() {
        println!("  Agents ({}):", agents.len());
        for a in &agents {
            let permits: Vec<String> = a.permits.iter().map(format_permission).collect();
            let permits_str = if permits.is_empty() {
                String::new()
            } else {
                format!(" permits: [{}]", permits.join(", "))
            };
            println!(
                "    @{:<25} {} tool(s){}",
                a.name,
                a.tools.len(),
                permits_str
            );
        }
        println!();
    }

    if !tools.is_empty() {
        println!("  Tools ({}):", tools.len());
        for t in &tools {
            let handler_str = match &t.handler {
                Some(h) => format!(" handler: {h}"),
                None => String::new(),
            };
            let requires: Vec<String> = t.requires.iter().map(format_permission).collect();
            let requires_str = if requires.is_empty() {
                String::new()
            } else {
                format!(" requires: [{}]", requires.join(", "))
            };
            println!("    #{:<25}{}{}", t.name, handler_str, requires_str);
        }
        println!();
    }

    if !flows.is_empty() {
        println!("  Flows ({}):", flows.len());
        for f in &flows {
            let params: Vec<String> = f
                .params
                .iter()
                .map(|p| match &p.ty {
                    Some(ty) => format!("{} :: {}", p.name, format_type_expr(ty)),
                    None => p.name.clone(),
                })
                .collect();
            let ret = match &f.return_type {
                Some(ty) => format!(" -> {}", format_type_expr(ty)),
                None => String::new(),
            };
            println!("    {}({}){}", f.name, params.join(", "), ret);
        }
        println!();
    }

    if !schemas.is_empty() {
        println!("  Schemas ({}):", schemas.len());
        for s in &schemas {
            println!("    {} {{ {} field(s) }}", s.name, s.fields.len());
        }
        println!();
    }

    if !type_aliases.is_empty() {
        println!("  Type Aliases ({}):", type_aliases.len());
        for t in &type_aliases {
            println!("    {} = {}", t.name, t.variants.join(" | "));
        }
        println!();
    }

    if !tests.is_empty() {
        println!("  Tests ({}):", tests.len());
        for t in &tests {
            println!("    \"{}\"", t.description);
        }
        println!();
    }

    if !skills.is_empty() {
        println!("  Skills ({}):", skills.len());
        for s in &skills {
            let tools: Vec<String> = s.tools.iter().map(format_ref).collect();
            let tools_str = if tools.is_empty() {
                String::new()
            } else {
                format!(" tools: [{}]", tools.join(", "))
            };
            println!("    ${:<25}{}", s.name, tools_str);
        }
        println!();
    }

    if !bundles.is_empty() {
        println!("  Agent Bundles ({}):", bundles.len());
        for b in &bundles {
            let members: Vec<String> = b.agents.iter().map(format_ref).collect();
            println!("    @{:<25} members: [{}]", b.name, members.join(", "));
        }
        println!();
    }

    let total = agents.len()
        + bundles.len()
        + tools.len()
        + flows.len()
        + schemas.len()
        + type_aliases.len()
        + tests.len()
        + skills.len();
    println!("Total: {total} declaration(s)");

    Ok(())
}

/// `pact test <file>` — run all test declarations.
/// `pact from-mermaid <file> [-o output.pact]` — convert Mermaid to PACT.
fn cmd_from_mermaid(path: &str, output: Option<&str>) -> Result<()> {
    let source = fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to read '{path}'"))?;

    let pact_source = pact_mermaid::mermaid_to_pact(&source)
        .map_err(|e| miette::miette!("{e}"))?;

    if let Some(out_path) = output {
        fs::write(out_path, &pact_source)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to write '{out_path}'"))?;
        println!("Converted '{}' -> '{}'", path, out_path);
    } else {
        println!("{pact_source}");
    }

    Ok(())
}

/// `pact to-mermaid <file> [-o output.mmd]` — convert PACT to Mermaid.
fn cmd_to_mermaid(path: &str, output: Option<&str>) -> Result<()> {
    let (program, _sm) = load_and_check(path)?;

    let mermaid_source = pact_mermaid::pact_to_mermaid(&program);

    if let Some(out_path) = output {
        fs::write(out_path, &mermaid_source)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to write '{out_path}'"))?;
        println!("Converted '{}' -> '{}'", path, out_path);
    } else {
        println!("{mermaid_source}");
    }

    Ok(())
}

fn cmd_test(path: &str) -> Result<()> {
    let (program, _sm) = load_and_check(path)?;

    let mut interpreter = Interpreter::new();
    let results = interpreter.run_tests(&program);

    let total = results.len();
    let passed = results.iter().filter(|(_, p)| *p).count();
    let failed = total - passed;

    println!("\n{passed}/{total} tests passed");
    if failed > 0 {
        Err(miette::miette!("{failed} test(s) failed"))
    } else {
        Ok(())
    }
}

/// `pact doc <file> [-o output.md]` — generate Markdown documentation.
/// `pact fmt <file> [--write]` — format a .pact file.
fn cmd_fmt(path: &str, write: bool) -> Result<()> {
    let (program, _sm) = load_and_check(path)?;

    let formatted = pact_core::formatter::format_program(&program);

    if write {
        fs::write(path, &formatted)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to write '{path}'"))?;
        println!("Formatted '{path}'");
    } else {
        print!("{formatted}");
    }

    Ok(())
}

fn cmd_doc(path: &str, output: Option<&str>) -> Result<()> {
    let (program, _sm) = load_and_check(path)?;

    let title = std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());

    let markdown = pact_core::doc::generate_docs(&program, &title);

    if let Some(out_path) = output {
        fs::write(out_path, &markdown)
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to write '{out_path}'"))?;
        println!("Generated documentation: '{}'", out_path);
    } else {
        print!("{markdown}");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Playground (interactive REPL)
// ---------------------------------------------------------------------------

use pact_core::ast::stmt::{Decl, Program};

/// `pact playground [--load <file>]` — interactive REPL for experimenting with PACT.
fn cmd_playground(preload: Option<&str>) -> Result<()> {
    use rustyline::error::ReadlineError;
    use rustyline::DefaultEditor;

    let version = env!("CARGO_PKG_VERSION");
    println!();
    println!("  ╔═══════════════════════════════════════════╗");
    println!("  ║  PACT Playground v{:<25}║", version);
    println!("  ║  Programmable Agent Contract Toolkit      ║");
    println!("  ║                                           ║");
    println!("  ║  Type :help for available commands.       ║");
    println!("  ║  Type :quit to exit.                      ║");
    println!("  ╚═══════════════════════════════════════════╝");
    println!();

    let mut decls: Vec<Decl> = Vec::new();
    let mut interpreter = Interpreter::with_dispatcher(Box::new(MockDispatcher));
    let mut source_map = SourceMap::new();
    let mut input_counter: u32 = 0;

    // Preload a file if specified.
    if let Some(path) = preload {
        match playground_load_file(path, &mut decls, &mut interpreter, &mut source_map, &mut input_counter) {
            Ok(count) => println!("Loaded {count} declaration(s) from '{path}'"),
            Err(msg) => eprintln!("Error loading '{path}': {msg}"),
        }
    }

    let mut rl = DefaultEditor::new().into_diagnostic()?;

    // Try to load history from a file (ignore errors).
    let history_path = dirs_history_path();
    if let Some(ref p) = history_path {
        let _ = rl.load_history(p);
    }

    loop {
        let readline = rl.readline("pact> ");
        match readline {
            Ok(line) => {
                let trimmed = line.trim();

                // Skip empty lines.
                if trimmed.is_empty() {
                    continue;
                }

                let _ = rl.add_history_entry(&line);

                // Handle REPL commands.
                if trimmed.starts_with(':') {
                    if handle_repl_command(trimmed, &mut decls, &mut interpreter, &mut source_map, &mut input_counter) {
                        break; // :quit / :exit
                    }
                    continue;
                }

                // Accumulate multi-line input if the line ends with `{`.
                let full_input = if trimmed.ends_with('{') {
                    match read_multiline(&mut rl, &line) {
                        Ok(input) => input,
                        Err(msg) => {
                            eprintln!("Error: {msg}");
                            continue;
                        }
                    }
                } else {
                    line.clone()
                };

                // Try to evaluate.
                playground_eval(
                    &full_input,
                    &mut decls,
                    &mut interpreter,
                    &mut source_map,
                    &mut input_counter,
                );
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                continue;
            }
            Err(ReadlineError::Eof) => {
                println!("Goodbye!");
                break;
            }
            Err(err) => {
                eprintln!("Error: {err}");
                break;
            }
        }
    }

    // Save history.
    if let Some(ref p) = history_path {
        let _ = rl.save_history(p);
    }

    Ok(())
}

/// Return a path for REPL history, or None if we can't determine one.
fn dirs_history_path() -> Option<String> {
    std::env::var("HOME")
        .ok()
        .map(|home| format!("{home}/.pact_history"))
}

/// Read additional lines until braces are balanced.
fn read_multiline(rl: &mut rustyline::DefaultEditor, first_line: &str) -> std::result::Result<String, String> {
    let mut buf = String::from(first_line);
    buf.push('\n');
    let mut depth: i32 = 0;

    // Count braces in the first line.
    for ch in first_line.chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
    }

    while depth > 0 {
        let prompt = format!("{:width$}| ", "", width = 3);
        match rl.readline(&prompt) {
            Ok(line) => {
                let _ = rl.add_history_entry(&line);
                for ch in line.chars() {
                    match ch {
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        _ => {}
                    }
                }
                buf.push_str(&line);
                buf.push('\n');
            }
            Err(_) => return Err("interrupted while reading multi-line input".into()),
        }
    }

    Ok(buf)
}

/// Handle a REPL `:command`. Returns `true` if the REPL should exit.
fn handle_repl_command(
    input: &str,
    decls: &mut Vec<Decl>,
    interpreter: &mut Interpreter,
    source_map: &mut SourceMap,
    input_counter: &mut u32,
) -> bool {
    let parts: Vec<&str> = input.splitn(2, ' ').collect();
    let cmd = parts[0];
    let arg = parts.get(1).map(|s| s.trim()).unwrap_or("");

    match cmd {
        ":quit" | ":exit" | ":q" => {
            println!("Goodbye!");
            return true;
        }
        ":help" | ":h" => {
            println!();
            println!("  Available commands:");
            println!("    :help              Show this help message");
            println!("    :list              List all declared items");
            println!("    :clear             Reset all declarations");
            println!("    :load <file>       Load a .pact file into the session");
            println!("    :type <expr>       Show the inferred type of an expression");
            println!("    :env KEY VALUE     Set an environment variable");
            println!("    :memory <agent>    Show agent memory contents");
            println!("    :cache clear       Clear the tool result cache");
            println!("    :run <flow(args)>  Run a flow from a loaded file");
            println!("    :quit / :exit      Exit the playground");
            println!();
            println!("  You can enter any PACT declaration (agent, tool, flow, schema, etc.)");
            println!("  or expression. Declarations persist across inputs.");
            println!("  Multi-line input: if a line ends with '{{', keep typing until '}}'.");
            println!();
        }
        ":list" | ":ls" => {
            playground_list_decls(decls);
        }
        ":clear" => {
            decls.clear();
            *interpreter = Interpreter::with_dispatcher(Box::new(MockDispatcher));
            *source_map = SourceMap::new();
            *input_counter = 0;
            println!("All declarations cleared.");
        }
        ":load" | ":l" => {
            if arg.is_empty() {
                eprintln!("Usage: :load <file.pact>");
            } else {
                match playground_load_file(arg, decls, interpreter, source_map, input_counter) {
                    Ok(count) => println!("Loaded {count} declaration(s) from '{arg}'"),
                    Err(msg) => eprintln!("Error: {msg}"),
                }
            }
        }
        ":type" | ":t" => {
            if arg.is_empty() {
                eprintln!("Usage: :type <expression>");
            } else {
                playground_type_expr(arg, decls, source_map, input_counter);
            }
        }
        ":env" => {
            let env_parts: Vec<&str> = arg.splitn(2, ' ').collect();
            if env_parts.len() < 2 || env_parts[0].is_empty() {
                eprintln!("Usage: :env KEY VALUE");
            } else {
                let key = env_parts[0];
                let value = env_parts[1];
                std::env::set_var(key, value);
                println!("Set {}={}", key, value);
            }
        }
        ":memory" => {
            if arg.is_empty() {
                eprintln!("Usage: :memory <agent_name>");
            } else {
                let store = pact_core::memory::MemoryStore::load(arg);
                let keys = store.keys();
                if keys.is_empty() {
                    println!("No memory stored for agent '{}'.", arg);
                } else {
                    println!("Memory for agent '{}':", arg);
                    for key in &keys {
                        if let Some(val) = store.get(key) {
                            println!("  {} = {}", key, val);
                        }
                    }
                }
            }
        }
        ":cache" => {
            if arg == "clear" {
                pact_dispatch::cache::global_cache().clear();
                println!("Tool cache cleared.");
            } else {
                eprintln!("Usage: :cache clear");
            }
        }
        ":run" => {
            if arg.is_empty() {
                eprintln!("Usage: :run flow_name(arg1, arg2, ...)");
            } else {
                // Parse flow name and arguments: flow_name(arg1, arg2)
                let (flow_name, flow_args) = if let Some(paren_start) = arg.find('(') {
                    let name = &arg[..paren_start];
                    let args_str = arg[paren_start + 1..].trim_end_matches(')');
                    let args: Vec<Value> = if args_str.trim().is_empty() {
                        vec![]
                    } else {
                        args_str
                            .split(',')
                            .map(|a| {
                                let a = a.trim().trim_matches('"');
                                Value::String(a.to_string())
                            })
                            .collect()
                    };
                    (name, args)
                } else {
                    (arg, vec![])
                };

                let full_program = pact_core::ast::stmt::Program {
                    decls: decls.clone(),
                };
                let mut run_interp = Interpreter::with_dispatcher(Box::new(MockDispatcher));
                match run_interp.run(&full_program, flow_name, flow_args) {
                    Ok(value) => println!("=> {value}"),
                    Err(e) => eprintln!("Runtime error: {e}"),
                }
            }
        }
        other => {
            eprintln!("Unknown command '{other}'. Type :help for available commands.");
        }
    }

    false
}

/// List all declarations currently in the playground session.
fn playground_list_decls(decls: &[Decl]) {
    if decls.is_empty() {
        println!("No declarations yet. Define agents, tools, flows, etc.");
        return;
    }

    let mut agents = Vec::new();
    let mut tools = Vec::new();
    let mut flows = Vec::new();
    let mut schemas = Vec::new();
    let mut type_aliases = Vec::new();
    let mut tests = Vec::new();
    let mut skills = Vec::new();
    let mut bundles = Vec::new();
    let mut permit_trees = 0usize;

    for decl in decls {
        match &decl.kind {
            DeclKind::Agent(a) => agents.push(format!("@{}", a.name)),
            DeclKind::AgentBundle(b) => bundles.push(format!("@{}", b.name)),
            DeclKind::Tool(t) => tools.push(format!("#{}", t.name)),
            DeclKind::Flow(f) => flows.push(f.name.clone()),
            DeclKind::Schema(s) => schemas.push(s.name.clone()),
            DeclKind::TypeAlias(t) => type_aliases.push(t.name.clone()),
            DeclKind::Test(t) => tests.push(t.description.clone()),
            DeclKind::Skill(s) => skills.push(format!("${}", s.name)),
            DeclKind::PermitTree(_) => permit_trees += 1,
            DeclKind::Template(_) => {}   // Templates are structural
            DeclKind::Directive(_) => {}  // Directives are structural
            DeclKind::Import(_) => {} // Resolved by loader
        }
    }

    println!();
    if !agents.is_empty() {
        println!("  Agents:       {}", agents.join(", "));
    }
    if !bundles.is_empty() {
        println!("  Bundles:      {}", bundles.join(", "));
    }
    if !tools.is_empty() {
        println!("  Tools:        {}", tools.join(", "));
    }
    if !flows.is_empty() {
        println!("  Flows:        {}", flows.join(", "));
    }
    if !schemas.is_empty() {
        println!("  Schemas:      {}", schemas.join(", "));
    }
    if !type_aliases.is_empty() {
        println!("  Type Aliases: {}", type_aliases.join(", "));
    }
    if !skills.is_empty() {
        println!("  Skills:       {}", skills.join(", "));
    }
    if !tests.is_empty() {
        let quoted: Vec<String> = tests.iter().map(|t| format!("\"{}\"", t)).collect();
        println!("  Tests:        {}", quoted.join(", "));
    }
    if permit_trees > 0 {
        println!("  Permit Trees: {permit_trees}");
    }
    let total = decls.len();
    println!();
    println!("  Total: {total} declaration(s)");
    println!();
}

/// Load a .pact file into the playground session.
fn playground_load_file(
    path: &str,
    decls: &mut Vec<Decl>,
    interpreter: &mut Interpreter,
    source_map: &mut SourceMap,
    input_counter: &mut u32,
) -> std::result::Result<usize, String> {
    let source = fs::read_to_string(path).map_err(|e| format!("failed to read '{path}': {e}"))?;

    let source_id = source_map.add(path, &source);

    let tokens = Lexer::new(&source, source_id)
        .lex()
        .map_err(|e| format!("{e}"))?;

    let (program, parse_errors) = PactParser::new(&tokens).parse_collecting_errors();
    if !parse_errors.is_empty() {
        return Err(format!("{} parse error(s)", parse_errors.len()));
    }

    let errors = Checker::new().check(&program);
    if !errors.is_empty() {
        for error in &errors {
            let report = miette::Report::new(error.clone())
                .with_source_code(source_map.miette_source(source_id));
            eprintln!("{:?}", report);
        }
        return Err(format!("{} semantic error(s)", errors.len()));
    }

    let count = program.decls.len();
    decls.extend(program.decls.clone());

    // Reload the interpreter with all accumulated declarations.
    let full_program = Program { decls: decls.clone() };
    *interpreter = Interpreter::with_dispatcher(Box::new(MockDispatcher));
    interpreter.load(&full_program);

    *input_counter += 1;

    Ok(count)
}

/// Try to evaluate user input as either a declaration or an expression.
fn playground_eval(
    input: &str,
    decls: &mut Vec<Decl>,
    interpreter: &mut Interpreter,
    source_map: &mut SourceMap,
    input_counter: &mut u32,
) {
    let source_name = format!("<repl:{}>", *input_counter);
    *input_counter += 1;

    // First, try to parse as a complete program (i.e., one or more declarations).
    let source_id = source_map.add(&source_name, input);
    let tokens_result = Lexer::new(input, source_id).lex();
    let tokens = match tokens_result {
        Ok(t) => t,
        Err(e) => {
            let report = miette::Report::new(e)
                .with_source_code(source_map.miette_source(source_id));
            eprintln!("{:?}", report);
            return;
        }
    };

    // Try parsing as a full program (declarations).
    let (program, parse_errors) = PactParser::new(&tokens).parse_collecting_errors();

    if parse_errors.is_empty() && !program.decls.is_empty() {
        // Check if any of the parsed declarations look like they're real declarations
        // (not just the wrapper flow we'd create for expressions).
        // If it parsed successfully as declarations, accept them.
        let mut new_decls = decls.clone();
        new_decls.extend(program.decls.clone());

        let full_program = Program { decls: new_decls.clone() };
        let check_errors = Checker::new().check(&full_program);
        if !check_errors.is_empty() {
            for error in &check_errors {
                let report = miette::Report::new(error.clone())
                    .with_source_code(source_map.miette_source(source_id));
                eprintln!("{:?}", report);
            }
            return;
        }

        // Successfully parsed and checked as declarations.
        let count = program.decls.len();
        for decl in &program.decls {
            match &decl.kind {
                DeclKind::Agent(a) => println!("Defined agent @{}", a.name),
                DeclKind::AgentBundle(b) => println!("Defined agent bundle @{}", b.name),
                DeclKind::Tool(t) => println!("Defined tool #{}", t.name),
                DeclKind::Flow(f) => println!("Defined flow {}", f.name),
                DeclKind::Schema(s) => println!("Defined schema {}", s.name),
                DeclKind::TypeAlias(t) => println!("Defined type {}", t.name),
                DeclKind::Skill(s) => println!("Defined skill ${}", s.name),
                DeclKind::PermitTree(_) => println!("Defined permit_tree"),
                DeclKind::Template(t) => println!("Defined template %{}", t.name),
                DeclKind::Directive(d) => println!("Defined directive %{}", d.name),
                DeclKind::Test(t) => println!("Defined test \"{}\"", t.description),
                DeclKind::Import(i) => println!("Import \"{}\"", i.path),
            }
        }

        *decls = new_decls;

        // Reload the interpreter.
        let full_program = Program { decls: decls.clone() };
        *interpreter = Interpreter::with_dispatcher(Box::new(MockDispatcher));
        interpreter.load(&full_program);

        if count > 0 {
            return;
        }
    }

    // If declaration parsing failed, try wrapping as an expression inside a flow.
    let wrapper = format!(
        "flow __repl__() -> String {{\n{}\n}}",
        input
    );

    // Build a full source with existing declarations serialized minimally,
    // plus the wrapper flow. But we only need the wrapper parsed — we'll
    // evaluate it with the interpreter that already has the declarations loaded.
    let wrapper_source_name = format!("<repl-expr:{}>", *input_counter - 1);
    let wrapper_source_id = source_map.add(&wrapper_source_name, &wrapper);

    let wrapper_tokens = match Lexer::new(&wrapper, wrapper_source_id).lex() {
        Ok(t) => t,
        Err(e) => {
            // If even wrapping fails, show the original parse errors.
            if !parse_errors.is_empty() {
                for error in &parse_errors {
                    let report = miette::Report::new(error.clone())
                        .with_source_code(source_map.miette_source(source_id));
                    eprintln!("{:?}", report);
                }
            } else {
                let report = miette::Report::new(e)
                    .with_source_code(source_map.miette_source(wrapper_source_id));
                eprintln!("{:?}", report);
            }
            return;
        }
    };

    let (wrapper_program, wrapper_parse_errors) = PactParser::new(&wrapper_tokens).parse_collecting_errors();
    if !wrapper_parse_errors.is_empty() {
        // Show original parse errors since expression wrapping also failed.
        if !parse_errors.is_empty() {
            for error in &parse_errors {
                let report = miette::Report::new(error.clone())
                    .with_source_code(source_map.miette_source(source_id));
                eprintln!("{:?}", report);
            }
        } else {
            for error in &wrapper_parse_errors {
                let report = miette::Report::new(error.clone())
                    .with_source_code(source_map.miette_source(wrapper_source_id));
                eprintln!("{:?}", report);
            }
        }
        return;
    }

    // Build a combined program with existing decls + the __repl__ flow.
    let mut eval_decls = decls.clone();
    eval_decls.extend(wrapper_program.decls);
    let eval_program = Program { decls: eval_decls };

    // Skip type-checking for REPL expressions to be more lenient.

    // Run the __repl__ flow.
    let mut eval_interp = Interpreter::with_dispatcher(Box::new(MockDispatcher));
    match eval_interp.run(&eval_program, "__repl__", vec![]) {
        Ok(value) => {
            match &value {
                Value::Null => {} // Don't print null for side-effect-only expressions
                _ => println!("=> {value}"),
            }
        }
        Err(e) => {
            eprintln!("Runtime error: {e}");
        }
    }
}

/// Show the basic type of an expression by evaluating it.
fn playground_type_expr(
    input: &str,
    decls: &[Decl],
    source_map: &mut SourceMap,
    input_counter: &mut u32,
) {
    let wrapper = format!("flow __repl__() -> String {{\nreturn {}\n}}", input);
    let source_name = format!("<repl-type:{}>", *input_counter);
    *input_counter += 1;
    let source_id = source_map.add(&source_name, &wrapper);

    let tokens = match Lexer::new(&wrapper, source_id).lex() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Parse error: {e}");
            return;
        }
    };

    let (wrapper_program, parse_errors) = PactParser::new(&tokens).parse_collecting_errors();
    if !parse_errors.is_empty() {
        for error in &parse_errors {
            let report = miette::Report::new(error.clone())
                .with_source_code(source_map.miette_source(source_id));
            eprintln!("{:?}", report);
        }
        return;
    }

    let mut eval_decls = decls.to_vec();
    eval_decls.extend(wrapper_program.decls);
    let eval_program = Program { decls: eval_decls };

    let mut eval_interp = Interpreter::with_dispatcher(Box::new(MockDispatcher));
    match eval_interp.run(&eval_program, "__repl__", vec![]) {
        Ok(value) => {
            println!("{input} :: {}", value.type_name());
        }
        Err(e) => {
            eprintln!("Error: {e}");
        }
    }
}
