# PACT -- Programmable Agent Contract Toolkit

> A typed, permission-enforced language for orchestrating AI agents.

PACT is a **language**, not a library. Where frameworks bolt safety onto Python after the fact, PACT encodes permissions, types, and agent contracts directly into its syntax. Every tool declares what it needs. Every agent declares what it may do. The compiler enforces the rest -- before a single API call is made. The result: AI agent systems you can reason about, audit, and trust.

## Why PACT?

| | PACT | LangChain | CrewAI | AutoGen |
|---|---|---|---|---|
| Language vs Library | Language | Python lib | Python lib | Python lib |
| Type safety | Built-in (`::`) | None | None | None |
| Permission system | First-class (`^`) | Manual | None | None |
| Agent contracts | Enforced at compile-time | Runtime only | Runtime only | Runtime only |
| Composable prompts | Templates + Directives | String concatenation | String templates | String templates |
| Source providers | Declarative (`source:`) | Raw HTTP calls | Manual | Manual |
| Auto-guardrails | GDPR, HIPAA, PCI-DSS | Manual | None | None |
| Multi-backend | Claude, OpenAI, Ollama | Many | OpenAI | Many |
| Tooling | LSP, VS Code, formatter | IDE plugins | None | None |

## Quick Start

```bash
# Install
cargo install pact-lang

# Scaffold a new project
pact init my_agent.pact

# Type-check a contract
pact check examples/hello_agent.pact

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
  │ Checker  │   Types, permissions, template/directive validation
  └────┬────┘
  ┌────▼────┐
  │  Build   │   TOML configs, Markdown prompts, Claude JSON schemas
  └────┬────┘
  ┌────▼────┐
  │Dispatch  │   Tool execution, retry, compliance mediation
  └─────────┘
```

The checker runs two passes: name collection, then validation. Permission violations, type mismatches, and undefined references are all caught before execution. The dispatcher supports mock mode for development and real API dispatch for Claude, OpenAI, and Ollama.

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
| `pact build <file> [--out-dir dir]` | Compile to TOML, Markdown, and Claude JSON |
| `pact run <file> --flow <name>` | Execute a flow (add `--dispatch claude` for real API) |
| `pact test <file>` | Run all `test` declarations |
| `pact fmt <file> [--write]` | Format a `.pact` file |
| `pact doc <file> [-o file]` | Generate Markdown documentation |
| `pact playground [--load file]` | Interactive REPL |
| `pact list [skills\|prompts\|all]` | List built-in skills and templates |
| `pact to-mermaid <file>` | Export flow as a Mermaid diagram |
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

## Roadmap

- [x] Core language -- lexer, parser, checker, interpreter
- [x] Build system -- TOML, Markdown, Claude JSON compilation
- [x] Real dispatch -- Claude API with tool-use conversation loop
- [x] Runtime mediation -- compliance validation on every tool call
- [x] Developer tooling -- formatter, doc generator, playground, LSP
- [ ] Package registry -- share and reuse templates, directives, tools
- [ ] Streaming responses -- real-time agent output
- [ ] WASM compilation -- run PACT in the browser
- [ ] Visual editor -- drag-and-drop flow builder
- [ ] MCP integration -- Model Context Protocol support

## License

MIT -- Copyright (c) 2025-2026 Gabriel Lars Sabadin
