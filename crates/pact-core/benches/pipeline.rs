// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Benchmarks for the PACT compiler pipeline.
//!
//! Measures the performance of each stage individually and end-to-end:
//!
//! - **Lexer** — tokenizes the `website_builder.pact` example.
//! - **Parser** — parses a pre-lexed token stream into an AST.
//! - **Checker** — runs semantic analysis on a parsed program.
//! - **Interpreter** — executes a simple arithmetic flow (no agent dispatch).
//! - **Full pipeline** — lex → parse → check on `website_builder.pact`.
//! - **Small input** — lex + parse a minimal single-agent declaration.
//!
//! Run with: `cargo bench -p pact-core`

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

use pact_core::checker::Checker;
use pact_core::interpreter::value::Value;
use pact_core::interpreter::Interpreter;
use pact_core::lexer::Lexer;
use pact_core::parser::Parser;
use pact_core::span::SourceMap;

/// The full `website_builder.pact` example, included at compile time.
const WEBSITE_BUILDER_SRC: &str = include_str!("../../../examples/website_builder.pact");

/// A self-contained program for interpreter benchmarking (no agent dispatch).
const INTERPRETER_SRC: &str = "flow add(a :: Int, b :: Int) -> Int { return a + b }";

/// A minimal agent declaration for small-input benchmarking.
const SMALL_SRC: &str = "agent @greeter { permits: [^llm.query] tools: [#greet] }";

/// Benchmark: lex the website_builder.pact source into tokens.
fn bench_lexer(c: &mut Criterion) {
    let mut sm = SourceMap::new();
    let id = sm.add("website_builder.pact", WEBSITE_BUILDER_SRC);

    c.bench_function("lexer/website_builder", |b| {
        b.iter(|| {
            let lexer = Lexer::new(black_box(WEBSITE_BUILDER_SRC), id);
            lexer.lex().unwrap()
        });
    });
}

/// Benchmark: parse a pre-lexed token stream from website_builder.pact.
fn bench_parser(c: &mut Criterion) {
    let mut sm = SourceMap::new();
    let id = sm.add("website_builder.pact", WEBSITE_BUILDER_SRC);
    let tokens = Lexer::new(WEBSITE_BUILDER_SRC, id).lex().unwrap();

    c.bench_function("parser/website_builder", |b| {
        b.iter(|| {
            let mut parser = Parser::new(black_box(&tokens));
            parser.parse().unwrap()
        });
    });
}

/// Benchmark: run the checker on a parsed website_builder.pact program.
fn bench_checker(c: &mut Criterion) {
    let mut sm = SourceMap::new();
    let id = sm.add("website_builder.pact", WEBSITE_BUILDER_SRC);
    let tokens = Lexer::new(WEBSITE_BUILDER_SRC, id).lex().unwrap();
    let program = Parser::new(&tokens).parse().unwrap();

    c.bench_function("checker/website_builder", |b| {
        b.iter(|| {
            let checker = Checker::new();
            checker.check(black_box(&program))
        });
    });
}

/// Benchmark: interpret a simple arithmetic flow (no agent dispatch).
fn bench_interpreter(c: &mut Criterion) {
    let mut sm = SourceMap::new();
    let id = sm.add("bench.pact", INTERPRETER_SRC);
    let tokens = Lexer::new(INTERPRETER_SRC, id).lex().unwrap();
    let program = Parser::new(&tokens).parse().unwrap();

    c.bench_function("interpreter/add_flow", |b| {
        b.iter(|| {
            let mut interp = Interpreter::new();
            interp
                .run(
                    black_box(&program),
                    "add",
                    vec![Value::Int(2), Value::Int(3)],
                )
                .unwrap()
        });
    });
}

/// Benchmark: full pipeline (lex → parse → check) on website_builder.pact.
fn bench_full_pipeline(c: &mut Criterion) {
    let mut sm = SourceMap::new();
    let id = sm.add("website_builder.pact", WEBSITE_BUILDER_SRC);

    c.bench_function("pipeline/lex_parse_check", |b| {
        b.iter(|| {
            let tokens = Lexer::new(black_box(WEBSITE_BUILDER_SRC), id)
                .lex()
                .unwrap();
            let program = Parser::new(&tokens).parse().unwrap();
            Checker::new().check(&program)
        });
    });
}

/// Benchmark: lex + parse a minimal agent declaration.
fn bench_small_input(c: &mut Criterion) {
    let mut sm = SourceMap::new();
    let id = sm.add("small.pact", SMALL_SRC);

    c.bench_function("pipeline/small_agent", |b| {
        b.iter(|| {
            let tokens = Lexer::new(black_box(SMALL_SRC), id).lex().unwrap();
            Parser::new(&tokens).parse().unwrap()
        });
    });
}

criterion_group!(
    benches,
    bench_lexer,
    bench_parser,
    bench_checker,
    bench_interpreter,
    bench_full_pipeline,
    bench_small_input,
);
criterion_main!(benches);
