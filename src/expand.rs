// ABOUTME: Polynomial expansion: distribute products over sums and
// ABOUTME: unroll integer powers, with a node-count guard against blowup.

use crate::ast::{BinaryOp, Expr};
use crate::error::{EvalError, EvalResultT};
use crate::numeric::as_integer_exponent;
use crate::simplify::MAX_NODES;

/// Distribute `*` over `+`/`-` and unroll `^` with small non-negative integer
/// exponents. The caller is expected to `simplify` the result to collect
/// like terms. Bounded by `MAX_NODES` (spec: combinatorial explosion).
pub fn expand(expr: &Expr) -> EvalResultT<Expr> {
    let out = expand_inner(expr)?;
    if out.node_count() > MAX_NODES {
        return Err(EvalError::ExpressionTooLarge);
    }
    Ok(out)
}

fn expand_inner(expr: &Expr) -> EvalResultT<Expr> {
    match expr {
        Expr::Number(_) | Expr::Variable(_) => Ok(expr.clone()),
        Expr::Neg(e) => Ok(Expr::negate(expand_inner(e)?)),
        Expr::Call(f, e) => Ok(Expr::call(*f, expand_inner(e)?)),
        Expr::Matrix(_) => Ok(expr.clone()),

        Expr::Binary(BinaryOp::Add, l, r) => Ok(Expr::binary(
            BinaryOp::Add,
            expand_inner(l)?,
            expand_inner(r)?,
        )),
        Expr::Binary(BinaryOp::Sub, l, r) => Ok(Expr::binary(
            BinaryOp::Sub,
            expand_inner(l)?,
            expand_inner(r)?,
        )),
        Expr::Binary(BinaryOp::Div, l, r) => Ok(Expr::binary(
            BinaryOp::Div,
            expand_inner(l)?,
            expand_inner(r)?,
        )),

        Expr::Binary(BinaryOp::Mul, l, r) => {
            let le = expand_inner(l)?;
            let re = expand_inner(r)?;
            distribute(&le, &re)
        }

        Expr::Binary(BinaryOp::Pow, base, exp) => {
            let be = expand_inner(base)?;
            if let Expr::Number(n) = exp.as_ref()
                && let Some(k) = as_integer_exponent(n)
                && (0..=64).contains(&k)
            {
                return unroll_pow(&be, k as u32);
            }
            Ok(Expr::binary(
                BinaryOp::Pow,
                be,
                expand_inner(exp)?,
            ))
        }

        // Command wrappers are handled by the engine before expansion.
        other => Ok(other.clone()),
    }
}

/// `(a1 + a2 + ...) * (b1 + b2 + ...)` -> sum of pairwise products.
///
/// The result-term count (`|left| * |right|`) is checked *before* building
/// anything, so a combinatorial blowup is rejected cheaply rather than after
/// allocating an enormous tree (spec Section 3). The sum is assembled as a
/// balanced tree to keep later recursive passes shallow.
fn distribute(l: &Expr, r: &Expr) -> EvalResultT<Expr> {
    let left_terms = sum_terms(l);
    let right_terms = sum_terms(r);

    if left_terms.len().saturating_mul(right_terms.len()) > MAX_NODES {
        return Err(EvalError::ExpressionTooLarge);
    }

    let mut products = Vec::with_capacity(left_terms.len() * right_terms.len());
    for a in &left_terms {
        for b in &right_terms {
            products.push(Expr::binary(BinaryOp::Mul, a.clone(), b.clone()));
        }
    }
    Ok(sum_balanced(&products)
        .unwrap_or_else(|| Expr::binary(BinaryOp::Mul, l.clone(), r.clone())))
}

/// Fold terms into a balanced `Add` tree (depth O(log n)).
fn sum_balanced(terms: &[Expr]) -> Option<Expr> {
    match terms {
        [] => None,
        [single] => Some(single.clone()),
        _ => {
            let mid = terms.len() / 2;
            let l = sum_balanced(&terms[..mid])?;
            let r = sum_balanced(&terms[mid..])?;
            Some(Expr::binary(BinaryOp::Add, l, r))
        }
    }
}

/// Split an Add/Sub tree into a flat list of additive terms (signs folded
/// into `Neg`), so distribution can pair every term with every other.
fn sum_terms(expr: &Expr) -> Vec<Expr> {
    let mut out = Vec::new();
    collect_terms(expr, false, &mut out);
    out
}

fn collect_terms(expr: &Expr, negate: bool, out: &mut Vec<Expr>) {
    match expr {
        Expr::Binary(BinaryOp::Add, l, r) => {
            collect_terms(l, negate, out);
            collect_terms(r, negate, out);
        }
        Expr::Binary(BinaryOp::Sub, l, r) => {
            collect_terms(l, negate, out);
            collect_terms(r, !negate, out);
        }
        other => out.push(if negate {
            Expr::negate(other.clone())
        } else {
            other.clone()
        }),
    }
}

/// `base^k` as repeated multiplication (k small and non-negative). Each
/// step's `distribute` enforces the node-count ceiling, so an exploding
/// power bails early instead of overflowing the stack.
fn unroll_pow(base: &Expr, k: u32) -> EvalResultT<Expr> {
    if k == 0 {
        return Ok(Expr::num(1));
    }
    let mut acc = base.clone();
    for _ in 1..k {
        acc = distribute(&acc, base)?;
    }
    Ok(acc)
}
