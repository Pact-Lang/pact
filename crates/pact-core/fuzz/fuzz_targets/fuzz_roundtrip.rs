// Fuzz target for the full PACT front-end pipeline: lex, parse, and check.
//
// Feeds arbitrary byte sequences through the entire front-end. Errors at any
// stage are expected and fine — only panics indicate real bugs.

#![no_main]

use libfuzzer_sys::fuzz_target;
use pact_core::checker::Checker;
use pact_core::lexer::Lexer;
use pact_core::parser::Parser;
use pact_core::span::SourceMap;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);
    let mut sm = SourceMap::new();
    let id = sm.add("fuzz.pact", input.as_ref());

    let tokens = match Lexer::new(sm.text(id), id).lex() {
        Ok(tokens) => tokens,
        Err(_) => return,
    };

    let program = match Parser::new(&tokens).parse() {
        Ok(program) => program,
        Err(_) => return,
    };

    // Check errors are fine; panics are bugs.
    let _ = Checker::new().check(&program);
});
