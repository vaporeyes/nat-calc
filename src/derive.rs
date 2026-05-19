// ABOUTME: Symbolic differentiation: core calculus rules plus the
// ABOUTME: transcendental functions sin/cos/tan/exp/ln.

use crate::ast::{BinaryOp, Expr, Func};
use crate::error::{EvalError, EvalResultT};
use bigdecimal::{BigDecimal, One, Zero};

/// d/d`var` of `expr`, as a raw (unsimplified) AST. The engine simplifies
/// the result afterwards.
pub fn derive(var: &str, expr: &Expr) -> EvalResultT<Expr> {
    match expr {
        Expr::Number(_) | Expr::Bool(_) => Ok(Expr::Number(BigDecimal::from(0))),
        Expr::Variable(v) => Ok(Expr::Number(if v == var {
            BigDecimal::one()
        } else {
            BigDecimal::from(0)
        })),
        Expr::Neg(e) => Ok(Expr::negate(derive(var, e)?)),

        Expr::Binary(BinaryOp::Add, l, r) => Ok(Expr::binary(
            BinaryOp::Add,
            derive(var, l)?,
            derive(var, r)?,
        )),
        Expr::Binary(BinaryOp::Sub, l, r) => Ok(Expr::binary(
            BinaryOp::Sub,
            derive(var, l)?,
            derive(var, r)?,
        )),

        // Product rule: (uv)' = u'v + uv'
        Expr::Binary(BinaryOp::Mul, u, v) => {
            let du = derive(var, u)?;
            let dv = derive(var, v)?;
            if is_zero(&du) && is_zero(&dv) {
                return Ok(zero());
            }
            if is_zero(&du) {
                return Ok(Expr::binary(BinaryOp::Mul, (**u).clone(), dv));
            }
            if is_zero(&dv) {
                return Ok(Expr::binary(BinaryOp::Mul, du, (**v).clone()));
            }
            Ok(Expr::binary(
                BinaryOp::Add,
                Expr::binary(BinaryOp::Mul, du, (**v).clone()),
                Expr::binary(BinaryOp::Mul, (**u).clone(), dv),
            ))
        }

        // Quotient rule: (u/v)' = (u'v - uv') / v^2
        Expr::Binary(BinaryOp::Div, u, v) => {
            let du = derive(var, u)?;
            let dv = derive(var, v)?;
            if is_zero(&du) && is_zero(&dv) {
                return Ok(zero());
            }
            let numerator = Expr::binary(
                BinaryOp::Sub,
                Expr::binary(BinaryOp::Mul, du, (**v).clone()),
                Expr::binary(BinaryOp::Mul, (**u).clone(), dv),
            );
            let denominator = Expr::binary(
                BinaryOp::Pow,
                (**v).clone(),
                Expr::Number(BigDecimal::from(2)),
            );
            Ok(Expr::binary(BinaryOp::Div, numerator, denominator))
        }

        Expr::Binary(BinaryOp::Pow, base, exp) => derive_pow(var, base, exp),

        Expr::Call(f, arg) => {
            let du = derive(var, arg)?;
            if is_zero(&du) {
                return Ok(zero());
            }
            let outer = match f {
                // (sin u)' = cos(u) u'
                Func::Sin => Expr::call(Func::Cos, (**arg).clone()),
                // (cos u)' = -sin(u) u'
                Func::Cos => Expr::negate(Expr::call(Func::Sin, (**arg).clone())),
                // (tan u)' = (1 + tan(u)^2) u'
                Func::Tan => Expr::binary(
                    BinaryOp::Add,
                    Expr::Number(BigDecimal::one()),
                    Expr::binary(
                        BinaryOp::Pow,
                        Expr::call(Func::Tan, (**arg).clone()),
                        Expr::Number(BigDecimal::from(2)),
                    ),
                ),
                // (exp u)' = exp(u) u'
                Func::Exp => Expr::call(Func::Exp, (**arg).clone()),
                // (ln u)' = u'/u
                Func::Ln => {
                    return Ok(Expr::binary(BinaryOp::Div, du, (**arg).clone()));
                }
            };
            Ok(Expr::binary(BinaryOp::Mul, outer, du))
        }

        Expr::Matrix(_) => Err(EvalError::TypeMismatch(
            "cannot differentiate a matrix".into(),
        )),

        Expr::Lambda(..) | Expr::Apply(..) => Err(EvalError::TypeMismatch(
            "cannot differentiate a lambda term".into(),
        )),
        Expr::Not(_) | Expr::Logic(_, _, _) => Err(EvalError::TypeMismatch(
            "cannot differentiate a logic expression".into(),
        )),
        Expr::Equiv(_, _) => Err(EvalError::TypeMismatch(
            "cannot differentiate an equivalence check".into(),
        )),
        Expr::KMap(_, e) => derive(var, e),
        Expr::HalfAdder(_, _) | Expr::FullAdder(_, _, _) => Err(EvalError::TypeMismatch(
            "cannot differentiate an adder preset".into(),
        )),
        Expr::Table(e, _, _, _, _) => derive(var, e),
        Expr::Plot(e, _, _, _) => derive(var, e),

        // Differentiate through explicit command wrappers by differentiating
        // their target. Nested `derive` (higher-order) is not supported.
        Expr::Simplify(e)
        | Expr::Expand(e)
        | Expr::Truth(e)
        | Expr::Circuit(e)
        | Expr::LogicSimplify(e) => {
            derive(var, e)
        }
        Expr::Assign(_, e) => derive(var, e),
        Expr::Derive(_, _) => Err(EvalError::TypeMismatch(
            "higher-order derivatives are not supported".into(),
        )),
    }
}

fn derive_pow(var: &str, base: &Expr, exp: &Expr) -> EvalResultT<Expr> {
    let du = derive(var, base)?;

    // Constant exponent -> power rule: (u^n)' = n*u^(n-1)*u'
    if let Expr::Number(n) = exp {
        if n.is_zero() || is_zero(&du) {
            return Ok(zero());
        }
        let n_minus_1 = (n - BigDecimal::one()).normalized();
        return Ok(Expr::binary(
            BinaryOp::Mul,
            Expr::binary(
                BinaryOp::Mul,
                Expr::Number(n.clone()),
                Expr::binary(BinaryOp::Pow, base.clone(), Expr::Number(n_minus_1)),
            ),
            du,
        ));
    }

    // General case: (u^v)' = u^v * (v'*ln(u) + v*u'/u)
    let dv = derive(var, exp)?;
    if is_zero(&du) && is_zero(&dv) {
        return Ok(zero());
    }
    let term1 = Expr::binary(BinaryOp::Mul, dv, Expr::call(Func::Ln, base.clone()));
    let term2 = Expr::binary(
        BinaryOp::Mul,
        exp.clone(),
        Expr::binary(BinaryOp::Div, du, base.clone()),
    );
    Ok(Expr::binary(
        BinaryOp::Mul,
        Expr::binary(BinaryOp::Pow, base.clone(), exp.clone()),
        Expr::binary(BinaryOp::Add, term1, term2),
    ))
}

fn zero() -> Expr {
    Expr::Number(BigDecimal::zero())
}

fn is_zero(expr: &Expr) -> bool {
    matches!(expr, Expr::Number(n) if n.is_zero())
}
