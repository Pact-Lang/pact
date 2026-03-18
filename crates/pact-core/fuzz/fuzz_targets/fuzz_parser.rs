// Fuzz target for the PACT parser.
//
// Feeds arbitrary byte sequences through the lexer and, if lexing succeeds,
// through the parser. Both lex errors and parse errors are expected and fine —
// only panics indicate real bugs.

#![no_main]

use libfuzzer_sys::fuzz_target;
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

    // Parse errors are fine; panics are bugs.
    let _ = Parser::new(&tokens).parse();
});
