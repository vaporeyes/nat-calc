// ABOUTME: The unified reduction engine: eager numeric path, lazy symbolic
// ABOUTME: fallback, delayed binding, and memoized environment.

use crate::ast::{BinaryOp, Expr, LogicOp};
use crate::derive::derive;
use crate::error::{EvalError, EvalResultT};
use crate::expand::expand;
use crate::graph::{ValueTable, table};
use crate::lambda::reduce_lambda;
use crate::logic::{
    AdderResult, CircuitDiagram, EquivResult, KMap, TruthTable, adder_preset,
    circuit_diagram, equivalence, kmap, simplify_logic, truth_table,
};
use crate::numeric::{as_integer_exponent, bounded_div, pow_int};
use crate::simplify::simplify;
use bigdecimal::{BigDecimal, Zero};
use std::collections::{HashMap, HashSet};

/// Engine output: terminal (Numeric/Matrix) or a reduced AST (Symbolic).
#[derive(Debug, Clone, PartialEq)]
pub enum EvalResult {
    Numeric(BigDecimal),
    Bool(bool),
    TruthTable(TruthTable),
    CircuitDiagram(CircuitDiagram),
    EquivResult(EquivResult),
    KMap(KMap),
    AdderResult(AdderResult),
    ValueTable(ValueTable),
    Matrix(Vec<Vec<BigDecimal>>),
    Symbolic(Box<Expr>),
    /// A lambda abstraction in normal form (its own reduction mode).
    Lambda(Box<Expr>),
}

impl std::fmt::Display for EvalResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalResult::Numeric(n) => write!(f, "{}", n.normalized()),
            EvalResult::Bool(b) => write!(f, "{b}"),
            EvalResult::TruthTable(table) => write!(f, "{table}"),
            EvalResult::CircuitDiagram(diagram) => write!(f, "{diagram}"),
            EvalResult::EquivResult(result) => write!(f, "{result}"),
            EvalResult::KMap(map) => write!(f, "{map}"),
            EvalResult::AdderResult(result) => write!(f, "{result}"),
            EvalResult::ValueTable(table) => write!(f, "{table}"),
            EvalResult::Matrix(rows) => {
                write!(f, "[")?;
                for (i, row) in rows.iter().enumerate() {
                    if i > 0 {
                        write!(f, "; ")?;
                    }
                    for (j, v) in row.iter().enumerate() {
                        if j > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", v.normalized())?;
                    }
                }
                write!(f, "]")
            }
            EvalResult::Symbolic(e) => write!(f, "{e}"),
            EvalResult::Lambda(e) => write!(f, "{e}"),
        }
    }
}

/// Variables are stored as unevaluated `Expr` (Sub-Task 1). A simple result
/// cache provides memoization; any assignment clears it (correct and
/// monotonic, the spec's alternative to a dependency DAG).
#[derive(Clone, Default)]
pub struct Environment {
    vars: HashMap<String, Expr>,
    cache: HashMap<String, EvalResult>,
    /// Names whose binding is currently being reduced. Re-entering one means
    /// a self/mutually referential binding (e.g. `y = y + y` stores `2*y`);
    /// the recursive occurrence is treated as a free symbol to break what
    /// would otherwise be unbounded recursion.
    resolving: HashSet<String>,
}

impl Environment {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, name: &str) -> Option<&Expr> {
        self.vars.get(name)
    }

    /// Snapshot of all bindings (name, stored AST), sorted by name. Used by
    /// the GUI to render a live environment panel.
    pub fn bindings(&self) -> Vec<(String, Expr)> {
        let mut out: Vec<(String, Expr)> = self
            .vars
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    pub(crate) fn resolving_names(&self) -> HashSet<String> {
        self.resolving.clone()
    }

    fn store(&mut self, name: String, value: Expr) {
        self.vars.insert(name, value);
        self.cache.clear(); // invalidate dependents
    }
}

/// Reduce `expr` against `env`. The single entry point.
pub fn reduce(expr: &Expr, env: &mut Environment) -> EvalResultT<EvalResult> {
    match expr {
        Expr::Number(n) => Ok(EvalResult::Numeric(n.clone())),
        Expr::Bool(b) => Ok(EvalResult::Bool(*b)),

        Expr::Variable(name) => {
            if let Some(cached) = env.cache.get(name) {
                return Ok(cached.clone());
            }
            if env.resolving.contains(name) {
                // Cyclic binding: treat this occurrence as a free symbol.
                return Ok(EvalResult::Symbolic(Box::new(Expr::Variable(
                    name.clone(),
                ))));
            }
            match env.get(name).cloned() {
                Some(bound) => {
                    env.resolving.insert(name.clone());
                    let result = reduce(&bound, env);
                    env.resolving.remove(name);
                    let result = result?;
                    env.cache.insert(name.clone(), result.clone());
                    Ok(result)
                }
                // Unbound: graceful fallback to the symbolic path.
                None => Ok(EvalResult::Symbolic(Box::new(Expr::Variable(
                    name.clone(),
                )))),
            }
        }

        Expr::Matrix(rows) => reduce_matrix_literal(rows, env),

        Expr::Neg(e) => match reduce(e, env)? {
            EvalResult::Numeric(n) => Ok(EvalResult::Numeric((-n).normalized())),
            EvalResult::Matrix(m) => Ok(EvalResult::Matrix(map_matrix(m, |v| -v))),
            EvalResult::Bool(_) => Err(EvalError::TypeMismatch(
                "unary minus cannot be applied to a boolean".into(),
            )),
            EvalResult::TruthTable(_) => Err(EvalError::TypeMismatch(
                "unary minus cannot be applied to a truth table".into(),
            )),
            EvalResult::CircuitDiagram(_) => Err(EvalError::TypeMismatch(
                "unary minus cannot be applied to a circuit diagram".into(),
            )),
            EvalResult::EquivResult(_) => Err(EvalError::TypeMismatch(
                "unary minus cannot be applied to an equivalence result".into(),
            )),
            EvalResult::KMap(_) => Err(EvalError::TypeMismatch(
                "unary minus cannot be applied to a kmap".into(),
            )),
            EvalResult::AdderResult(_) => Err(EvalError::TypeMismatch(
                "unary minus cannot be applied to an adder result".into(),
            )),
            EvalResult::ValueTable(_) => Err(EvalError::TypeMismatch(
                "unary minus cannot be applied to a value table".into(),
            )),
            EvalResult::Symbolic(inner) | EvalResult::Lambda(inner) => {
                classify(simplify(Expr::negate(*inner))?)
            }
        },

        Expr::Binary(op, l, r) => {
            let lv = reduce(l, env)?;
            let rv = reduce(r, env)?;
            reduce_binary(*op, lv, rv)
        }

        Expr::Not(e) => match reduce(e, env)? {
            EvalResult::Bool(b) => Ok(EvalResult::Bool(!b)),
            other => classify(Expr::not(result_to_expr(other)?)),
        },

        Expr::Logic(op, l, r) => {
            let lv = reduce(l, env)?;
            let rv = reduce(r, env)?;
            reduce_logic(*op, lv, rv)
        }

        Expr::Call(f, arg) => match reduce(arg, env)? {
            EvalResult::Matrix(_) => Err(EvalError::TypeMismatch(
                "function applied to a matrix".into(),
            )),
            other => {
                let arg_expr = result_to_expr(other)?;
                classify(simplify(Expr::call(*f, arg_expr))?)
            }
        },

        // Lambda calculus mode: normalize via capture-avoiding, env-aware
        // normal-order beta reduction, then let the engine resolve any
        // residual bindings / arithmetic in the result.
        Expr::Lambda(..) | Expr::Apply(..) => {
            let nf = reduce_lambda(expr.clone(), env)?;
            match nf {
                lam @ Expr::Lambda(..) => {
                    Ok(EvalResult::Lambda(Box::new(simplify(lam)?)))
                }
                stuck @ Expr::Apply(..) => classify(simplify(stuck)?),
                other => reduce(&other, env),
            }
        }

        Expr::Assign(name, rhs) => {
            let value = reduce(rhs, env)?;
            env.store(name.clone(), result_to_expr(value.clone())?);
            Ok(value)
        }

        // Explicit context switches: operate structurally on the AST,
        // ignoring bindings (force lazy even if variables are bound).
        Expr::Simplify(e) => classify(simplify((**e).clone())?),
        Expr::Expand(e) => classify(simplify(expand(e)?)?),
        Expr::Derive(var, e) => {
            let d = derive(var, e)?;
            classify(simplify(d)?)
        }
        Expr::Truth(e) => Ok(EvalResult::TruthTable(truth_table(e)?)),
        Expr::Circuit(e) => Ok(EvalResult::CircuitDiagram(circuit_diagram(e)?)),
        Expr::LogicSimplify(e) => classify(simplify_logic(e)?),
        Expr::Equiv(l, r) => Ok(EvalResult::EquivResult(equivalence(l, r)?)),
        Expr::KMap(vars, e) => Ok(EvalResult::KMap(kmap(vars, e)?)),
        Expr::HalfAdder(a, b) => Ok(EvalResult::AdderResult(adder_preset(
            "half_adder",
            vec![
                (
                    "sum".into(),
                    Expr::logic(LogicOp::Xor, (**a).clone(), (**b).clone()),
                ),
                (
                    "carry".into(),
                    Expr::logic(LogicOp::And, (**a).clone(), (**b).clone()),
                ),
            ],
        )?)),
        Expr::FullAdder(a, b, c) => {
            let axb = Expr::logic(LogicOp::Xor, (**a).clone(), (**b).clone());
            let sum = Expr::logic(LogicOp::Xor, axb.clone(), (**c).clone());
            let carry_left = Expr::logic(LogicOp::And, (**a).clone(), (**b).clone());
            let carry_right = Expr::logic(LogicOp::And, axb, (**c).clone());
            let carry = Expr::logic(LogicOp::Or, carry_left, carry_right);
            Ok(EvalResult::AdderResult(adder_preset(
                "full_adder",
                vec![("sum".into(), sum), ("carry".into(), carry)],
            )?))
        }
        Expr::Table(e, var, start, end, step) => {
            Ok(EvalResult::ValueTable(table(e, var, start, end, step)?))
        }
    }
}

fn reduce_binary(
    op: BinaryOp,
    lv: EvalResult,
    rv: EvalResult,
) -> EvalResultT<EvalResult> {
    use EvalResult::*;
    match (lv, rv) {
        (Numeric(a), Numeric(b)) => numeric_op(op, a, b),

        (Matrix(a), Matrix(b)) => matrix_matrix(op, a, b),
        (Numeric(s), Matrix(m)) | (Matrix(m), Numeric(s)) => {
            scalar_matrix(op, s, m)
        }

        // Anything symbolic (or matrix mixed with symbolic) -> lazy path.
        (a, b) => {
            let le = result_to_expr(a)?;
            let re = result_to_expr(b)?;
            classify(simplify(Expr::binary(op, le, re))?)
        }
    }
}

fn numeric_op(
    op: BinaryOp,
    a: BigDecimal,
    b: BigDecimal,
) -> EvalResultT<EvalResult> {
    let v = match op {
        BinaryOp::Add => a + b,
        BinaryOp::Sub => a - b,
        BinaryOp::Mul => a * b,
        BinaryOp::Div => {
            if b.is_zero() {
                return Err(EvalError::DivisionByZero);
            }
            bounded_div(&a, &b)
        }
        BinaryOp::Pow => match as_integer_exponent(&b) {
            Some(e) if !(a.is_zero() && e < 0) => pow_int(&a, e),
            // Non-integer exponent: stay exact via the symbolic path.
            _ => {
                return classify(simplify(Expr::binary(
                    BinaryOp::Pow,
                    Expr::Number(a),
                    Expr::Number(b),
                ))?);
            }
        },
    };
    Ok(EvalResult::Numeric(v.normalized()))
}

fn reduce_logic(op: LogicOp, lv: EvalResult, rv: EvalResult) -> EvalResultT<EvalResult> {
    match (lv, rv) {
        (EvalResult::Bool(a), EvalResult::Bool(b)) => {
            let v = match op {
                LogicOp::And => a && b,
                LogicOp::Or => a || b,
                LogicOp::Xor => a ^ b,
                LogicOp::Nand => !(a && b),
                LogicOp::Nor => !(a || b),
            };
            Ok(EvalResult::Bool(v))
        }
        (a, b) => classify(Expr::logic(op, result_to_expr(a)?, result_to_expr(b)?)),
    }
}

// ---------------------------------------------------------------------------
// Matrix arithmetic (numeric only).
// ---------------------------------------------------------------------------

type Mat = Vec<Vec<BigDecimal>>;

fn reduce_matrix_literal(
    rows: &[Vec<Expr>],
    env: &mut Environment,
) -> EvalResultT<EvalResult> {
    let mut out: Mat = Vec::with_capacity(rows.len());
    for row in rows {
        let mut nr = Vec::with_capacity(row.len());
        for el in row {
            match reduce(el, env)? {
                EvalResult::Numeric(n) => nr.push(n),
                _ => {
                    return Err(EvalError::TypeMismatch(
                        "matrix elements must be numeric".into(),
                    ));
                }
            }
        }
        out.push(nr);
    }
    Ok(EvalResult::Matrix(out))
}

fn map_matrix(m: Mat, f: impl Fn(BigDecimal) -> BigDecimal) -> Mat {
    m.into_iter()
        .map(|row| row.into_iter().map(&f).map(|v| v.normalized()).collect())
        .collect()
}

fn dims(m: &Mat) -> (usize, usize) {
    (m.len(), m.first().map_or(0, Vec::len))
}

fn matrix_matrix(op: BinaryOp, a: Mat, b: Mat) -> EvalResultT<EvalResult> {
    match op {
        BinaryOp::Add | BinaryOp::Sub => {
            if dims(&a) != dims(&b) {
                return Err(EvalError::TypeMismatch(
                    "matrix dimension mismatch for +/-".into(),
                ));
            }
            let out = a
                .into_iter()
                .zip(b)
                .map(|(ra, rb)| {
                    ra.into_iter()
                        .zip(rb)
                        .map(|(x, y)| {
                            if op == BinaryOp::Add { x + y } else { x - y }
                                .normalized()
                        })
                        .collect()
                })
                .collect();
            Ok(EvalResult::Matrix(out))
        }
        BinaryOp::Mul => {
            let (ar, ac) = dims(&a);
            let (br, bc) = dims(&b);
            if ac != br {
                return Err(EvalError::TypeMismatch(
                    "matrix dimension mismatch for *".into(),
                ));
            }
            let mut out = vec![vec![BigDecimal::zero(); bc]; ar];
            for i in 0..ar {
                for j in 0..bc {
                    let mut acc = BigDecimal::zero();
                    for k in 0..ac {
                        acc += &a[i][k] * &b[k][j];
                    }
                    out[i][j] = acc.normalized();
                }
            }
            Ok(EvalResult::Matrix(out))
        }
        _ => Err(EvalError::TypeMismatch(
            "unsupported matrix-matrix operation".into(),
        )),
    }
}

fn scalar_matrix(op: BinaryOp, s: BigDecimal, m: Mat) -> EvalResultT<EvalResult> {
    match op {
        BinaryOp::Mul => Ok(EvalResult::Matrix(map_matrix(m, |v| &v * &s))),
        BinaryOp::Div => {
            if s.is_zero() {
                return Err(EvalError::DivisionByZero);
            }
            Ok(EvalResult::Matrix(map_matrix(m, |v| bounded_div(&v, &s))))
        }
        _ => Err(EvalError::TypeMismatch(
            "only scalar * / matrix is supported".into(),
        )),
    }
}

// ---------------------------------------------------------------------------
// Bridging between EvalResult and Expr.
// ---------------------------------------------------------------------------

fn result_to_expr(r: EvalResult) -> EvalResultT<Expr> {
    match r {
        EvalResult::Numeric(n) => Ok(Expr::Number(n)),
        EvalResult::Bool(b) => Ok(Expr::Bool(b)),
        EvalResult::TruthTable(_) => Err(EvalError::TypeMismatch(
            "a truth table cannot appear inside an expression".into(),
        )),
        EvalResult::CircuitDiagram(_) => Err(EvalError::TypeMismatch(
            "a circuit diagram cannot appear inside an expression".into(),
        )),
        EvalResult::EquivResult(_) => Err(EvalError::TypeMismatch(
            "an equivalence result cannot appear inside an expression".into(),
        )),
        EvalResult::KMap(_) => Err(EvalError::TypeMismatch(
            "a kmap cannot appear inside an expression".into(),
        )),
        EvalResult::AdderResult(_) => Err(EvalError::TypeMismatch(
            "an adder result cannot appear inside an expression".into(),
        )),
        EvalResult::ValueTable(_) => Err(EvalError::TypeMismatch(
            "a value table cannot appear inside an expression".into(),
        )),
        EvalResult::Symbolic(e) | EvalResult::Lambda(e) => Ok(*e),
        EvalResult::Matrix(_) => Err(EvalError::TypeMismatch(
            "a matrix cannot appear inside a symbolic expression".into(),
        )),
    }
}

/// Decide whether a reduced AST is terminal (Numeric/Matrix) or symbolic.
fn classify(e: Expr) -> EvalResultT<EvalResult> {
    match e {
        Expr::Number(n) => Ok(EvalResult::Numeric(n)),
        Expr::Bool(b) => Ok(EvalResult::Bool(b)),
        Expr::Matrix(rows) => {
            let mut out: Mat = Vec::with_capacity(rows.len());
            for row in &rows {
                let mut nr = Vec::with_capacity(row.len());
                for el in row {
                    match el {
                        Expr::Number(n) => nr.push(n.clone()),
                        _ => {
                            return Ok(EvalResult::Symbolic(Box::new(
                                Expr::Matrix(rows.clone()),
                            )));
                        }
                    }
                }
                out.push(nr);
            }
            Ok(EvalResult::Matrix(out))
        }
        lam @ Expr::Lambda(..) => Ok(EvalResult::Lambda(Box::new(lam))),
        other => Ok(EvalResult::Symbolic(Box::new(other))),
    }
}
