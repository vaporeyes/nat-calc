// ABOUTME: Crate root wiring the dual-mode reduction engine modules together.
// ABOUTME: Public surface: parse, Environment, reduce, EvalResult.

pub mod ast;
pub mod config;
pub mod derive;
pub mod engine;
pub mod error;
pub mod expand;
pub mod lambda;
pub mod lexer;
pub mod logic;
pub mod numeric;
pub mod parser;
pub mod simplify;

pub use engine::{Environment, EvalResult, reduce};
pub use error::EvalError;
pub use parser::parse;

/// Parse and reduce one line of source against `env`.
pub fn eval(src: &str, env: &mut Environment) -> Result<EvalResult, EvalError> {
    let expr = parse(src)?;
    reduce(&expr, env)
}
