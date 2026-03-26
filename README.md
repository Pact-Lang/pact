# PACT -- Programmable Agent Contract Toolkit

> A typed, permission-enforced language for AI agent orchestration, multi-agent systems, and LLM tool calling -- with built-in compliance, agent-to-agent discovery, and multi-target builds.

PACT is a **language**, not a library. Where frameworks bolt safety onto Python after the fact, PACT encodes permissions, types, and agent contracts directly into its syntax. Every tool declares what it needs. Every agent declares what it may do. The compiler enforces the rest -- before a single API call is made. Write once, deploy to **Claude, OpenAI, CrewAI, Cursor, and Gemini**. The result: AI agent systems you can reason about, audit, and trust.

**Key capabilities:** compile-time permission enforcement | typed agent contracts | compliance declarations (GDPR, HIPAA, PCI-DSS, SOX) | multi-target build (5 backends) | agent cards for A2A discovery | MCP server | LSP + VS Code | Mermaid diagram export

## Why PACT?

| | PACT | LangChain | CrewAI | AutoGen |
|---|---|---|---|---|
| Language vs Library | Language | Python lib | Python lib | Python lib |
| Type safety | Built-in (`::`) | None | None | None |
| Permission system | First-class (`^`) | Manual | None | None |
| Agent contracts | Enforced at compile-time | Runtime only | Runtime only | Runtime only |
| Compliance declarations | Built-in (risk tiers, audit, SOD) | Manual | None | None |
| Multi-target build | Claude, OpenAI, CrewAI, Cursor, Gemini | N/A | N/A | N/A |
| Agent discovery (A2A) | Agent cards JSON | None | None | None |
| Composable prompts | Templates + Directives | String concatenation | String templates | String templates |
| Source providers | Declarative (`source:`) | Raw HTTP calls | Manual | Manual |
| Auto-guardrails | GDPR, HIPAA, PCI-DSS, SOX | Manual | None | None |
| Tooling | LSP, VS Code, formatter, MCP | IDE plugins | None | None |

## Quick Start

```bash
# Install
cargo install pact-lang

# Scaffold a new project
pact init my_agent.pact

# Type-check a contract
pact check examples/hello_agent.pact

# Build for a specific target
pact build app.pact --target openai

# Run with Claude
export ANTHROPIC_API_KEY=sk-...
pact run examples/website_builder.pact --flow build_bilingual_site \
  --args "coffee shop in Uppsala, Sweden" --dispatch claude

# Format code
pact fmt examples/hello_agent.pact --write

# Generate docs
pact doc examples/website_builder.pact -o docs.md

# Interactive playground
pact playground --load examples/research_flow.pact
```

## The Language at a Glance

Here is a complete, working PACT program -- a research agent that searches the web, summarizes findings, and drafts a report:

```pact
permit_tree {
    ^net  { ^net.read }
    ^llm  { ^llm.query }
}

tool #web_search {
    description: <<Search the web for a query.>>
    requires: [^net.read]
    source: ^search.duckduckgo(query)
    params { query :: String }
    returns :: List<String>
}

tool #summarize {
    description: <<Condense content into key points.>>
    requires: [^llm.query]
    params { content :: String }
    returns :: String
}

agent @researcher {
    permits: [^net.read, ^llm.query]
    tools: [#web_search, #summarize]
    model: "claude-sonnet-4-20250514"
    prompt: <<You are a thorough research assistant.>>
}

flow research(topic :: String) -> String {
    results = @researcher -> #web_search(topic)
    summary = @researcher -> #summarize(results)
    return summary
}
```

Every symbol in PACT carries meaning through its sigil.

### Sigils -- Everything has a Symbol

| Sigil | Meaning | Example |
|-------|---------|---------|
| `@` | Agent | `@researcher` |
| `#` | Tool | `#web_search` |
| `$` | Skill | `$summarize` |
| `~` | Memory | `~conversation` |
| `^` | Permission | `^net.read` |
| `%` | Template / Directive | `%report_format` |

### Permissions -- Security by Default

Permissions are not an afterthought. They are part of the grammar. If an agent uses a tool it lacks permission for, the compiler rejects the program:

```pact
agent @writer {
    permits: [^llm.query]          -- only LLM access
    tools: [#save_to_disk]         -- tries to use a file tool
}

tool #save_to_disk {
    requires: [^fs.write]          -- needs filesystem write
    params { content :: String }
    returns :: String
}
```

```
  x agent '@writer' uses tool '#save_to_disk' which requires permission 'fs.write',
  | but the agent does not have it
   ,---[contract.pact:3:13]
 3 |     tools: [#save_to_disk]
   .             ------+------
   .                   `-- tool used here
   `----
  help: add '^fs.write' to the agent's permits list
```

This is caught at **compile time** -- before any API call, before any file is touched.

### Compliance Declarations

PACT supports first-class compliance declarations for regulatory frameworks. Attach a compliance block to any agent to enforce risk tiers, audit levels, separation of duties, and data retention policies:

```pact
compliance "payment_processing" {
    risk: high
    frameworks: [pci_dss, gdpr, sox]
    audit: full
    retention: "7y"
    review_interval: "90d"
    roles {
        approver: "finance_lead"
        executor: "payment_agent"
        auditor: "compliance_team"
    }
}

agent @payment_processor {
    permits: [^pay.charge]
    tools: [#process_payment]
    compliance: "payment_processing"
}
```

**Risk tiers** classify agents by operational risk: `low`, `medium`, `high`, `critical`. Higher tiers trigger stricter mediation at runtime.

**Audit levels** control logging granularity: `none` (no audit trail), `summary` (outcome logging), `full` (every tool call, input, and output recorded).

**Separation of duties (SOD)** prevents a single agent or user from holding conflicting roles. The `roles` block defines `approver`, `executor`, and `auditor` -- the checker enforces that no single identity fills more than one role.

**Regulatory frameworks** (`pci_dss`, `gdpr`, `sox`, `hipaa`, `coppa`, `ccpa`) activate domain-specific guardrails at build time. Combine multiple frameworks in a single declaration.

### Templates -- Structured Output

Templates define reusable output schemas that tools must conform to:

```pact
template %website_copy {
    HERO_TAGLINE :: String      <<one powerful headline>>
    HERO_SUBTITLE :: String     <<one compelling subtitle>>
    ABOUT :: String             <<two paragraphs about the business>>
    MENU_ITEM :: String * 6     <<Name | Price | Description>>
}

tool #write_copy {
    description: <<Write marketing copy for a website.>>
    requires: [^llm.query]
    output: %website_copy
    params { brief :: String }
    returns :: String
}
```

### Directives -- Composable Prompts

Directives are reusable prompt blocks with typed parameters. Attach them to tools to compose complex behavior from small, testable pieces:

```pact
directive %scandinavian_design {
    <<Use Google Fonts ({heading_font} for headings, {body_font} for body).
    Rich color palette matching a Scandinavian brand.>>
    params {
        heading_font :: String = "Playfair Display"
        body_font :: String = "Inter"
    }
}

tool #generate_html {
    description: <<Generate a one-page HTML website.>>
    requires: [^llm.query]
    directives: [%scandinavian_design, %scroll_animations]
    params { content :: String }
    returns :: String
}
```

### Source Providers -- No More Raw URLs

Instead of embedding HTTP endpoints in handler strings, declare what a tool needs and let PACT resolve it:

```pact
-- Before: fragile, hardcoded URL
tool #search {
    handler: "http GET https://api.duckduckgo.com/?q={query}&format=json"
    ...
}

-- After: declarative source provider
tool #search {
    source: ^search.duckduckgo(query)
    ...
}
```

Providers are built-in, tested, and carry their own permission requirements. Swap `^search.duckduckgo` for `^search.brave` without changing anything else.

### Flows -- Multi-Agent Orchestration

Flows chain agents together with dispatch (`->`), pipelines (`|>`), and fallbacks (`?>`):

```pact
flow build_bilingual_site(request :: String) -> String {
    -- Chain agents with dispatch
    research = @researcher -> #research_location(request)
    english  = @researcher -> #write_copy(research)
    swedish  = @translator -> #translate_to_swedish(english)
    html     = @designer   -> #generate_html(swedish)
    return html
}

flow safe_search(query :: String) -> String {
    -- Fallback: if primary fails, try backup
    result = @researcher -> #web_search(query) ?> @writer -> #draft_report(query)
    return result
}
```

## Multi-Target Build

PACT compiles to multiple backend formats from a single source file. Write your agent contracts once and deploy to any supported platform:

```bash
pact build app.pact --target claude    # Anthropic Claude tool_use JSON
pact build app.pact --target openai    # OpenAI function calling JSON
pact build app.pact --target crewai    # CrewAI YAML (agents + tasks)
pact build app.pact --target cursor    # .cursorrules + .cursor/mcp.json
pact build app.pact --target gemini    # Google Gemini function declarations
```

Each target generates idiomatic output for its platform -- Claude `tool_use` blocks, OpenAI `functions` arrays, CrewAI agent/task YAML, Cursor rules with MCP configuration, or Gemini function declarations. The permission model, types, and compliance posture carry through to every target.

## Agent Cards -- A2A Discovery

Generate Agent Card JSON for agent-to-agent (A2A) discovery and interoperability:

```bash
pact build app.pact --agent-cards
```

Each agent in your program produces a structured JSON card describing its capabilities:

```json
{
  "version": "1.0",
  "agent": {
    "name": "researcher",
    "description": "A thorough research assistant.",
    "model": "claude-sonnet-4-20250514",
    "capabilities": {
      "permissions": ["net.read", "llm.query"],
      "tools": [
        { "name": "web_search", "description": "Search the web for a query.", "parameters": { "query": "string" } }
      ],
      "skills": []
    },
    "compliance": {}
  },
  "flows": [
    { "name": "research", "params": [{"name": "topic", "type": "String"}], "return_type": "String" }
  ],
  "metadata": { "generated_by": "pact-build", "source": "app.pact" }
}
```

Agent cards enable automated discovery in multi-agent systems -- other agents and orchestrators can query what an agent can do, what permissions it holds, and what compliance posture it maintains.

## MCP Server

PACT includes a built-in Model Context Protocol (MCP) server, allowing AI agents and tools to interact with PACT programs over both stdio and SSE transports:

```bash
# Start MCP server (stdio transport, for editor integrations)
pact-mcp --stdio

# Start MCP server (SSE transport, for networked agents)
pact-mcp --sse --port 3000
```

The MCP server exposes PACT's type checking, build, and documentation capabilities as MCP tools, enabling any MCP-compatible client to work with `.pact` files programmatically.

## Examples

| File | Description |
|------|-------------|
| `hello_agent.pact` | Minimal agent with a single tool and flow |
| `research_flow.pact` | Multi-agent research with fallback chains |
| `website_builder.pact` | Bilingual website generator with templates, directives, and source providers |
| `age_verified_website.pact` | Age-gated content with compliance guardrails |

Run any example:

```bash
pact check examples/hello_agent.pact
pact run examples/hello_agent.pact --flow hello --args "world" --dispatch claude
```

## Automatic Guardrails

PACT detects compliance domains from your permission declarations and injects security boundaries into agent prompts at build time -- no manual boilerplate:

| Domain | Trigger | Standards |
|--------|---------|-----------|
| Personal Data | `^data.read`, `^data.write` | GDPR, CCPA |
| Age Verification | `^user.age_check` | COPPA |
| Financial Data | `^payment.read`, `^payment.write` | PCI-DSS |
| Health Data | `^health.read`, `^health.write` | HIPAA |
| Credentials | `^auth.read`, `^auth.write` | Secret masking |

Write 10 lines of PACT. Get production-grade guardrails for free.

## Architecture

```
┌─────────────┐
│  .pact file  │
└──────┬──────┘
       │
  ┌────▼────┐
  │  Lexer   │   Tokenizes sigils, keywords, <<prompt>> literals
  └────┬────┘
  ┌────▼────┐
  │  Parser  │   Recursive descent with error recovery
  └────┬────┘
  ┌────▼────┐
  │ Checker  │   Types, permissions, compliance, template/directive validation
  └────┬────┘
  ┌────▼────┐
  │  Build   │   Multi-target: Claude / OpenAI / CrewAI / Cursor / Gemini
  └────┬────┘
  ┌────▼────┐
  │Dispatch  │   Tool execution, retry, compliance mediation
  └─────────┘
```

The checker runs two passes: name collection, then validation. Permission violations, type mismatches, compliance role conflicts, and undefined references are all caught before execution. The dispatcher supports mock mode for development and real API dispatch for Claude, OpenAI, and Ollama.

## Editor Support

### VS Code

The `pact-lang` extension provides syntax highlighting and LSP integration:

```bash
cd editors/vscode
npm install && npm run compile
# Install via "Extensions: Install from VSIX" in VS Code
```

Configure the LSP path in settings if needed:

```json
{ "pact.lspPath": "/path/to/pact-lsp" }
```

## CLI Reference

| Command | Description |
|---------|-------------|
| `pact init [file]` | Scaffold a new `.pact` project file |
| `pact check <file>` | Type-check and validate permissions |
| `pact build <file> [--target T]` | Compile to target format (claude, openai, crewai, cursor, gemini) |
| `pact build <file> --agent-cards` | Generate Agent Card JSON for A2A discovery |
| `pact run <file> --flow <name>` | Execute a flow (add `--dispatch claude` for real API) |
| `pact test <file>` | Run all `test` declarations |
| `pact fmt <file> [--write]` | Format a `.pact` file |
| `pact doc <file> [-o file]` | Generate Markdown documentation |
| `pact playground [--load file]` | Interactive REPL |
| `pact list [skills\|prompts\|all]` | List built-in skills and templates |
| `pact to-mermaid <file>` | Export flow as an agentflow Mermaid diagram |
| `pact from-mermaid <file>` | Import a Mermaid diagram as PACT |

## Built-in Providers

| Provider | Permission | Description |
|----------|-----------|-------------|
| `^search.duckduckgo` | `^net.read` | Web search via DuckDuckGo |
| `^search.google` | `^net.read` | Web search via Google Custom Search |
| `^search.brave` | `^net.read` | Web search via Brave Search |
| `^http.get` | `^net.read` | HTTP GET request |
| `^http.post` | `^net.write` | HTTP POST with JSON body |
| `^fs.read` | `^fs.read` | Read file contents |
| `^fs.write` | `^fs.write` | Write file contents |
| `^fs.glob` | `^fs.read` | Find files by glob pattern |
| `^time.now` | `^time.read` | Current timestamp |
| `^json.parse` | `^json.parse` | Parse and validate JSON |

## Suggested Repository Topics

For GitHub discoverability, add these topics to the repository:

`pact-lang` `ai-agents` `agent-orchestration` `llm-tool-calling` `multi-agent-systems` `agent-permissions` `compliance-framework` `openai-agents` `crewai` `cursor-rules` `gemini` `claude` `mcp-server` `agent-to-agent` `a2a-protocol` `type-safe` `programming-language` `rust`

## Roadmap

- [x] Core language -- lexer, parser, checker, interpreter
- [x] Build system -- multi-target compilation (Claude, OpenAI, CrewAI, Cursor, Gemini)
- [x] Real dispatch -- Claude API with tool-use conversation loop
- [x] Runtime mediation -- compliance validation on every tool call
- [x] Developer tooling -- formatter, doc generator, playground, LSP
- [x] Compliance declarations -- risk tiers, audit levels, SOD roles, regulatory frameworks
- [x] Agent cards -- A2A discovery JSON
- [x] MCP server -- stdio and SSE transports
- [ ] Package registry -- share and reuse templates, directives, tools
- [ ] Streaming responses -- real-time agent output
- [ ] WASM module -- run PACT in the browser and embed in other tools
- [ ] Visual editor -- drag-and-drop flow builder

## License

MIT -- Copyright (c) 2025-2026 Gabriel Lars Sabadin
