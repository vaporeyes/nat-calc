// ABOUTME: The expression AST shared by every stage of the engine.
// ABOUTME: An enum (closed variant set) per Rust best practice, no trait objects.

use bigdecimal::BigDecimal;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

impl BinaryOp {
    pub fn symbol(self) -> char {
        match self {
            BinaryOp::Add => '+',
            BinaryOp::Sub => '-',
            BinaryOp::Mul => '*',
            BinaryOp::Div => '/',
            BinaryOp::Pow => '^',
        }
    }
}

/// Transcendental functions kept exact: they reduce symbolically rather
/// than to a lossy floating-point approximation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Func {
    Sin,
    Cos,
    Tan,
    Exp,
    Ln,
}

impl Func {
    pub fn name(self) -> &'static str {
        match self {
            Func::Sin => "sin",
            Func::Cos => "cos",
            Func::Tan => "tan",
            Func::Exp => "exp",
            Func::Ln => "ln",
        }
    }
}

/// The unevaluated expression tree. The `Environment` stores variables as
/// `Expr` values (not primitives), which is what makes lazy evaluation and
/// delayed binding possible.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Expr {
    Number(BigDecimal),
    Variable(String),
    /// Row-major matrix. Elements are `Expr` for structural uniformity; the
    /// engine requires them to reduce to numbers.
    Matrix(Vec<Vec<Expr>>),
    Neg(Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Call(Func, Box<Expr>),
    /// `\param. body` — single-parameter abstraction. Multi-parameter
    /// `\x y. e` is desugared by the parser to nested lambdas.
    Lambda(String, Box<Expr>),
    /// Function application `func arg` (parsed from `func(arg)`).
    Apply(Box<Expr>, Box<Expr>),
    Assign(String, Box<Expr>),
    // Explicit context-switch wrappers (Sub-Task 2).
    Simplify(Box<Expr>),
    Expand(Box<Expr>),
    Derive(String, Box<Expr>),
}

impl Expr {
    pub fn num(n: impl Into<BigDecimal>) -> Expr {
        Expr::Number(n.into())
    }

    pub fn var(name: impl Into<String>) -> Expr {
        Expr::Variable(name.into())
    }

    pub fn binary(op: BinaryOp, l: Expr, r: Expr) -> Expr {
        Expr::Binary(op, Box::new(l), Box::new(r))
    }

    pub fn negate(e: Expr) -> Expr {
        Expr::Neg(Box::new(e))
    }

    pub fn call(f: Func, arg: Expr) -> Expr {
        Expr::Call(f, Box::new(arg))
    }

    /// Total node count, used to enforce `MAX_NODES` during rewrites.
    pub fn node_count(&self) -> usize {
        match self {
            Expr::Number(_) | Expr::Variable(_) => 1,
            Expr::Matrix(rows) => {
                1 + rows.iter().flatten().map(Expr::node_count).sum::<usize>()
            }
            Expr::Neg(e)
            | Expr::Call(_, e)
            | Expr::Simplify(e)
            | Expr::Expand(e)
            | Expr::Assign(_, e)
            | Expr::Derive(_, e)
            | Expr::Lambda(_, e) => 1 + e.node_count(),
            Expr::Binary(_, l, r) | Expr::Apply(l, r) => {
                1 + l.node_count() + r.node_count()
            }
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Number(n) => write!(f, "{}", n.normalized()),
            Expr::Variable(v) => write!(f, "{v}"),
            Expr::Matrix(rows) => {
                write!(f, "[")?;
                for (i, row) in rows.iter().enumerate() {
                    if i > 0 {
                        write!(f, "; ")?;
                    }
                    for (j, el) in row.iter().enumerate() {
                        if j > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{el}")?;
                    }
                }
                write!(f, "]")
            }
            Expr::Neg(e) => write!(f, "-({e})"),
            Expr::Binary(op, l, r) => write!(f, "({l} {} {r})", op.symbol()),
            Expr::Call(func, a) => write!(f, "{}({a})", func.name()),
            Expr::Lambda(p, b) => write!(f, "(\\{p}. {b})"),
            Expr::Apply(g, a) => write!(f, "({g} {a})"),
            Expr::Assign(name, e) => write!(f, "{name} = {e}"),
            Expr::Simplify(e) => write!(f, "simplify({e})"),
            Expr::Expand(e) => write!(f, "expand({e})"),
            Expr::Derive(v, e) => write!(f, "derive({v}, {e})"),
        }
    }
}
