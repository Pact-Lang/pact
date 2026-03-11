// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-01-10

pub mod convert;
pub mod emit;
pub mod parser;

pub use convert::graph_to_pact;
pub use emit::pact_to_mermaid;
pub use parser::{parse_mermaid, MermaidError, MermaidGraph};

/// Parse a Mermaid diagram and generate PACT source.
pub fn mermaid_to_pact(input: &str) -> Result<String, MermaidError> {
    let graph = parse_mermaid(input)?;
    Ok(graph_to_pact(&graph))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_to_end_mermaid_to_pact() {
        let input = r#"
flowchart LR
    A(Search Web) -->|results| B{Researcher}
    B -->|summary| C(Summarize)
    C -->|report| D{Writer}
    D --> E(Draft Report)
"#;
        let pact = mermaid_to_pact(input).unwrap();

        // Should contain tool declarations for rounded nodes.
        assert!(pact.contains("tool #search_web"));
        assert!(pact.contains("tool #summarize"));
        assert!(pact.contains("tool #draft_report"));

        // Should contain agent declarations for diamond nodes.
        assert!(pact.contains("agent @researcher"));
        assert!(pact.contains("agent @writer"));

        // Should contain a flow.
        assert!(pact.contains("flow main(input :: String)"));

        // Should contain the permit_tree block.
        assert!(pact.contains("permit_tree"));
        assert!(pact.contains("^llm.query"));

        // Should have the header comment.
        assert!(pact.contains("Auto-generated from Mermaid"));
    }
}
