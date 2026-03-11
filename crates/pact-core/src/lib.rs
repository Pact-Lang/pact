// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-03-16

//! # pact-core
//!
//! Core library for the **PACT** (Programmable Agent Contract Toolkit) language.
//!
//! This crate provides the complete pipeline for processing `.pact` files:
//!
//! - **Lexer** — tokenizes source text into a flat token stream
//! - **Parser** — builds an Abstract Syntax Tree (AST) via recursive descent
//! - **Checker** — performs semantic analysis (types, permissions, name resolution)
//! - **Interpreter** — tree-walking execution with mock agent dispatch
//!
//! ## Architecture
//!
//! ```text
//! Source text → Lexer → Tokens → Parser → AST → Checker → AST → Interpreter → Output
//! ```
//!
//! The `pact-core` crate is a library; the `pact-cli` crate provides the
//! command-line interface (`pact check`, `pact run`).

pub mod ast;
pub mod checker;
pub mod doc;
pub mod formatter;
pub mod interpreter;
pub mod lexer;
pub mod loader;
pub mod memory;
pub mod parser;
pub mod span;
pub mod template;
