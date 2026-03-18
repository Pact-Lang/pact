// Fuzz target for the PACT lexer.
//
// Feeds arbitrary byte sequences (converted to UTF-8 via lossy conversion) into
// the lexer. Lex errors are expected and fine — only panics indicate real bugs.

#![no_main]

use libfuzzer_sys::fuzz_target;
use pact_core::lexer::Lexer;
use pact_core::span::SourceMap;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);
    let mut sm = SourceMap::new();
    let id = sm.add("fuzz.pact", input.as_ref());

    // Lex errors are fine; panics are bugs.
    let _ = Lexer::new(sm.text(id), id).lex();
});
