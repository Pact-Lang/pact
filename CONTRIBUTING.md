# Contributing to PACT

Thanks for your interest in contributing to PACT! Here's how to get started.

## Development Setup

```bash
git clone https://github.com/Pact-Lang/pact.git
cd pact
cargo build
cargo test
```

## Project Structure

| Crate | Purpose |
|-------|---------|
| `pact-core` | Lexer, parser, AST, checker, interpreter, formatter |
| `pact-build` | Build pipeline: TOML, Markdown, Claude JSON, guardrails |
| `pact-dispatch` | Runtime: API clients, tool execution, retry, cache |
| `pact-cli` | CLI binary (`pact check`, `pact run`, `pact fmt`, etc.) |
| `pact-lsp` | Language Server Protocol for editor integration |
| `pact-mermaid` | Bidirectional Mermaid diagram conversion |
| `pact-mcp` | Model Context Protocol server |

## Before Submitting

1. Run `cargo fmt --all` to format code
2. Run `cargo clippy -- -D warnings` for lint checks
3. Run `cargo test` to verify all tests pass
4. Add tests for new functionality

## What to Contribute

- **Bug fixes** — always welcome
- **New examples** — `.pact` files demonstrating use cases
- **Documentation** — improvements to README, doc comments, or examples
- **New source providers** — expand the built-in provider registry
- **Editor support** — improvements to VS Code extension or new editor plugins
- **Language features** — discuss in an issue first before implementing

## Code Style

- Follow existing Rust conventions
- Keep functions focused and small
- Add doc comments for public APIs
- Use meaningful variable names — PACT code should be readable

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
