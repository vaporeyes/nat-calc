// ABOUTME: Error types for the dual-mode reduction engine.
// ABOUTME: Covers lexing, parsing, and evaluation/rewrite failures.

use std::fmt;

/// Failure modes for the lexer, parser, and reduction engine.
///
/// An *unbound* variable is deliberately not an error: it promotes the
/// expression to the symbolic (lazy) path instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalError {
    Lex(String),
    Parse(String),
    DivisionByZero,
    /// AST grew past `MAX_NODES` during a simplify/expand pass.
    ExpressionTooLarge,
    /// `simplify` did not reach a fixpoint within `MAX_REWRITE_DEPTH`.
    RewriteLimitExceeded,
    /// Operands did not fit the attempted operation (shape/kind mismatch).
    TypeMismatch(String),
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalError::Lex(m) => write!(f, "lex error: {m}"),
            EvalError::Parse(m) => write!(f, "parse error: {m}"),
            EvalError::DivisionByZero => write!(f, "division by zero"),
            EvalError::ExpressionTooLarge => {
                write!(f, "expression too large (exceeded MAX_NODES)")
            }
            EvalError::RewriteLimitExceeded => {
                write!(f, "rewrite limit exceeded (MAX_REWRITE_DEPTH)")
            }
            EvalError::TypeMismatch(m) => write!(f, "type mismatch: {m}"),
        }
    }
}

impl std::error::Error for EvalError {}

pub type EvalResultT<T> = Result<T, EvalError>;
