// ABOUTME: The algebraic rewrite engine (Sub-Task 3): pure, bounded,
// ABOUTME: monotonically simplifying AST -> AST term rewriting.

use crate::ast::{BinaryOp, Expr};
use crate::error::{EvalError, EvalResultT};
use crate::numeric::{as_integer_exponent, pow_int};
use bigdecimal::{BigDecimal, One, Zero};

/// AST node-count ceiling enforced per pass (spec: combinatorial explosion).
pub const MAX_NODES: usize = 10_000;
/// Hard cap on rewrite iterations (spec: infinite rewrite loops).
pub const MAX_REWRITE_DEPTH: usize = 50;

/// Simplify to a fixpoint. Pure: ignores any environment bindings, so it is
/// safe for the explicit `simplify(...)` command even when variables are bound.
pub fn simplify(expr: Expr) -> EvalResultT<Expr> {
    let mut current = expr;
    for _ in 0..MAX_REWRITE_DEPTH {
        if current.node_count() > MAX_NODES {
            return Err(EvalError::ExpressionTooLarge);
        }
        let next = pass(current.clone())?;
        if next == current {
            return Ok(next);
        }
        current = next;
    }
    Err(EvalError::RewriteLimitExceeded)
}

/// One bottom-up rewrite pass: simplify children, then the node itself.
fn pass(expr: Expr) -> EvalResultT<Expr> {
    let with_children = match expr {
        Expr::Number(_) | Expr::Variable(_) => expr,
        Expr::Matrix(rows) => {
            let mut new_rows = Vec::with_capacity(rows.len());
            for row in rows {
                let mut nr = Vec::with_capacity(row.len());
                for el in row {
                    nr.push(pass(el)?);
                }
                new_rows.push(nr);
            }
            Expr::Matrix(new_rows)
        }
        Expr::Neg(e) => Expr::Neg(Box::new(pass(*e)?)),
        Expr::Call(f, e) => Expr::Call(f, Box::new(pass(*e)?)),
        Expr::Binary(op, l, r) => {
            Expr::Binary(op, Box::new(pass(*l)?), Box::new(pass(*r)?))
        }
        // Fold arithmetic inside lambda/application structure (e.g. the
        // body of `\x. 1 + 2`); the engine has already done beta reduction.
        Expr::Lambda(p, b) => Expr::Lambda(p, Box::new(pass(*b)?)),
        Expr::Apply(g, a) => {
            Expr::Apply(Box::new(pass(*g)?), Box::new(pass(*a)?))
        }
        // Command wrappers are resolved by the engine, not here; leave intact.
        other => other,
    };
    rewrite_node(with_children)
}

fn rewrite_node(expr: Expr) -> EvalResultT<Expr> {
    match expr {
        Expr::Neg(e) => Ok(match *e {
            Expr::Number(n) => Expr::Number((-n).normalized()),
            Expr::Neg(inner) => *inner,
            other if is_zero(&other) => Expr::Number(BigDecimal::zero()),
            other => Expr::Neg(Box::new(other)),
        }),

        Expr::Call(f, arg) => Ok(fold_call(f, *arg)),

        Expr::Binary(BinaryOp::Add, _, _) | Expr::Binary(BinaryOp::Sub, _, _) => {
            Ok(collect_sum(&expr))
        }

        Expr::Binary(BinaryOp::Mul, _, _) => Ok(collect_product(&expr)),

        Expr::Binary(BinaryOp::Div, l, r) => Ok(rewrite_div(*l, *r)?),

        Expr::Binary(BinaryOp::Pow, l, r) => Ok(rewrite_pow(*l, *r)),

        other => Ok(other),
    }
}

// ---------------------------------------------------------------------------
// Sums: flatten an Add/Sub tree, combine like terms, fold constants.
// ---------------------------------------------------------------------------

fn collect_sum(expr: &Expr) -> Expr {
    let mut constant = BigDecimal::zero();
    // Insertion-ordered (base, coefficient) groups for deterministic output.
    let mut groups: Vec<(Expr, BigDecimal)> = Vec::new();

    let mut terms = Vec::new();
    flatten_sum(expr, BigDecimal::one(), &mut terms);

    for (coeff, base) in terms {
        match base {
            None => constant += coeff,
            Some(b) => {
                if let Some(slot) = groups.iter_mut().find(|(eb, _)| *eb == b) {
                    slot.1 += &coeff;
                } else {
                    groups.push((b, coeff));
                }
            }
        }
    }

    let mut result: Option<Expr> = None;
    for (base, coeff) in groups {
        if coeff.is_zero() {
            continue;
        }
        let term = if coeff.is_one() {
            base
        } else if (-&coeff).is_one() {
            Expr::Neg(Box::new(base))
        } else {
            Expr::binary(BinaryOp::Mul, Expr::Number(coeff.normalized()), base)
        };
        result = Some(match result {
            None => term,
            Some(acc) => Expr::binary(BinaryOp::Add, acc, term),
        });
    }

    if !constant.is_zero() || result.is_none() {
        let c = Expr::Number(constant.normalized());
        result = Some(match result {
            None => c,
            Some(acc) => Expr::binary(BinaryOp::Add, acc, c),
        });
    }

    result.unwrap_or_else(|| Expr::Number(BigDecimal::zero()))
}

/// Walk an Add/Sub tree, emitting `(coefficient, optional-base)` terms.
/// `sign` carries accumulated +/- and constant factors down the tree.
fn flatten_sum(expr: &Expr, sign: BigDecimal, out: &mut Vec<(BigDecimal, Option<Expr>)>) {
    match expr {
        Expr::Binary(BinaryOp::Add, l, r) => {
            flatten_sum(l, sign.clone(), out);
            flatten_sum(r, sign, out);
        }
        Expr::Binary(BinaryOp::Sub, l, r) => {
            flatten_sum(l, sign.clone(), out);
            flatten_sum(r, -sign, out);
        }
        Expr::Neg(inner) => flatten_sum(inner, -sign, out),
        other => {
            let (coeff, base) = split_coefficient(other);
            out.push((sign * coeff, base));
        }
    }
}

/// Split a single term into a numeric coefficient and the remaining base.
fn split_coefficient(expr: &Expr) -> (BigDecimal, Option<Expr>) {
    match expr {
        Expr::Number(n) => (n.clone(), None),
        Expr::Neg(inner) => {
            let (c, b) = split_coefficient(inner);
            (-c, b)
        }
        Expr::Binary(BinaryOp::Mul, l, r) => match (l.as_ref(), r.as_ref()) {
            (Expr::Number(a), Expr::Number(b)) => ((a * b).normalized(), None),
            (Expr::Number(a), other) => (a.clone(), Some(other.clone())),
            (other, Expr::Number(b)) => (b.clone(), Some(other.clone())),
            _ => (BigDecimal::one(), Some(expr.clone())),
        },
        _ => (BigDecimal::one(), Some(expr.clone())),
    }
}

// ---------------------------------------------------------------------------
// Products: flatten a Mul tree, combine equal bases into powers.
// ---------------------------------------------------------------------------

fn collect_product(expr: &Expr) -> Expr {
    let mut constant = BigDecimal::one();
    let mut groups: Vec<(Expr, BigDecimal)> = Vec::new();

    let mut factors = Vec::new();
    flatten_product(expr, &mut constant, &mut factors);

    if constant.is_zero() {
        return Expr::Number(BigDecimal::zero());
    }

    for (base, exp) in factors {
        if let Some(slot) = groups.iter_mut().find(|(eb, _)| *eb == base) {
            slot.1 += &exp;
        } else {
            groups.push((base, exp));
        }
    }

    // A -1 constant alongside other factors reads better as negation.
    let negate = (-&constant).is_one() && groups.iter().any(|(_, e)| !e.is_zero());

    let mut result: Option<Expr> = None;
    if !constant.is_one() && !negate {
        result = Some(Expr::Number(constant.normalized()));
    }
    for (base, exp) in groups {
        if exp.is_zero() {
            continue;
        }
        let factor = if exp.is_one() {
            base
        } else {
            Expr::binary(BinaryOp::Pow, base, Expr::Number(exp.normalized()))
        };
        result = Some(match result {
            None => factor,
            Some(acc) => Expr::binary(BinaryOp::Mul, acc, factor),
        });
    }

    match result {
        Some(e) if negate => Expr::Neg(Box::new(e)),
        Some(e) => e,
        None => Expr::Number(constant.normalized()),
    }
}

fn flatten_product(expr: &Expr, constant: &mut BigDecimal, out: &mut Vec<(Expr, BigDecimal)>) {
    match expr {
        Expr::Binary(BinaryOp::Mul, l, r) => {
            flatten_product(l, constant, out);
            flatten_product(r, constant, out);
        }
        Expr::Neg(inner) => {
            // Pull the sign into the numeric constant factor.
            let prev = std::mem::replace(constant, BigDecimal::zero());
            *constant = -prev;
            flatten_product(inner, constant, out);
        }
        Expr::Number(n) => *constant *= n,
        Expr::Binary(BinaryOp::Pow, b, e) => {
            if let Expr::Number(exp) = e.as_ref() {
                out.push(((**b).clone(), exp.clone()));
            } else {
                out.push((expr.clone(), BigDecimal::one()));
            }
        }
        other => out.push((other.clone(), BigDecimal::one())),
    }
}

// ---------------------------------------------------------------------------
// Division and exponentiation.
// ---------------------------------------------------------------------------

fn rewrite_div(l: Expr, r: Expr) -> EvalResultT<Expr> {
    if let (Expr::Number(a), Expr::Number(b)) = (&l, &r) {
        if b.is_zero() {
            return Err(EvalError::DivisionByZero);
        }
        return Ok(Expr::Number((a / b).normalized()));
    }
    if is_one(&r) {
        return Ok(l);
    }
    if is_zero(&l) && matches!(&r, Expr::Number(n) if !n.is_zero()) {
        return Ok(Expr::Number(BigDecimal::zero()));
    }
    if l == r {
        // Standard CAS assumption: r != 0.
        return Ok(Expr::Number(BigDecimal::one()));
    }
    Ok(Expr::binary(BinaryOp::Div, l, r))
}

fn rewrite_pow(base: Expr, exp: Expr) -> Expr {
    if let (Expr::Number(b), Expr::Number(e)) = (&base, &exp)
        && let Some(ei) = as_integer_exponent(e)
        && !(b.is_zero() && ei < 0)
    {
        return Expr::Number(pow_int(b, ei));
    }
    if is_one(&exp) {
        return base;
    }
    if is_zero(&exp) {
        return Expr::Number(BigDecimal::one());
    }
    if is_one(&base) {
        return Expr::Number(BigDecimal::one());
    }
    if is_zero(&base) {
        return Expr::Number(BigDecimal::zero());
    }
    // (b^e1)^e2 -> b^(e1*e2) when exponents are numeric.
    if let Expr::Binary(BinaryOp::Pow, inner_b, inner_e) = &base
        && let (Expr::Number(e1), Expr::Number(e2)) = (inner_e.as_ref(), &exp)
    {
        return Expr::binary(
            BinaryOp::Pow,
            (**inner_b).clone(),
            Expr::Number((e1 * e2).normalized()),
        );
    }
    Expr::binary(BinaryOp::Pow, base, exp)
}

fn fold_call(f: crate::ast::Func, arg: Expr) -> Expr {
    use crate::ast::Func::*;
    if let Expr::Number(n) = &arg {
        match f {
            Sin if n.is_zero() => return Expr::Number(BigDecimal::zero()),
            Cos if n.is_zero() => return Expr::Number(BigDecimal::one()),
            Tan if n.is_zero() => return Expr::Number(BigDecimal::zero()),
            Exp if n.is_zero() => return Expr::Number(BigDecimal::one()),
            Ln if n.is_one() => return Expr::Number(BigDecimal::zero()),
            _ => {}
        }
    }
    Expr::Call(f, Box::new(arg))
}

// ---------------------------------------------------------------------------
// Small predicates.
// ---------------------------------------------------------------------------

fn is_zero(e: &Expr) -> bool {
    matches!(e, Expr::Number(n) if n.is_zero())
}

fn is_one(e: &Expr) -> bool {
    matches!(e, Expr::Number(n) if n.is_one())
}
