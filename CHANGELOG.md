# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-03-11

### Added

- Core language with lexer, recursive-descent parser, type checker, and interpreter.
- First-class permission system (`^`) enforced at compile time.
- Templates for structured, typed output schemas.
- Directives for composable, parameterized prompt blocks.
- Declarative source providers replacing raw HTTP endpoints.
- Rich error handling with source-mapped diagnostics and recovery suggestions.
- Memory system (`~`) for agent conversation state.
- Build pipeline compiling `.pact` files to TOML configs, Markdown prompts, and Claude JSON schemas.
- Multi-backend dispatch supporting Claude, OpenAI, and Ollama.
- Streaming responses during agent execution.
- Language Server Protocol (LSP) implementation.
- VS Code extension with syntax highlighting and LSP integration.
- Code formatter (`pact fmt`).
- Documentation generator (`pact doc`).
- Interactive playground / REPL (`pact playground`).
- Mermaid diagram integration (`pact to-mermaid` / `pact from-mermaid`).
- 10 built-in source providers: DuckDuckGo, Google, Brave search; HTTP GET/POST; filesystem read/write/glob; time; JSON parse.
- Automatic guardrails for GDPR, CCPA, COPPA, PCI-DSS, and HIPAA based on declared permissions.
