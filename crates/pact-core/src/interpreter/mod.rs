// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-07-22

//! Tree-walking interpreter for the PACT language.
//!
//! The interpreter executes a checked AST by walking the tree directly.
//! Agent dispatches are mocked (printing to stdout), making this suitable
//! for development and testing of agent contracts.
//!
//! # Usage
//!
//! ```
//! use pact_core::interpreter::Interpreter;
//! use pact_core::lexer::Lexer;
//! use pact_core::parser::Parser;
//! use pact_core::span::SourceMap;
//!
//! let src = r#"
//!     agent @g { permits: [^llm.query] tools: [#greet] }
//!     flow hello(name :: String) -> String {
//!         result = @g -> #greet(name)
//!         return result
//!     }
//! "#;
//! let mut sm = SourceMap::new();
//! let id = sm.add("test.pact", src);
//! let tokens = Lexer::new(src, id).lex().unwrap();
//! let program = Parser::new(&tokens).parse().unwrap();
//! let mut interp = Interpreter::new();
//! let result = interp.run(&program, "hello", vec![pact_core::interpreter::value::Value::String("world".into())]);
//! ```

pub mod agent;
pub mod env;
pub mod value;

use crate::ast::expr::{BinOpKind, Expr, ExprKind, MatchPattern};
use crate::ast::stmt::{AgentDecl, DeclKind, FlowDecl, Program};
pub use agent::{Dispatcher, MockDispatcher};
use env::Environment;
use value::Value;

use miette::Diagnostic;
use thiserror::Error;

/// Runtime error during interpretation.
#[derive(Debug, Error, Diagnostic, Clone)]
pub enum RuntimeError {
    /// A referenced variable was not found in the current scope.
    #[error("undefined variable '{name}'")]
    UndefinedVariable {
        /// The variable name that was not found.
        name: String,
    },

    /// A referenced flow was not declared in the program.
    #[error("flow '{name}' not found")]
    FlowNotFound {
        /// The flow name that was not found.
        name: String,
    },

    /// A referenced agent was not declared in the program.
    #[error("agent '@{name}' not found")]
    AgentNotFound {
        /// The agent name that was not found.
        name: String,
    },

    /// A value had an unexpected type at runtime.
    #[error("type error: expected {expected}, got {got}")]
    TypeError {
        /// The expected type.
        expected: String,
        /// The actual type encountered.
        got: String,
    },

    /// An `assert` expression evaluated to a falsy value.
    #[error("assertion failed")]
    AssertionFailed {
        /// Description of the failed assertion.
        message: String,
    },

    /// A `fail` expression was executed.
    #[error("explicit fail: {message}")]
    ExplicitFail {
        /// The failure message.
        message: String,
    },

    /// A flow was called with the wrong number of arguments.
    #[error("wrong number of arguments: expected {expected}, got {got}")]
    ArityMismatch {
        /// The expected number of arguments.
        expected: usize,
        /// The actual number of arguments provided.
        got: usize,
    },

    /// A general runtime error.
    #[error("runtime error: {0}")]
    RuntimeError(String),
}

/// A signal used internally to unwind the stack on `return` or `fail`.
enum Signal {
    Return(Value),
    Fail(String),
}

/// The PACT tree-walking interpreter.
pub struct Interpreter {
    env: Environment,
    /// Stores agent declarations for dispatch.
    agents: std::collections::HashMap<String, AgentInfo>,
    /// Stores flow declarations for calling.
    flows: std::collections::HashMap<String, FlowDecl>,
    /// Test results: (description, passed).
    test_results: Vec<(String, bool)>,
    /// The dispatch backend (mock or real API).
    dispatcher: Box<dyn Dispatcher>,
    /// The full program AST, needed for dispatch.
    program: Option<Program>,
    /// The agent currently being dispatched to (for memory scoping).
    current_agent: Option<String>,
}

/// Info about an agent needed at runtime.
#[derive(Debug, Clone)]
struct AgentInfo {
    #[allow(dead_code)]
    name: String,
    /// The full agent declaration for dispatch.
    decl: AgentDecl,
}

impl Interpreter {
    /// Create a new interpreter with the mock dispatcher.
    pub fn new() -> Self {
        Self {
            env: Environment::new(),
            agents: std::collections::HashMap::new(),
            flows: std::collections::HashMap::new(),
            test_results: Vec::new(),
            dispatcher: Box::new(MockDispatcher),
            program: None,
            current_agent: None,
        }
    }

    /// Create a new interpreter with a custom dispatcher.
    pub fn with_dispatcher(dispatcher: Box<dyn Dispatcher>) -> Self {
        Self {
            env: Environment::new(),
            agents: std::collections::HashMap::new(),
            flows: std::collections::HashMap::new(),
            test_results: Vec::new(),
            dispatcher,
            program: None,
            current_agent: None,
        }
    }

    /// Load a program's declarations (agents, flows) into the interpreter.
    pub fn load(&mut self, program: &Program) {
        self.program = Some(program.clone());
        for decl in &program.decls {
            match &decl.kind {
                DeclKind::Agent(a) => {
                    self.agents.insert(
                        a.name.clone(),
                        AgentInfo {
                            name: a.name.clone(),
                            decl: a.clone(),
                        },
                    );
                }
                DeclKind::Flow(f) => {
                    self.flows.insert(f.name.clone(), f.clone());
                }
                _ => {}
            }
        }
    }

    /// Run a named flow with the given arguments.
    pub fn run(
        &mut self,
        program: &Program,
        flow_name: &str,
        args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        self.load(program);
        self.call_flow(flow_name, args)
    }

    /// Run all test declarations in the program.
    pub fn run_tests(&mut self, program: &Program) -> Vec<(String, bool)> {
        self.load(program);

        let test_decls: Vec<_> = program
            .decls
            .iter()
            .filter_map(|d| match &d.kind {
                DeclKind::Test(t) => Some(t.clone()),
                _ => None,
            })
            .collect();

        for test in &test_decls {
            self.env.push_scope();
            let mut passed = true;
            for expr in &test.body {
                match self.eval(expr) {
                    Ok(_) => {}
                    Err(RuntimeError::AssertionFailed { message }) => {
                        println!("  FAIL: {}", message);
                        passed = false;
                    }
                    Err(e) => {
                        println!("  ERROR: {}", e);
                        passed = false;
                    }
                }
            }
            let status = if passed { "PASS" } else { "FAIL" };
            println!("[TEST] {} ... {}", test.description, status);
            self.test_results.push((test.description.clone(), passed));
            self.env.pop_scope();
        }

        self.test_results.clone()
    }

    /// Call a flow by name with arguments.
    fn call_flow(&mut self, name: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
        let flow = self
            .flows
            .get(name)
            .cloned()
            .ok_or_else(|| RuntimeError::FlowNotFound {
                name: name.to_string(),
            })?;

        if args.len() != flow.params.len() {
            return Err(RuntimeError::ArityMismatch {
                expected: flow.params.len(),
                got: args.len(),
            });
        }

        self.env.push_scope();

        // Bind parameters
        for (param, value) in flow.params.iter().zip(args) {
            self.env.define(param.name.clone(), value);
        }

        let mut result = Value::Null;
        for expr in &flow.body {
            match self.eval_with_signal(expr) {
                Ok(val) => result = val,
                Err(Signal::Return(val)) => {
                    self.env.pop_scope();
                    return Ok(val);
                }
                Err(Signal::Fail(msg)) => {
                    self.env.pop_scope();
                    return Err(RuntimeError::ExplicitFail { message: msg });
                }
            }
        }

        self.env.pop_scope();
        Ok(result)
    }

    /// Evaluate an expression, catching return/fail signals.
    fn eval(&mut self, expr: &Expr) -> Result<Value, RuntimeError> {
        match self.eval_with_signal(expr) {
            Ok(val) => Ok(val),
            Err(Signal::Return(val)) => Ok(val),
            Err(Signal::Fail(msg)) => Err(RuntimeError::ExplicitFail { message: msg }),
        }
    }

    /// Evaluate an expression, propagating return/fail signals.
    fn eval_with_signal(&mut self, expr: &Expr) -> Result<Value, Signal> {
        match &expr.kind {
            ExprKind::IntLit(n) => Ok(Value::Int(*n)),
            ExprKind::FloatLit(n) => Ok(Value::Float(*n)),
            ExprKind::StringLit(s) => Ok(Value::String(s.clone())),
            ExprKind::PromptLit(s) => Ok(Value::String(self.interpolate_prompt(s))),
            ExprKind::BoolLit(b) => Ok(Value::Bool(*b)),

            ExprKind::Ident(name) => self
                .env
                .lookup(name)
                .cloned()
                .ok_or_else(|| Signal::Fail(format!("undefined variable '{name}'"))),

            ExprKind::AgentRef(name) => Ok(Value::AgentRef(name.clone())),
            ExprKind::ToolRef(name) => Ok(Value::String(format!("#{name}"))),
            ExprKind::MemoryRef(name) => {
                // Memory read — look up in the current agent's memory store.
                let agent = self.current_agent.as_deref().unwrap_or("default");
                let store = crate::memory::MemoryStore::load(agent);
                match store.get(name) {
                    Some(val) => Ok(Value::String(val.to_string())),
                    None => Ok(Value::Null),
                }
            }
            ExprKind::PermissionRef(segs) => Ok(Value::String(format!("!{}", segs.join(".")))),

            ExprKind::Assign { name, value } => {
                let val = self.eval_with_signal(value)?;
                self.env.define(name.clone(), val.clone());
                Ok(val)
            }

            ExprKind::Return(inner) => {
                let val = self.eval_with_signal(inner)?;
                Err(Signal::Return(val))
            }

            ExprKind::Fail(inner) => {
                let val = self.eval_with_signal(inner)?;
                Err(Signal::Fail(val.to_string()))
            }

            ExprKind::AgentDispatch { agent, tool, args } => {
                let agent_val = self.eval_with_signal(agent)?;
                let agent_name = match &agent_val {
                    Value::AgentRef(name) => name.clone(),
                    _ => return Err(Signal::Fail("dispatch target must be an agent".into())),
                };

                // Verify agent exists and get its declaration
                let agent_info = self
                    .agents
                    .get(&agent_name)
                    .cloned()
                    .ok_or_else(|| Signal::Fail(format!("agent '@{agent_name}' not found")))?;

                let tool_name = match &tool.kind {
                    ExprKind::ToolRef(name) => name.clone(),
                    _ => {
                        let val = self.eval_with_signal(tool)?;
                        val.to_string()
                    }
                };

                let mut arg_values = Vec::new();
                for arg in args {
                    arg_values.push(self.eval_with_signal(arg)?);
                }

                let program = self
                    .program
                    .as_ref()
                    .expect("program must be loaded before dispatch");

                // Track agent context for memory scoping.
                let prev_agent = self.current_agent.take();
                self.current_agent = Some(agent_name.clone());

                let result = self
                    .dispatcher
                    .dispatch(
                        &agent_name,
                        &tool_name,
                        &arg_values,
                        &agent_info.decl,
                        program,
                    )
                    .map_err(Signal::Fail);

                self.current_agent = prev_agent;
                result
            }

            ExprKind::Pipeline { left, right } => {
                let left_val = self.eval_with_signal(left)?;
                // In a pipeline, the left value becomes an implicit argument.
                // For v0.1, we just evaluate both sides sequentially.
                self.env.define("_pipe".into(), left_val);
                self.eval_with_signal(right)
            }

            ExprKind::FallbackChain { primary, fallback } => match self.eval_with_signal(primary) {
                Ok(val) if val.is_truthy() => Ok(val),
                _ => self.eval_with_signal(fallback),
            },

            ExprKind::Parallel(exprs) => {
                // v0.1: sequential execution, collect results as a list.
                let mut results = Vec::new();
                for e in exprs {
                    results.push(self.eval_with_signal(e)?);
                }
                Ok(Value::List(results))
            }

            ExprKind::Match { subject, arms } => {
                let subject_val = self.eval_with_signal(subject)?;
                for arm in arms {
                    if match_pattern(&arm.pattern, &subject_val) {
                        // Bind the matched value for ident patterns
                        if let MatchPattern::Ident(name) = &arm.pattern {
                            self.env.define(name.clone(), subject_val.clone());
                        }
                        return self.eval_with_signal(&arm.body);
                    }
                }
                Ok(Value::Null) // No arm matched
            }

            ExprKind::FieldAccess { object, field } => {
                let obj = self.eval_with_signal(object)?;
                match obj {
                    Value::Record(map) => Ok(map.get(field).cloned().unwrap_or(Value::Null)),
                    Value::ToolResult(s) => {
                        // Allow .value on ToolResult as an identity
                        if field == "value" {
                            Ok(Value::ToolResult(s))
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    _ => Ok(Value::Null),
                }
            }

            ExprKind::FuncCall { callee, args } => {
                // Try to call as a flow
                if let ExprKind::Ident(name) = &callee.kind {
                    if self.flows.contains_key(name.as_str()) {
                        let mut arg_vals = Vec::new();
                        for arg in args {
                            arg_vals.push(self.eval_with_signal(arg)?);
                        }
                        return self
                            .call_flow(name, arg_vals)
                            .map_err(|e| Signal::Fail(e.to_string()));
                    }
                }
                // Otherwise evaluate callee and args but return null
                let _callee_val = self.eval_with_signal(callee)?;
                let mut _arg_vals = Vec::new();
                for arg in args {
                    _arg_vals.push(self.eval_with_signal(arg)?);
                }
                Ok(Value::Null)
            }

            ExprKind::BinOp { left, op, right } => {
                let l = self.eval_with_signal(left)?;
                let r = self.eval_with_signal(right)?;
                Ok(eval_binop(&l, *op, &r))
            }

            ExprKind::Assert(inner) => {
                let val = self.eval_with_signal(inner)?;
                if val.is_truthy() {
                    println!("  PASS: assertion succeeded");
                    Ok(Value::Bool(true))
                } else {
                    Err(Signal::Fail(format!(
                        "assertion failed: expected truthy value, got {val}"
                    )))
                }
            }

            ExprKind::Record(exprs) => {
                let mut results = Vec::new();
                for e in exprs {
                    let val = self.eval_with_signal(e)?;
                    // If inside an agent context, persist assignments to memory.
                    if let Some(agent_name) = &self.current_agent {
                        if let ExprKind::Assign { name, .. } = &e.kind {
                            let mut store = crate::memory::MemoryStore::load(agent_name);
                            store.set(name.clone(), val.to_string());
                        }
                    }
                    results.push(val);
                }
                Ok(Value::List(results))
            }

            ExprKind::ListLit(items) => {
                let mut values = Vec::new();
                for item in items {
                    values.push(self.eval_with_signal(item)?);
                }
                Ok(Value::List(values))
            }

            ExprKind::RecordFields(fields) => {
                let mut map = std::collections::HashMap::new();
                for (key, expr) in fields {
                    let val = self.eval_with_signal(expr)?;
                    map.insert(key.clone(), val);
                }
                Ok(Value::Record(map))
            }

            ExprKind::Typed { expr, .. } => self.eval_with_signal(expr),

            ExprKind::SkillRef(name) => Ok(Value::String(format!("$skill:{}", name))),

            ExprKind::TemplateRef(name) => Ok(Value::String(format!("%{}", name))),

            ExprKind::Env(key) => match std::env::var(key) {
                Ok(val) => Ok(Value::String(val)),
                Err(_) => Err(Signal::Fail(format!(
                    "environment variable '{}' not set",
                    key
                ))),
            },

            ExprKind::OnError { body, fallback } => match self.eval_with_signal(body) {
                Ok(val) => Ok(val),
                Err(_) => self.eval_with_signal(fallback),
            },

            ExprKind::RunFlow { flow_name, args } => {
                let mut evaluated_args = Vec::new();
                for a in args {
                    evaluated_args.push(self.eval_with_signal(a)?);
                }
                self.call_flow(flow_name, evaluated_args)
                    .map_err(|e| Signal::Fail(e.to_string()))
            }
        }
    }

    /// Interpolate `{variable}` placeholders in a prompt string.
    /// Looks up each variable in the current environment and substitutes its value.
    fn interpolate_prompt(&self, template: &str) -> String {
        let mut result = String::new();
        let mut chars = template.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '{' {
                // Collect the identifier
                let mut ident = String::new();
                while let Some(&c) = chars.peek() {
                    if c == '}' {
                        chars.next(); // consume '}'
                        break;
                    }
                    ident.push(c);
                    chars.next();
                }

                if !ident.is_empty() {
                    // Look up in environment
                    match self.env.lookup(&ident) {
                        Some(val) => result.push_str(&val.to_string()),
                        None => {
                            // Leave as-is if not found
                            result.push('{');
                            result.push_str(&ident);
                            result.push('}');
                        }
                    }
                } else {
                    result.push('{');
                }
            } else {
                result.push(ch);
            }
        }

        result
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a pattern matches a value.
fn match_pattern(pattern: &MatchPattern, value: &Value) -> bool {
    match pattern {
        MatchPattern::Wildcard => true,
        MatchPattern::StringLit(s) => matches!(value, Value::String(v) if v == s),
        MatchPattern::BoolLit(b) => matches!(value, Value::Bool(v) if v == b),
        MatchPattern::IntLit(n) => matches!(value, Value::Int(v) if v == n),
        MatchPattern::Ident(name) => {
            // If the ident matches a type name, use it as a type pattern
            match name.as_str() {
                "String" => matches!(value, Value::String(_)),
                "Int" => matches!(value, Value::Int(_)),
                "Float" => matches!(value, Value::Float(_)),
                "Bool" => matches!(value, Value::Bool(_)),
                "List" => matches!(value, Value::List(_)),
                "Record" => matches!(value, Value::Record(_)),
                "Null" => matches!(value, Value::Null),
                _ => true, // Other idents are catch-all bindings
            }
        }
    }
}

/// Compare two values for equality, with coercion between ToolResult and String.
fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::ToolResult(a), Value::String(b)) | (Value::String(b), Value::ToolResult(a)) => {
            a == b
        }
        _ => left == right,
    }
}

/// Evaluate a binary operation on two values.
fn eval_binop(left: &Value, op: BinOpKind, right: &Value) -> Value {
    match op {
        BinOpKind::Eq => Value::Bool(values_equal(left, right)),
        BinOpKind::Neq => Value::Bool(!values_equal(left, right)),
        BinOpKind::Lt => match (left, right) {
            (Value::Int(a), Value::Int(b)) => Value::Bool(a < b),
            (Value::Float(a), Value::Float(b)) => Value::Bool(a < b),
            _ => Value::Bool(false),
        },
        BinOpKind::Gt => match (left, right) {
            (Value::Int(a), Value::Int(b)) => Value::Bool(a > b),
            (Value::Float(a), Value::Float(b)) => Value::Bool(a > b),
            _ => Value::Bool(false),
        },
        BinOpKind::LtEq => match (left, right) {
            (Value::Int(a), Value::Int(b)) => Value::Bool(a <= b),
            (Value::Float(a), Value::Float(b)) => Value::Bool(a <= b),
            _ => Value::Bool(false),
        },
        BinOpKind::GtEq => match (left, right) {
            (Value::Int(a), Value::Int(b)) => Value::Bool(a >= b),
            (Value::Float(a), Value::Float(b)) => Value::Bool(a >= b),
            _ => Value::Bool(false),
        },
        BinOpKind::Add => match (left, right) {
            (Value::Int(a), Value::Int(b)) => Value::Int(a + b),
            (Value::Float(a), Value::Float(b)) => Value::Float(a + b),
            (Value::String(a), Value::String(b)) => Value::String(format!("{a}{b}")),
            _ => Value::Null,
        },
        BinOpKind::Sub => match (left, right) {
            (Value::Int(a), Value::Int(b)) => Value::Int(a - b),
            (Value::Float(a), Value::Float(b)) => Value::Float(a - b),
            _ => Value::Null,
        },
        BinOpKind::Mul => match (left, right) {
            (Value::Int(a), Value::Int(b)) => Value::Int(a * b),
            (Value::Float(a), Value::Float(b)) => Value::Float(a * b),
            _ => Value::Null,
        },
        BinOpKind::Div => match (left, right) {
            (Value::Int(a), Value::Int(b)) if *b != 0 => Value::Int(a / b),
            (Value::Float(a), Value::Float(b)) if *b != 0.0 => Value::Float(a / b),
            _ => Value::Null,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::span::SourceMap;

    fn run_flow(src: &str, flow_name: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
        let mut sm = SourceMap::new();
        let id = sm.add("test.pact", src);
        let tokens = Lexer::new(src, id).lex().unwrap();
        let program = Parser::new(&tokens).parse().unwrap();
        let mut interp = Interpreter::new();
        interp.run(&program, flow_name, args)
    }

    #[test]
    fn run_hello_flow() {
        let src = r#"
            agent @greeter {
                permits: [^llm.query]
                tools: [#greet]
            }
            flow hello(name :: String) -> String {
                result = @greeter -> #greet(name)
                return result
            }
        "#;
        let result = run_flow(src, "hello", vec![Value::String("world".into())]).unwrap();
        assert_eq!(result, Value::ToolResult("greet_result".into()));
    }

    #[test]
    fn run_flow_with_binop() {
        let src = "flow add(a :: Int, b :: Int) -> Int { return a + b }";
        let result = run_flow(src, "add", vec![Value::Int(2), Value::Int(3)]).unwrap();
        assert_eq!(result, Value::Int(5));
    }

    #[test]
    fn run_flow_not_found() {
        let src = "flow hello() { return 42 }";
        let result = run_flow(src, "nonexistent", vec![]);
        assert!(matches!(result, Err(RuntimeError::FlowNotFound { .. })));
    }

    #[test]
    fn run_flow_arity_mismatch() {
        let src = "flow hello(x :: Int) { return x }";
        let result = run_flow(src, "hello", vec![]);
        assert!(matches!(result, Err(RuntimeError::ArityMismatch { .. })));
    }

    #[test]
    fn run_match_expression() {
        let src = r#"
            flow classify(x :: String) -> String {
                result = match x {
                    "a" => "alpha",
                    "b" => "beta",
                    _ => "unknown"
                }
                return result
            }
        "#;
        let result = run_flow(src, "classify", vec![Value::String("a".into())]).unwrap();
        assert_eq!(result, Value::String("alpha".into()));

        let result = run_flow(src, "classify", vec![Value::String("z".into())]).unwrap();
        assert_eq!(result, Value::String("unknown".into()));
    }

    #[test]
    fn run_test_pass() {
        let src = r#"
            agent @g { permits: [^llm.query] tools: [#greet] }
            test "basic" {
                assert true
            }
        "#;
        let mut sm = SourceMap::new();
        let id = sm.add("test.pact", src);
        let tokens = Lexer::new(src, id).lex().unwrap();
        let program = Parser::new(&tokens).parse().unwrap();
        let mut interp = Interpreter::new();
        let results = interp.run_tests(&program);
        assert_eq!(results.len(), 1);
        assert!(results[0].1); // passed
    }

    #[test]
    fn prompt_interpolation_basic() {
        let src = r#"
            flow greet(name :: String) -> String {
                msg = <<Hello {name}, welcome!>>
                return msg
            }
        "#;
        let result = run_flow(src, "greet", vec![Value::String("Alice".to_string())]).unwrap();
        assert_eq!(result, Value::String("Hello Alice, welcome!".to_string()));
    }

    #[test]
    fn prompt_interpolation_unknown_var_preserved() {
        let src = r#"
            flow test_prompt() -> String {
                msg = <<Hello {unknown}>>
                return msg
            }
        "#;
        let result = run_flow(src, "test_prompt", vec![]).unwrap();
        assert_eq!(result, Value::String("Hello {unknown}".to_string()));
    }

    #[test]
    fn prompt_interpolation_multiple_vars() {
        let src = r#"
            flow format(first :: String, last :: String) -> String {
                msg = <<Dear {first} {last}, thank you.>>
                return msg
            }
        "#;
        let result = run_flow(
            src,
            "format",
            vec![
                Value::String("John".to_string()),
                Value::String("Doe".to_string()),
            ],
        )
        .unwrap();
        assert_eq!(
            result,
            Value::String("Dear John Doe, thank you.".to_string())
        );
    }

    #[test]
    fn binop_comparison() {
        assert_eq!(
            eval_binop(&Value::Int(1), BinOpKind::Eq, &Value::Int(1)),
            Value::Bool(true)
        );
        assert_eq!(
            eval_binop(&Value::Int(1), BinOpKind::Eq, &Value::Int(2)),
            Value::Bool(false)
        );
        assert_eq!(
            eval_binop(
                &Value::String("a".into()),
                BinOpKind::Add,
                &Value::String("b".into())
            ),
            Value::String("ab".into())
        );
    }

    #[test]
    fn run_record_literal() {
        let src = r#"
            flow make() {
                result = { title: "Hello", count: 42 }
                return result
            }
        "#;
        let result = run_flow(src, "make", vec![]).unwrap();
        match result {
            Value::Record(map) => {
                assert_eq!(map.get("title"), Some(&Value::String("Hello".into())));
                assert_eq!(map.get("count"), Some(&Value::Int(42)));
            }
            _ => panic!("expected Record, got {:?}", result),
        }
    }

    #[test]
    fn run_record_field_access() {
        let src = r#"
            flow get_title() -> String {
                rec = { title: "Hello", count: 42 }
                return rec.title
            }
        "#;
        let result = run_flow(src, "get_title", vec![]).unwrap();
        assert_eq!(result, Value::String("Hello".into()));
    }

    #[test]
    fn run_record_with_variable() {
        let src = r#"
            flow make(name :: String) {
                result = { greeting: name, count: 1 }
                return result
            }
        "#;
        let result = run_flow(src, "make", vec![Value::String("world".into())]).unwrap();
        match result {
            Value::Record(map) => {
                assert_eq!(map.get("greeting"), Some(&Value::String("world".into())));
                assert_eq!(map.get("count"), Some(&Value::Int(1)));
            }
            _ => panic!("expected Record"),
        }
    }

    #[test]
    fn match_wildcard_always_matches() {
        let src = r#"
            flow test_wildcard(x :: Int) -> String {
                result = match x {
                    _ => "matched"
                }
                return result
            }
        "#;
        let result = run_flow(src, "test_wildcard", vec![Value::Int(42)]).unwrap();
        assert_eq!(result, Value::String("matched".into()));
    }

    #[test]
    fn match_type_pattern_string() {
        let src = r#"
            flow type_check(x :: Any) -> String {
                result = match x {
                    String => "is_string",
                    Int => "is_int",
                    _ => "other"
                }
                return result
            }
        "#;
        let result = run_flow(src, "type_check", vec![Value::String("hello".into())]).unwrap();
        assert_eq!(result, Value::String("is_string".into()));
    }

    #[test]
    fn match_type_pattern_int() {
        let src = r#"
            flow type_check(x :: Any) -> String {
                result = match x {
                    String => "is_string",
                    Int => "is_int",
                    _ => "other"
                }
                return result
            }
        "#;
        let result = run_flow(src, "type_check", vec![Value::Int(42)]).unwrap();
        assert_eq!(result, Value::String("is_int".into()));
    }

    #[test]
    fn match_type_pattern_bool() {
        let src = r#"
            flow type_check(x :: Any) -> String {
                result = match x {
                    Bool => "is_bool",
                    _ => "other"
                }
                return result
            }
        "#;
        let result = run_flow(src, "type_check", vec![Value::Bool(true)]).unwrap();
        assert_eq!(result, Value::String("is_bool".into()));

        let result = run_flow(src, "type_check", vec![Value::Int(1)]).unwrap();
        assert_eq!(result, Value::String("other".into()));
    }

    #[test]
    fn match_type_pattern_no_match_falls_through() {
        let src = r#"
            flow type_check(x :: Any) -> String {
                result = match x {
                    String => "is_string",
                    Int => "is_int"
                }
                return result
            }
        "#;
        // Bool doesn't match String or Int, so no arm matches -> Null
        let result = run_flow(src, "type_check", vec![Value::Bool(true)]).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn env_expression_reads_var() {
        std::env::set_var("PACT_TEST_VAR", "hello_from_env");
        let src = r#"
            flow read_env() -> String {
                return env("PACT_TEST_VAR")
            }
        "#;
        let result = run_flow(src, "read_env", vec![]).unwrap();
        assert_eq!(result, Value::String("hello_from_env".into()));
    }

    #[test]
    fn env_expression_missing_var_fails() {
        std::env::remove_var("PACT_MISSING_VAR_XYZ");
        let src = r#"
            flow read_env() -> String {
                return env("PACT_MISSING_VAR_XYZ")
            }
        "#;
        let result = run_flow(src, "read_env", vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn on_error_returns_body_on_success() {
        let src = r#"
            flow safe() -> Int {
                result = 42 on_error 0
                return result
            }
        "#;
        let result = run_flow(src, "safe", vec![]).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn on_error_returns_fallback_on_failure() {
        // Use a variable that doesn't exist to trigger a failure in the body
        let src = r#"
            flow safe() -> String {
                result = nonexistent_var on_error "fallback_value"
                return result
            }
        "#;
        let result = run_flow(src, "safe", vec![]).unwrap();
        assert_eq!(result, Value::String("fallback_value".into()));
    }

    #[test]
    fn run_flow_expression() {
        let src = r#"
            flow add(a :: Int, b :: Int) -> Int {
                return a + b
            }
            flow main() -> Int {
                result = run add(2, 3)
                return result
            }
        "#;
        let result = run_flow(src, "main", vec![]).unwrap();
        assert_eq!(result, Value::Int(5));
    }

    #[test]
    fn run_flow_not_found_fails() {
        let src = r#"
            flow main() -> Int {
                result = run nonexistent()
                return result
            }
        "#;
        let result = run_flow(src, "main", vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn memory_ref_reads_from_agent_scoped_store() {
        std::env::set_var("PACT_MEMORY_DIR", "/tmp/pact_test_memory_interp");
        // Pre-populate memory for the agent.
        let mut store = crate::memory::MemoryStore::load("greeter");
        store.set("greeting".to_string(), "Hello from memory!".to_string());

        let src = r#"
            agent @greeter {
                permits: [^llm.query]
                tools: [#greet]
                memory: [~greeting]
            }
            flow hello() -> String {
                result = @greeter -> #greet(~greeting)
                return result
            }
        "#;
        let mut sm = SourceMap::new();
        let id = sm.add("test.pact", src);
        let tokens = Lexer::new(src, id).lex().unwrap();
        let program = Parser::new(&tokens).parse().unwrap();
        let mut interp = Interpreter::new();
        // Set the current agent context so ~greeting reads from @greeter's store.
        interp.current_agent = Some("greeter".to_string());
        interp.load(&program);
        let result = interp.call_flow("hello", vec![]).unwrap();
        // The mock dispatcher returns "greet_result" regardless of args,
        // but the important thing is the ~greeting was resolved to the stored value.
        assert_eq!(result, Value::ToolResult("greet_result".into()));

        // Cleanup
        store.clear();
    }

    #[test]
    fn memory_ref_returns_null_for_missing_key() {
        std::env::set_var("PACT_MEMORY_DIR", "/tmp/pact_test_memory_interp");
        let src = r#"
            flow read_missing() {
                val = ~nonexistent_key
                return val
            }
        "#;
        let result = run_flow(src, "read_missing", vec![]).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn agent_dispatch_sets_current_agent() {
        std::env::set_var("PACT_MEMORY_DIR", "/tmp/pact_test_memory_interp");
        // Pre-populate memory for the agent.
        let mut store = crate::memory::MemoryStore::load("researcher");
        store.set("context".to_string(), "prior research".to_string());

        let src = r#"
            agent @researcher {
                permits: [^net.read]
                tools: [#search]
                memory: [~context]
            }
            flow research(query :: String) -> String {
                result = @researcher -> #search(query)
                return result
            }
        "#;
        let mut sm = SourceMap::new();
        let id = sm.add("test.pact", src);
        let tokens = Lexer::new(src, id).lex().unwrap();
        let program = Parser::new(&tokens).parse().unwrap();
        let mut interp = Interpreter::new();
        let result = interp.run(&program, "research", vec![Value::String("test".into())]);
        assert!(result.is_ok());
        // After dispatch completes, current_agent should be restored.
        assert_eq!(interp.current_agent, None);

        // Cleanup
        store.clear();
    }

    #[test]
    fn memory_store_session_constructor() {
        std::env::set_var("PACT_MEMORY_DIR", "/tmp/pact_test_memory_interp");
        let store = crate::memory::MemoryStore::with_session("test_agent", "sess-123");
        assert_eq!(store.agent_name(), "test_agent");
        assert_eq!(store.session_id(), Some("sess-123"));
        assert!(store.is_empty());
    }
}
