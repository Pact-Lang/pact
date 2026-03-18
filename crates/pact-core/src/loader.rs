// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-12-28

//! Multi-file import loader for the PACT language.
//!
//! The loader resolves `import "path/to/file.pact"` declarations by reading
//! imported files, parsing them, recursively resolving their imports, and
//! merging all declarations into a single [`Program`].
//!
//! # Circular Import Prevention
//!
//! The loader tracks which files have already been loaded (by canonical path).
//! If a file has already been loaded, it is silently skipped, preventing
//! infinite loops from circular imports.
//!
//! # Usage
//!
//! ```no_run
//! use pact_core::loader::Loader;
//! use pact_core::span::SourceMap;
//! use std::path::Path;
//!
//! let mut source_map = SourceMap::new();
//! let mut loader = Loader::new();
//! let program = loader.load(Path::new("main.pact"), &mut source_map).unwrap();
//! ```

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::ast::stmt::{DeclKind, Program};
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::span::SourceMap;

/// Error produced during file loading.
#[derive(Debug, Clone)]
pub enum LoadError {
    /// An I/O error occurred while reading a file.
    Io {
        /// The file path that caused the error.
        path: PathBuf,
        /// The error message.
        message: String,
    },
    /// A lexer error occurred in the specified file.
    Lex {
        /// The file path that caused the error.
        path: PathBuf,
        /// The error message.
        message: String,
    },
    /// A parser error occurred in the specified file.
    Parse {
        /// The file path that caused the error.
        path: PathBuf,
        /// The error message.
        message: String,
    },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io { path, message } => {
                write!(f, "error reading '{}': {}", path.display(), message)
            }
            LoadError::Lex { path, message } => {
                write!(f, "lex error in '{}': {}", path.display(), message)
            }
            LoadError::Parse { path, message } => {
                write!(f, "parse error in '{}': {}", path.display(), message)
            }
        }
    }
}

/// Multi-file import loader.
///
/// Resolves `import` declarations by reading, parsing, and merging imported
/// files into a single program. Tracks loaded files to prevent circular imports.
pub struct Loader {
    /// Canonical paths of files that have already been loaded.
    loaded: HashSet<PathBuf>,
}

impl Loader {
    /// Create a new loader with no files loaded.
    pub fn new() -> Self {
        Self {
            loaded: HashSet::new(),
        }
    }

    /// Load a `.pact` file, recursively resolving all imports.
    ///
    /// Import paths are resolved relative to the directory containing the
    /// importing file. Files that have already been loaded are silently
    /// skipped to prevent circular imports.
    ///
    /// Returns a merged [`Program`] containing all declarations from the
    /// entry file and all transitively imported files.
    pub fn load(
        &mut self,
        path: &Path,
        source_map: &mut SourceMap,
    ) -> Result<Program, Vec<LoadError>> {
        let mut errors = Vec::new();
        let mut all_decls = Vec::new();

        self.load_recursive(path, source_map, &mut all_decls, &mut errors);

        if !errors.is_empty() {
            return Err(errors);
        }

        Ok(Program { decls: all_decls })
    }

    /// Internal recursive loader.
    fn load_recursive(
        &mut self,
        path: &Path,
        source_map: &mut SourceMap,
        all_decls: &mut Vec<crate::ast::stmt::Decl>,
        errors: &mut Vec<LoadError>,
    ) {
        // Canonicalize the path to detect circular imports reliably.
        let canonical = match std::fs::canonicalize(path) {
            Ok(p) => p,
            Err(e) => {
                errors.push(LoadError::Io {
                    path: path.to_path_buf(),
                    message: e.to_string(),
                });
                return;
            }
        };

        // Skip if already loaded (circular import prevention).
        if self.loaded.contains(&canonical) {
            return;
        }
        self.loaded.insert(canonical.clone());

        // Read the file.
        let source = match std::fs::read_to_string(&canonical) {
            Ok(s) => s,
            Err(e) => {
                errors.push(LoadError::Io {
                    path: path.to_path_buf(),
                    message: e.to_string(),
                });
                return;
            }
        };

        // Lex.
        let file_name = path.display().to_string();
        let source_id = source_map.add(&file_name, &source);
        let tokens = match Lexer::new(source_map.text(source_id), source_id).lex() {
            Ok(t) => t,
            Err(e) => {
                errors.push(LoadError::Lex {
                    path: path.to_path_buf(),
                    message: e.to_string(),
                });
                return;
            }
        };

        // Parse.
        let program = match Parser::new(&tokens).parse() {
            Ok(p) => p,
            Err(e) => {
                errors.push(LoadError::Parse {
                    path: path.to_path_buf(),
                    message: e.to_string(),
                });
                return;
            }
        };

        // The directory containing this file, used to resolve relative imports.
        let base_dir = canonical.parent().unwrap_or_else(|| Path::new("."));

        // Process declarations: resolve imports first, then collect non-import decls.
        for decl in program.decls {
            match &decl.kind {
                DeclKind::Import(import) => {
                    let import_path = base_dir.join(&import.path);
                    self.load_recursive(&import_path, source_map, all_decls, errors);
                }
                _ => {
                    all_decls.push(decl);
                }
            }
        }
    }
}

impl Default for Loader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Create a temp directory with test files and return its path.
    fn create_test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pact-loader-test-{}", name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn basic_import() {
        let dir = create_test_dir("basic");

        // Create the imported file.
        fs::write(
            dir.join("agents.pact"),
            "agent @helper { permits: [^llm.query] tools: [] }",
        )
        .unwrap();

        // Create the main file that imports agents.pact.
        fs::write(
            dir.join("main.pact"),
            r#"import "agents.pact"

flow hello() {
    return 42
}"#,
        )
        .unwrap();

        let mut sm = SourceMap::new();
        let mut loader = Loader::new();
        let program = loader.load(&dir.join("main.pact"), &mut sm).unwrap();

        // Should have 2 declarations: the agent from agents.pact and the flow from main.pact.
        assert_eq!(program.decls.len(), 2);
        assert!(matches!(&program.decls[0].kind, DeclKind::Agent(_)));
        assert!(matches!(&program.decls[1].kind, DeclKind::Flow(_)));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn circular_import_does_not_loop() {
        let dir = create_test_dir("circular");

        // a.pact imports b.pact, b.pact imports a.pact.
        fs::write(
            dir.join("a.pact"),
            r#"import "b.pact"
agent @a { permits: [] tools: [] }"#,
        )
        .unwrap();

        fs::write(
            dir.join("b.pact"),
            r#"import "a.pact"
agent @b { permits: [] tools: [] }"#,
        )
        .unwrap();

        let mut sm = SourceMap::new();
        let mut loader = Loader::new();
        let program = loader.load(&dir.join("a.pact"), &mut sm).unwrap();

        // Should have 2 agents without infinite looping.
        assert_eq!(program.decls.len(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn imported_declarations_available() {
        let dir = create_test_dir("available");

        // Create a file with a tool and an agent.
        fs::write(
            dir.join("defs.pact"),
            r#"tool #greet {
    description: <<Greet someone>>
    requires: [^llm.query]
    params { name :: String }
    returns :: String
}

agent @greeter {
    permits: [^llm.query]
    tools: [#greet]
}"#,
        )
        .unwrap();

        // Main file imports defs and defines a flow using them.
        fs::write(
            dir.join("main.pact"),
            r#"import "defs.pact"

flow hello(name :: String) -> String {
    result = @greeter -> #greet(name)
    return result
}"#,
        )
        .unwrap();

        let mut sm = SourceMap::new();
        let mut loader = Loader::new();
        let program = loader.load(&dir.join("main.pact"), &mut sm).unwrap();

        // Should have 3 declarations: tool, agent, flow.
        assert_eq!(program.decls.len(), 3);
        assert!(matches!(&program.decls[0].kind, DeclKind::Tool(_)));
        assert!(matches!(&program.decls[1].kind, DeclKind::Agent(_)));
        assert!(matches!(&program.decls[2].kind, DeclKind::Flow(_)));

        // The merged program should pass the checker.
        let errors = crate::checker::Checker::new().check(&program);
        assert!(
            errors.is_empty(),
            "expected no check errors, got: {:?}",
            errors
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_import_file_returns_error() {
        let dir = create_test_dir("missing");

        fs::write(
            dir.join("main.pact"),
            r#"import "nonexistent.pact"

agent @a { permits: [] tools: [] }"#,
        )
        .unwrap();

        let mut sm = SourceMap::new();
        let mut loader = Loader::new();
        let result = loader.load(&dir.join("main.pact"), &mut sm);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(&errors[0], LoadError::Io { .. }));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn nested_imports() {
        let dir = create_test_dir("nested");
        let sub = dir.join("sub");
        fs::create_dir_all(&sub).unwrap();

        // sub/types.pact defines a schema.
        fs::write(sub.join("types.pact"), "schema Report { title :: String }").unwrap();

        // defs.pact imports sub/types.pact and defines an agent.
        fs::write(
            dir.join("defs.pact"),
            r#"import "sub/types.pact"
agent @a { permits: [] tools: [] }"#,
        )
        .unwrap();

        // main.pact imports defs.pact.
        fs::write(
            dir.join("main.pact"),
            r#"import "defs.pact"
flow main() { return 1 }"#,
        )
        .unwrap();

        let mut sm = SourceMap::new();
        let mut loader = Loader::new();
        let program = loader.load(&dir.join("main.pact"), &mut sm).unwrap();

        // schema + agent + flow = 3.
        assert_eq!(program.decls.len(), 3);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_import_loaded_once() {
        let dir = create_test_dir("dedup");

        // shared.pact defines a tool.
        fs::write(
            dir.join("shared.pact"),
            r#"tool #shared_tool {
    description: <<Shared>>
    requires: []
    params {}
}"#,
        )
        .unwrap();

        // Both a.pact and b.pact import shared.pact.
        fs::write(
            dir.join("a.pact"),
            r#"import "shared.pact"
agent @a { permits: [] tools: [] }"#,
        )
        .unwrap();

        fs::write(
            dir.join("b.pact"),
            r#"import "shared.pact"
agent @b { permits: [] tools: [] }"#,
        )
        .unwrap();

        // main.pact imports both a.pact and b.pact.
        fs::write(
            dir.join("main.pact"),
            r#"import "a.pact"
import "b.pact"
flow main() { return 1 }"#,
        )
        .unwrap();

        let mut sm = SourceMap::new();
        let mut loader = Loader::new();
        let program = loader.load(&dir.join("main.pact"), &mut sm).unwrap();

        // shared_tool should appear only once: tool + agent_a + agent_b + flow = 4.
        assert_eq!(program.decls.len(), 4);

        // Count tools — should be exactly 1.
        let tool_count = program
            .decls
            .iter()
            .filter(|d| matches!(&d.kind, DeclKind::Tool(_)))
            .count();
        assert_eq!(tool_count, 1);

        let _ = fs::remove_dir_all(&dir);
    }
}
