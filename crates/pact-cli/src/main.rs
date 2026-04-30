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

        /// Target platform (claude, openai, crewai, cursor, gemini).
        #[arg(long, default_value = "claude")]
        target: String,

        /// Watch for changes and re-build automatically.
        #[arg(long)]
        watch: bool,

        /// Generate Claude Code custom skill files (.claude/skills/).
        #[arg(long)]
        claude_skill: bool,

        /// Generate a CLAUDE.md process memory file.
        #[arg(long)]
        claude_md: bool,

        /// Generate MCP server recommendations.
        #[arg(long)]
        recommend_mcp: bool,

        /// Generate agent card JSON files for A2A discovery.
        #[arg(long)]
        agent_cards: bool,
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

        /// Maximum API calls per agent before rate limiting.
        #[arg(long, default_value = "100")]
        max_calls: u64,

        /// Maximum tokens per flow before rate limiting.
        #[arg(long, default_value = "100000")]
        max_tokens: u64,

        /// Maximum total API calls across all agents.
        #[arg(long, default_value = "1000")]
        max_global_calls: u64,

        /// Output file path for the run report (supports .html).
        #[arg(long, short)]
        output: Option<String>,
    },

    /// Run all test declarations in a .pact file.
    Test {
        /// Path to the .pact file.
        file: String,
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

    /// MCP server operations.
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },

    /// Install dependencies from Pact.toml.
    Install,

    /// Add a dependency to Pact.toml.
    Add {
        /// Package spec: "org/repo" or "org/repo@version".
        package: String,
    },

    /// Search for PACT packages on GitHub.
    Search {
        /// Search query.
        query: String,
    },

    /// Federation operations — remote agent discovery and management.
    Federation {
        #[command(subcommand)]
        action: FederationAction,
    },
}

/// MCP subcommands.
#[derive(Subcommand)]
enum McpAction {
    /// List tools available on an MCP server declared in a .pact file.
    ListTools {
        /// Name of the MCP server (as declared in the connect block).
        server: String,

        /// Path to the .pact file containing the connect block.
        #[arg(long)]
        file: String,
    },
}

/// Federation subcommands.
#[derive(Subcommand)]
enum FederationAction {
    /// Discover remote agents from federation registries declared in a .pact file.
    Discover {
        /// Path to the .pact file containing a federation block.
        file: String,

        /// Optional agent name filter.
        #[arg(long)]
        query: Option<String>,
    },

    /// Register local agents with a remote federation registry.
    Register {
        /// Path to the .pact file containing agents to register.
        file: String,

        /// Registry URL to register agents with.
        #[arg(long)]
        registry: String,
    },

    /// Check the health of federation registries declared in a .pact file.
    Health {
        /// Path to the .pact file containing a federation block.
        file: String,
    },
}

fn main() -> Result<()> {
    // Initialize structured logging (respects RUST_LOG, defaults to info for pact_dispatch)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("pact_dispatch=info")),
        )
        .with_writer(std::io::stderr)
        .init();

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
            claude_skill,
            claude_md,
            recommend_mcp,
            agent_cards,
        } => cmd_build(
            &file,
            &out_dir,
            &target,
            watch,
            claude_skill,
            claude_md,
            recommend_mcp,
            agent_cards,
        ),
        Command::Run {
            file,
            flow,
            args,
            dispatch,
            stream,
            max_calls,
            max_tokens,
            max_global_calls,
            output,
        } => cmd_run(
            &file,
            &flow,
            args,
            &dispatch,
            stream,
            max_calls,
            max_tokens,
            max_global_calls,
            output.as_deref(),
        ),
        Command::Test { file } => cmd_test(&file),
        Command::Playground { load } => cmd_playground(load.as_deref()),
        Command::List { what, file } => cmd_list(&what, file.as_deref()),
        Command::Fmt { file, write } => cmd_fmt(&file, write),
        Command::Doc { file, output } => cmd_doc(&file, output.as_deref()),
        Command::Mcp { action } => match action {
            McpAction::ListTools { server, file } => cmd_mcp_list_tools(&server, &file),
        },
        Command::Install => cmd_install(),
        Command::Add { package } => cmd_add(&package),
        Command::Search { query } => cmd_search(&query),
        Command::Federation { action } => match action {
            FederationAction::Discover { file, query } => {
                cmd_federation_discover(&file, query.as_deref())
            }
            FederationAction::Register { file, registry } => {
                cmd_federation_register(&file, &registry)
            }
            FederationAction::Health { file } => cmd_federation_health(&file),
        },
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

    println!("Watching '{}' for changes... (Ctrl+C to stop)\n", path);

    let (tx, rx) = mpsc::channel();
    let mut watcher = recommended_watcher(
        move |res: std::result::Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                if matches!(event.kind, EventKind::Modify(_)) {
                    let _ = tx.send(());
                }
            }
        },
    )
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
        watch_file(path, move || match load_and_check(&path_owned) {
            Ok(_) => println!("OK — no errors found in '{}'", path_owned),
            Err(e) => eprintln!("{:?}", e),
        })?;
    }

    Ok(())
}

/// `pact build <file>` — compile to output artifacts.
#[allow(clippy::too_many_arguments)]
fn cmd_build(
    path: &str,
    out_dir: &str,
    target_str: &str,
    watch: bool,
    claude_skill: bool,
    claude_md: bool,
    recommend_mcp: bool,
    agent_cards: bool,
) -> Result<()> {
    let (program, _sm) = load_and_check(path)?;

    let target = Target::parse(target_str).ok_or_else(|| {
        miette::miette!(
            "unknown target '{}'. Supported: {}",
            target_str,
            Target::all_names().join(", ")
        )
    })?;

    let mut config = BuildConfig::new(path, out_dir, target);
    config.emit_claude_skill = claude_skill;
    config.emit_claude_md = claude_md;
    config.emit_mcp_recommendations = recommend_mcp;
    config.emit_agent_cards = agent_cards;

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
        watch_file(path, move || match load_and_check(&path_owned) {
            Ok((program, _sm)) => {
                let target = match Target::parse(&target_str_owned) {
                    Some(t) => t,
                    None => {
                        eprintln!("unknown target '{}'", target_str_owned);
                        return;
                    }
                };
                let mut config = BuildConfig::new(&path_owned, &out_dir_owned, target);
                config.emit_claude_skill = claude_skill;
                config.emit_claude_md = claude_md;
                config.emit_mcp_recommendations = recommend_mcp;
                config.emit_agent_cards = agent_cards;
                match pact_build::build(&program, &config) {
                    Ok(()) => {
                        println!("Built to '{out_dir_owned}/' (target: {target_str_owned})");
                        list_output_files(&out_dir_owned, 0);
                    }
                    Err(e) => eprintln!("build failed: {e}"),
                }
            }
            Err(e) => eprintln!("{:?}", e),
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
#[allow(clippy::too_many_arguments)]
fn cmd_run(
    path: &str,
    flow_name: &str,
    args: Option<Vec<String>>,
    dispatch: &str,
    stream: bool,
    max_calls: u64,
    max_tokens: u64,
    max_global_calls: u64,
    output: Option<&str>,
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
        return cmd_run_stream(path, flow_name, &arg_values, &program, dispatch);
    }

    let rate_config = pact_dispatch::RateLimitConfig {
        max_calls_per_agent: max_calls,
        max_tokens_per_flow: max_tokens,
        max_global_calls,
    };

    let mut interpreter = match dispatch {
        "mock" => Interpreter::with_dispatcher(Box::new(MockDispatcher)),
        "claude" => {
            let dispatcher = pact_dispatch::ClaudeDispatcher::from_env()
                .map_err(|e| miette::miette!("{e}"))?
                .with_rate_limits(rate_config);
            Interpreter::with_dispatcher(Box::new(dispatcher))
        }
        "openai" => {
            let dispatcher = pact_dispatch::OpenAIDispatcher::from_env()
                .map_err(|e| miette::miette!("{e}"))?
                .with_rate_limits(rate_config);
            Interpreter::with_dispatcher(Box::new(dispatcher))
        }
        "ollama" => {
            let dispatcher = pact_dispatch::OllamaDispatcher::from_env()
                .map_err(|e| miette::miette!("{e}"))?
                .with_rate_limits(rate_config);
            Interpreter::with_dispatcher(Box::new(dispatcher))
        }
        other => {
            return Err(miette::miette!(
                "unknown dispatch mode '{}'. Use: mock, claude, openai, ollama",
                other
            ))
        }
    };

    let start = std::time::Instant::now();
    match interpreter.run(&program, flow_name, arg_values) {
        Ok(result) => {
            let elapsed = start.elapsed();
            println!("\n=> {result}");

            if let Some(out_path) = output {
                write_html_report(out_path, path, flow_name, dispatch, elapsed, &result)?;
                println!("Report written to '{out_path}'");
            }

            Ok(())
        }
        Err(e) => Err(miette::miette!("{e}")),
    }
}

/// Write an HTML report for a completed flow run.
fn write_html_report(
    out_path: &str,
    source_path: &str,
    flow_name: &str,
    dispatch: &str,
    elapsed: std::time::Duration,
    result: &Value,
) -> Result<()> {
    let result_text = format!("{result}");

    // Parse the result into sections if it follows template output format (===SECTION===)
    let sections = parse_template_sections(&result_text);

    let source_name = std::path::Path::new(source_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| source_path.to_string());

    let elapsed_secs = elapsed.as_secs_f64();

    let mut sections_html = String::new();
    if sections.is_empty() {
        // Plain text result
        sections_html.push_str(&format!(
            r#"<div class="section"><div class="section-title">Result</div><div class="section-body"><pre>{}</pre></div></div>"#,
            html_escape(&result_text)
        ));
    } else {
        for (title, entries) in &sections {
            sections_html.push_str(&format!(
                r#"<div class="section"><div class="section-title">{}</div><div class="section-body">"#,
                html_escape(title)
            ));
            for entry in entries {
                let (label, value) = entry.split_once(": ").unwrap_or(("", entry));
                if label.is_empty() {
                    sections_html.push_str(&format!("<p>{}</p>", html_escape(value)));
                } else {
                    sections_html.push_str(&format!(
                        r#"<div class="entry"><span class="entry-label">{}</span> <span class="entry-value">{}</span></div>"#,
                        html_escape(label),
                        html_escape(value)
                    ));
                }
            }
            sections_html.push_str("</div></div>");
        }
    }

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>PACT Run Report — {flow_name}</title>
<style>
  :root {{
    --bg: #0d1117;
    --surface: #161b22;
    --border: #30363d;
    --text: #e6edf3;
    --text-muted: #8b949e;
    --accent: #58a6ff;
    --green: #3fb950;
    --orange: #d29922;
    --red: #f85149;
    --purple: #bc8cff;
  }}
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
    background: var(--bg);
    color: var(--text);
    line-height: 1.6;
    padding: 2rem;
  }}
  .container {{ max-width: 900px; margin: 0 auto; }}
  .header {{
    border-bottom: 1px solid var(--border);
    padding-bottom: 1.5rem;
    margin-bottom: 2rem;
  }}
  .header h1 {{
    font-size: 1.5rem;
    font-weight: 600;
    color: var(--accent);
    margin-bottom: 0.25rem;
  }}
  .header .subtitle {{
    color: var(--text-muted);
    font-size: 0.9rem;
  }}
  .meta {{
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 1rem;
    margin-bottom: 2rem;
  }}
  .meta-card {{
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 1rem;
  }}
  .meta-card .label {{
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
    margin-bottom: 0.25rem;
  }}
  .meta-card .value {{
    font-size: 1.1rem;
    font-weight: 600;
  }}
  .meta-card .value.dispatch {{ color: var(--purple); }}
  .meta-card .value.time {{ color: var(--green); }}
  .section {{
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    margin-bottom: 1.5rem;
    overflow: hidden;
  }}
  .section-title {{
    font-size: 0.85rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 0.75rem 1.25rem;
    background: rgba(88, 166, 255, 0.08);
    border-bottom: 1px solid var(--border);
    color: var(--accent);
  }}
  .section-body {{
    padding: 1.25rem;
  }}
  .section-body pre {{
    white-space: pre-wrap;
    word-wrap: break-word;
    font-family: "SF Mono", "Fira Code", monospace;
    font-size: 0.85rem;
    line-height: 1.7;
  }}
  .entry {{
    padding: 0.5rem 0;
    border-bottom: 1px solid var(--border);
    font-size: 0.9rem;
  }}
  .entry:last-child {{ border-bottom: none; }}
  .entry-label {{
    font-weight: 600;
    color: var(--orange);
    font-family: "SF Mono", "Fira Code", monospace;
    font-size: 0.8rem;
  }}
  .entry-value {{
    color: var(--text);
  }}
  .section-body p {{
    padding: 0.5rem 0;
    font-size: 0.9rem;
    line-height: 1.7;
  }}
  .footer {{
    margin-top: 2rem;
    padding-top: 1rem;
    border-top: 1px solid var(--border);
    color: var(--text-muted);
    font-size: 0.8rem;
    text-align: center;
  }}
</style>
</head>
<body>
<div class="container">
  <div class="header">
    <h1>PACT Run Report</h1>
    <div class="subtitle">Flow execution output for <code>{source_name}</code></div>
  </div>
  <div class="meta">
    <div class="meta-card">
      <div class="label">Flow</div>
      <div class="value">{flow_name}</div>
    </div>
    <div class="meta-card">
      <div class="label">Dispatch</div>
      <div class="value dispatch">{dispatch}</div>
    </div>
    <div class="meta-card">
      <div class="label">Duration</div>
      <div class="value time">{elapsed_secs:.1}s</div>
    </div>
    <div class="meta-card">
      <div class="label">Source</div>
      <div class="value">{source_name}</div>
    </div>
  </div>
  {sections_html}
  <div class="footer">
    Generated by PACT — Programmable Agent Contract Toolkit
  </div>
</div>
</body>
</html>"##,
    );

    fs::write(out_path, html)
        .into_diagnostic()
        .wrap_err_with(|| format!("failed to write report to '{out_path}'"))?;

    Ok(())
}

/// Parse template-style output into sections.
/// Recognizes `===SECTION_NAME===` headers followed by `KEY: value` lines.
fn parse_template_sections(text: &str) -> Vec<(String, Vec<String>)> {
    let mut sections = Vec::new();
    let mut current_title: Option<String> = None;
    let mut current_entries: Vec<String> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("===") && trimmed.ends_with("===") && trimmed.len() > 6 {
            // Flush previous section
            if let Some(title) = current_title.take() {
                sections.push((title, std::mem::take(&mut current_entries)));
            }
            let title = trimmed
                .trim_start_matches('=')
                .trim_end_matches('=')
                .trim()
                .to_string();
            current_title = Some(title);
        } else if !trimmed.is_empty() {
            if current_title.is_some() {
                current_entries.push(trimmed.to_string());
            } else {
                // Lines before any section header — create an implicit section
                current_title = Some("Output".to_string());
                current_entries.push(trimmed.to_string());
            }
        }
    }

    // Flush last section
    if let Some(title) = current_title {
        sections.push((title, current_entries));
    }

    sections
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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
    dispatch: &str,
) -> Result<()> {
    use pact_dispatch::StreamEvent;
    use std::io::Write;

    // Find the flow and its first agent dispatch
    let flow = program.decls.iter().find_map(|d| match &d.kind {
        DeclKind::Flow(f) if f.name == flow_name => Some(f),
        _ => None,
    });
    let flow = flow.ok_or_else(|| miette::miette!("flow '{}' not found", flow_name))?;

    let (agent_name, tool_name) = find_dispatch_in_exprs(&flow.body)
        .ok_or_else(|| miette::miette!("no agent dispatch found in flow '{}'", flow_name))?;

    let rt = tokio::runtime::Runtime::new()
        .into_diagnostic()
        .wrap_err("failed to create tokio runtime")?;

    match dispatch {
        "claude" => {
            // Full streaming tool loop with multi-turn support
            let agent_decl = program.decls.iter().find_map(|d| match &d.kind {
                DeclKind::Agent(a) if a.name == agent_name => Some(a),
                _ => None,
            });
            let agent_decl =
                agent_decl.ok_or_else(|| miette::miette!("agent '{}' not found", agent_name))?;

            let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamEvent>(256);

            rt.block_on(async {
                let client = pact_dispatch::client::AnthropicClient::from_env()
                    .map_err(|e| miette::miette!("{e}"))?;
                let tool_loop = pact_dispatch::tool_loop::ToolUseLoop::new(client);

                // Spawn the event printer on a separate task (rx is Send)
                let printer_handle = tokio::spawn(async move {
                    while let Some(event) = rx.recv().await {
                        match event {
                            StreamEvent::TextDelta(text) => {
                                eprint!("{}", text);
                                std::io::stderr().flush().ok();
                            }
                            StreamEvent::ToolUseStart { name, .. } => {
                                eprintln!("\n[stream] tool call: #{name}");
                            }
                            StreamEvent::ToolExecuting { name } => {
                                eprintln!("[stream] executing #{name}...");
                            }
                            StreamEvent::ToolResult { name, result } => {
                                let preview = if result.len() > 100 {
                                    format!("{}...", &result[..100])
                                } else {
                                    result
                                };
                                eprintln!("[stream] #{name} returned: {preview}");
                            }
                            StreamEvent::MessageDone { stop_reason } => {
                                eprintln!("\n[stream] done (stop_reason: {stop_reason:?})");
                            }
                            StreamEvent::Error(msg) => {
                                eprintln!("\n[stream] error: {msg}");
                            }
                            _ => {}
                        }
                    }
                });

                // Run the dispatch on the current task (ToolUseLoop is not Send)
                let result = tool_loop
                    .dispatch_stream(agent_decl, program, &tool_name, arg_values, tx)
                    .await;

                // Wait for the printer to finish
                let _ = printer_handle.await;

                result
                    .map(|_| ())
                    .map_err(|e| miette::miette!("stream dispatch failed: {e}"))
            })
        }
        "openai" => {
            let dispatcher =
                pact_dispatch::OpenAIDispatcher::from_env().map_err(|e| miette::miette!("{e}"))?;
            let prompt = format!(
                "You are agent @{agent_name}. Execute tool #{tool_name} with arguments: {:?}",
                arg_values
            );

            rt.block_on(async {
                let mut rx = dispatcher
                    .send_stream_events(&prompt)
                    .await
                    .map_err(|e| miette::miette!("{e}"))?;

                while let Some(event) = rx.recv().await {
                    match event {
                        StreamEvent::TextDelta(text) => {
                            eprint!("{}", text);
                            std::io::stderr().flush().ok();
                        }
                        StreamEvent::MessageDone { stop_reason } => {
                            eprintln!("\n[stream] done (stop_reason: {stop_reason:?})");
                        }
                        StreamEvent::Error(msg) => {
                            eprintln!("\n[stream] error: {msg}");
                        }
                        _ => {}
                    }
                }
                Ok(())
            })
        }
        "ollama" => {
            let dispatcher =
                pact_dispatch::OllamaDispatcher::from_env().map_err(|e| miette::miette!("{e}"))?;
            let prompt = format!(
                "You are agent @{agent_name}. Execute tool #{tool_name} with arguments: {:?}",
                arg_values
            );

            rt.block_on(async {
                let mut rx = dispatcher
                    .send_stream_events(&prompt)
                    .await
                    .map_err(|e| miette::miette!("{e}"))?;

                while let Some(event) = rx.recv().await {
                    match event {
                        StreamEvent::TextDelta(text) => {
                            eprint!("{}", text);
                            std::io::stderr().flush().ok();
                        }
                        StreamEvent::MessageDone { stop_reason } => {
                            eprintln!("\n[stream] done (stop_reason: {stop_reason:?})");
                        }
                        StreamEvent::Error(msg) => {
                            eprintln!("\n[stream] error: {msg}");
                        }
                        _ => {}
                    }
                }
                Ok(())
            })
        }
        _ => Err(miette::miette!(
            "--stream is not supported with --dispatch {dispatch}"
        )),
    }
}

/// Walk a slice of expressions to find the first `AgentDispatch`, returning
/// `(agent_name, tool_name)`. Handles both bare dispatches and `Let` bindings.
fn find_dispatch_in_exprs(exprs: &[pact_core::ast::expr::Expr]) -> Option<(String, String)> {
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
            DeclKind::Connect(_) => {}    // MCP connections
            DeclKind::Lesson(_) => {}     // Lessons are process memory
            DeclKind::Compliance(_) => {} // Compliance is governance metadata
            DeclKind::Federation(_) => {} // Federation is structural
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

/// `pact mcp list-tools <server> --file <file>` — list tools on an MCP server.
fn cmd_mcp_list_tools(server: &str, path: &str) -> Result<()> {
    let (program, _sm) = load_and_check(path)?;

    // Find the server in the connect block
    let mut found_transport = None;
    for decl in &program.decls {
        if let DeclKind::Connect(c) = &decl.kind {
            for entry in &c.servers {
                if entry.name == server {
                    found_transport = Some(entry.transport.clone());
                }
            }
        }
    }

    let transport = found_transport.ok_or_else(|| {
        miette::miette!(
            "MCP server '{}' not found in connect block of '{}'",
            server,
            path
        )
    })?;

    let command = if let Some(cmd) = transport.strip_prefix("stdio ") {
        cmd
    } else {
        miette::bail!(
            "only stdio transport is currently supported (got: {})",
            transport
        );
    };

    // Connect and list tools
    let rt = tokio::runtime::Runtime::new()
        .into_diagnostic()
        .wrap_err("failed to create tokio runtime")?;

    let tools = rt.block_on(async {
        let mut conn = pact_dispatch::mcp_client::McpConnection::connect_stdio(server, command)
            .await
            .map_err(|e| miette::miette!("{}", e))?;
        let tools = conn
            .list_tools()
            .await
            .map_err(|e| miette::miette!("{}", e))?;
        Ok::<Vec<pact_dispatch::mcp_client::McpToolInfo>, miette::Report>(tools.to_vec())
    })?;

    if tools.is_empty() {
        println!("No tools found on MCP server '{}'.", server);
    } else {
        println!("Tools on MCP server '{}':", server);
        println!("{:<30} DESCRIPTION", "NAME");
        println!("{}", "-".repeat(70));
        for tool in &tools {
            let desc = tool.description.as_deref().unwrap_or("-");
            println!("{:<30} {}", tool.name, desc);
        }
        println!("\n{} tool(s) total.", tools.len());
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
        match playground_load_file(
            path,
            &mut decls,
            &mut interpreter,
            &mut source_map,
            &mut input_counter,
        ) {
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
                    if handle_repl_command(
                        trimmed,
                        &mut decls,
                        &mut interpreter,
                        &mut source_map,
                        &mut input_counter,
                    ) {
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
fn read_multiline(
    rl: &mut rustyline::DefaultEditor,
    first_line: &str,
) -> std::result::Result<String, String> {
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
            DeclKind::Import(_) => {}     // Resolved by loader
            DeclKind::Connect(_) => {}    // MCP connections are structural
            DeclKind::Lesson(_) => {}     // Lessons are process memory
            DeclKind::Compliance(_) => {} // Compliance is governance metadata
            DeclKind::Federation(_) => {} // Federation is structural
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
    let full_program = Program {
        decls: decls.clone(),
    };
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
            let report =
                miette::Report::new(e).with_source_code(source_map.miette_source(source_id));
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

        let full_program = Program {
            decls: new_decls.clone(),
        };
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
                DeclKind::Connect(c) => {
                    let names: Vec<_> = c.servers.iter().map(|s| s.name.as_str()).collect();
                    println!("Defined connect block ({})", names.join(", "));
                }
                DeclKind::Lesson(l) => println!("Defined lesson \"{}\"", l.name),
                DeclKind::Compliance(c) => println!("Defined compliance \"{}\"", c.name),
                DeclKind::Federation(f) => {
                    println!("Defined federation with {} registries", f.registries.len())
                }
            }
        }

        *decls = new_decls;

        // Reload the interpreter.
        let full_program = Program {
            decls: decls.clone(),
        };
        *interpreter = Interpreter::with_dispatcher(Box::new(MockDispatcher));
        interpreter.load(&full_program);

        if count > 0 {
            return;
        }
    }

    // If declaration parsing failed, try wrapping as an expression inside a flow.
    let wrapper = format!("flow __repl__() -> String {{\n{}\n}}", input);

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

    let (wrapper_program, wrapper_parse_errors) =
        PactParser::new(&wrapper_tokens).parse_collecting_errors();
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

/// `pact install` — install dependencies from Pact.toml.
fn cmd_install() -> Result<()> {
    let cwd = std::env::current_dir().into_diagnostic()?;
    let manifest_dir = pact_registry::Manifest::find_upward(&cwd)
        .ok_or_else(|| miette::miette!("no Pact.toml found in current or parent directories"))?;

    let manifest =
        pact_registry::Manifest::load(&manifest_dir).map_err(|e| miette::miette!("{e}"))?;

    if manifest.dependencies.is_empty() {
        println!("No dependencies to install.");
        return Ok(());
    }

    println!("Installing {} dependencies...", manifest.dependencies.len());

    let rt = tokio::runtime::Runtime::new()
        .into_diagnostic()
        .wrap_err("failed to create tokio runtime")?;

    rt.block_on(async {
        let cache = pact_registry::Cache::new().map_err(|e| miette::miette!("{e}"))?;
        let resolver = pact_registry::Resolver::new(cache);

        // Check for existing lock file
        let lockfile_path = manifest_dir.join("pact.lock");
        let lockfile = if lockfile_path.exists() {
            println!("Using existing pact.lock");
            pact_registry::Lockfile::load(&lockfile_path).map_err(|e| miette::miette!("{e}"))?
        } else {
            println!("Resolving versions...");
            let lf = resolver
                .resolve(&manifest)
                .await
                .map_err(|e| miette::miette!("{e}"))?;
            lf.save(&lockfile_path)
                .map_err(|e| miette::miette!("{e}"))?;
            println!("Created pact.lock");
            lf
        };

        // Fetch packages into cache
        resolver
            .fetch(&lockfile)
            .await
            .map_err(|e| miette::miette!("{e}"))?;

        for pkg in &lockfile.packages {
            println!("  {} v{}", pkg.name, pkg.version);
        }
        println!("Done.");
        Ok(())
    })
}

/// `pact add <package>` — add a dependency to Pact.toml.
fn cmd_add(package: &str) -> Result<()> {
    let cwd = std::env::current_dir().into_diagnostic()?;
    let manifest_dir = pact_registry::Manifest::find_upward(&cwd)
        .ok_or_else(|| miette::miette!("no Pact.toml found in current or parent directories"))?;

    let mut manifest =
        pact_registry::Manifest::load(&manifest_dir).map_err(|e| miette::miette!("{e}"))?;

    // Parse package spec: "org/repo" or "org/repo@version"
    let (github, version) = if let Some(at_pos) = package.find('@') {
        (
            package[..at_pos].to_string(),
            package[at_pos + 1..].to_string(),
        )
    } else {
        (package.to_string(), "*".to_string())
    };

    // Extract short name from repo path
    let name = github.split('/').next_back().unwrap_or(&github).to_string();

    if manifest.dependencies.contains_key(&name) {
        println!("Dependency '{}' already exists, updating...", name);
    }

    manifest.dependencies.insert(
        name.clone(),
        pact_registry::manifest::Dependency {
            github: github.clone(),
            version: version.clone(),
        },
    );

    manifest
        .save(&manifest_dir)
        .map_err(|e| miette::miette!("{e}"))?;
    println!("Added {} ({}) @ {} to Pact.toml", name, github, version);

    // Run install
    cmd_install()
}

/// `pact search <query>` — search for PACT packages on GitHub.
fn cmd_search(query: &str) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()
        .into_diagnostic()
        .wrap_err("failed to create tokio runtime")?;

    rt.block_on(async {
        dotenvy::dotenv().ok();
        let token = std::env::var("GITHUB_TOKEN")
            .or_else(|_| std::env::var("GITHUB_ACCESS_TOKEN"))
            .ok();

        let client = reqwest::Client::builder()
            .user_agent("pact-registry/0.1")
            .build()
            .map_err(|e| miette::miette!("HTTP client error: {e}"))?;

        let url = format!(
            "https://api.github.com/search/repositories?q={}+topic:pact-lang&sort=stars&per_page=20",
            urlencoding::encode(query)
        );

        let mut req = client.get(&url);
        if let Some(t) = &token {
            req = req.header("Authorization", format!("Bearer {t}"));
        }

        let response = req
            .send()
            .await
            .map_err(|e| miette::miette!("search failed: {e}"))?;

        if !response.status().is_success() {
            return Err(miette::miette!(
                "GitHub API error: {}",
                response.status()
            ));
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| miette::miette!("failed to parse response: {e}"))?;

        let items = body["items"].as_array();
        match items {
            Some(repos) if !repos.is_empty() => {
                println!("Found {} packages:\n", repos.len());
                for repo in repos {
                    let name = repo["full_name"].as_str().unwrap_or("unknown");
                    let desc = repo["description"].as_str().unwrap_or("(no description)");
                    let stars = repo["stargazers_count"].as_u64().unwrap_or(0);
                    println!("  {} ({} stars)", name, stars);
                    println!("    {}", desc);
                    println!("    pact add {}\n", name);
                }
            }
            _ => {
                println!("No packages found matching '{}'.", query);
                println!(
                    "Tip: packages need the 'pact-lang' GitHub topic to appear in search."
                );
            }
        }
        Ok(())
    })
}

// ── Federation commands ─────────────────────────────────────────────────────

/// Extract federation registries from a loaded program.
fn extract_federation_registries(
    program: &pact_core::ast::stmt::Program,
) -> Vec<(String, Vec<String>)> {
    use pact_core::ast::expr::ExprKind;

    let mut registries = Vec::new();
    for decl in &program.decls {
        if let DeclKind::Federation(f) = &decl.kind {
            for entry in &f.registries {
                let perms: Vec<String> = entry
                    .trust
                    .iter()
                    .filter_map(|e| match &e.kind {
                        ExprKind::PermissionRef(segs) => Some(segs.join(".")),
                        _ => None,
                    })
                    .collect();
                registries.push((entry.url.clone(), perms));
            }
        }
    }
    registries
}

/// Discover remote agents from federation registries.
fn cmd_federation_discover(file: &str, query: Option<&str>) -> Result<()> {
    let (program, _sm) = load_and_check(file)?;
    let registries = extract_federation_registries(&program);

    if registries.is_empty() {
        println!("No federation registries declared in '{}'.", file);
        println!("Add a federation block:");
        println!();
        println!("  federation {{");
        println!("      \"https://registry.example.com\" trust: [^llm.query]");
        println!("  }}");
        return Ok(());
    }

    println!("Discovering agents from {} registries...", registries.len());
    println!();

    let rt = tokio::runtime::Runtime::new()
        .into_diagnostic()
        .wrap_err("failed to create runtime")?;

    rt.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .into_diagnostic()?;

        for (url, trust_perms) in &registries {
            println!("Registry: {}", url);
            println!(
                "  Trust: [{}]",
                trust_perms
                    .iter()
                    .map(|p| format!("^{p}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );

            let mut discover_url = format!("{url}/discover");
            if let Some(q) = query {
                discover_url = format!("{discover_url}?query={}", urlencoding::encode(q));
            }

            match client.get(&discover_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    match resp.json::<serde_json::Value>().await {
                        Ok(body) => {
                            if let Some(agents) = body.get("agents").and_then(|a| a.as_array()) {
                                if agents.is_empty() {
                                    println!("  No agents found.");
                                } else {
                                    for agent in agents {
                                        let name = agent
                                            .get("name")
                                            .and_then(|n| n.as_str())
                                            .unwrap_or("?");
                                        let desc = agent
                                            .get("description")
                                            .and_then(|d| d.as_str())
                                            .unwrap_or("");
                                        let endpoint = agent
                                            .get("endpoint")
                                            .and_then(|e| e.as_str())
                                            .unwrap_or("?");
                                        let perms: Vec<&str> = agent
                                            .get("permissions")
                                            .and_then(|p| p.as_array())
                                            .map(|arr| {
                                                arr.iter().filter_map(|v| v.as_str()).collect()
                                            })
                                            .unwrap_or_default();

                                        // Check trust boundary.
                                        let within_trust = perms.iter().all(|perm| {
                                            trust_perms.iter().any(|trusted| {
                                                *perm == trusted.as_str()
                                                    || perm.starts_with(&format!("{trusted}."))
                                            })
                                        });

                                        let trust_marker =
                                            if within_trust { "OK" } else { "UNTRUSTED" };
                                        println!("  @{name} [{trust_marker}]");
                                        if !desc.is_empty() {
                                            println!("    {desc}");
                                        }
                                        println!("    endpoint: {endpoint}");
                                        if !perms.is_empty() {
                                            println!(
                                                "    permits: [{}]",
                                                perms
                                                    .iter()
                                                    .map(|p| format!("^{p}"))
                                                    .collect::<Vec<_>>()
                                                    .join(", ")
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => println!("  Error parsing response: {e}"),
                    }
                }
                Ok(resp) => println!("  Error: HTTP {}", resp.status()),
                Err(e) => println!("  Unreachable: {e}"),
            }
            println!();
        }

        Ok(())
    })
}

/// Register local agents with a remote federation registry.
fn cmd_federation_register(file: &str, registry_url: &str) -> Result<()> {
    let (program, _sm) = load_and_check(file)?;

    use pact_core::ast::expr::ExprKind;

    let mut agents_registered = 0;

    let rt = tokio::runtime::Runtime::new()
        .into_diagnostic()
        .wrap_err("failed to create runtime")?;

    rt.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .into_diagnostic()?;

        for decl in &program.decls {
            if let DeclKind::Agent(a) = &decl.kind {
                let perms: Vec<String> = a
                    .permits
                    .iter()
                    .filter_map(|e| match &e.kind {
                        ExprKind::PermissionRef(segs) => Some(segs.join(".")),
                        _ => None,
                    })
                    .collect();

                let tools: Vec<serde_json::Value> = a
                    .tools
                    .iter()
                    .filter_map(|e| match &e.kind {
                        ExprKind::ToolRef(name) => Some(serde_json::json!({
                            "name": name,
                        })),
                        _ => None,
                    })
                    .collect();

                let description = a.prompt.as_ref().and_then(|p| match &p.kind {
                    ExprKind::PromptLit(s) => Some(s.clone()),
                    _ => None,
                });

                let endpoint = a.endpoint.clone().unwrap_or_default();

                let card = serde_json::json!({
                    "card": {
                        "name": a.name,
                        "description": description,
                        "endpoint": endpoint,
                        "permissions": perms,
                        "tools": tools,
                        "compliance": a.compliance,
                    }
                });

                let url = format!("{registry_url}/register");
                println!("Registering @{} with {}...", a.name, registry_url);

                match client.post(&url).json(&card).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        println!("  Registered @{}", a.name);
                        agents_registered += 1;
                    }
                    Ok(resp) => {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        println!("  Failed: HTTP {} — {}", status, body);
                    }
                    Err(e) => println!("  Failed: {}", e),
                }
            }
        }

        if agents_registered == 0 {
            println!("No agents found in '{}'.", file);
        } else {
            println!("\nRegistered {} agent(s).", agents_registered);
        }

        Ok(())
    })
}

/// Check health of federation registries declared in a .pact file.
fn cmd_federation_health(file: &str) -> Result<()> {
    let (program, _sm) = load_and_check(file)?;
    let registries = extract_federation_registries(&program);

    if registries.is_empty() {
        println!("No federation registries declared in '{}'.", file);
        return Ok(());
    }

    let rt = tokio::runtime::Runtime::new()
        .into_diagnostic()
        .wrap_err("failed to create runtime")?;

    rt.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .into_diagnostic()?;

        for (url, trust_perms) in &registries {
            print!("{url} ... ");

            let health_url = format!("{url}/health");
            match client.get(&health_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    match resp.json::<serde_json::Value>().await {
                        Ok(body) => {
                            let status = body.get("status").and_then(|s| s.as_str()).unwrap_or("unknown");
                            let version = body.get("version").and_then(|v| v.as_str()).unwrap_or("?");
                            let count = body.get("agent_count").and_then(|c| c.as_u64()).unwrap_or(0);
                            println!(
                                "OK (status={status}, version={version}, agents={count}, trust=[{}])",
                                trust_perms.iter().map(|p| format!("^{p}")).collect::<Vec<_>>().join(", ")
                            );
                        }
                        Err(e) => println!("ERROR parsing response: {e}"),
                    }
                }
                Ok(resp) => println!("ERROR HTTP {}", resp.status()),
                Err(e) => println!("UNREACHABLE: {e}"),
            }
        }

        Ok(())
    })
}
