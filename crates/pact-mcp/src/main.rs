// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-01-15

//! PACT MCP server — exposes PACT tools over the Model Context Protocol.

/// MCP JSON-RPC protocol types and message handling.
mod protocol;
/// MCP server lifecycle and request routing.
mod server;
/// Tool registration and execution for the MCP server.
mod tools;

/// Entry point for the PACT MCP server.
fn main() {
    server::run_server();
}
