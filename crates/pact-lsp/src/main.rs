// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-01-20

//! PACT Language Server Protocol binary.
//!
//! Communicates over stdin/stdout using the LSP JSON-RPC protocol.

use tower_lsp::{LspService, Server};

mod backend;
mod symbol_index;

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(backend::PactBackend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
