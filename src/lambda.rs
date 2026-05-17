// ABOUTME: Untyped lambda calculus: capture-avoiding substitution and
// ABOUTME: normal-order beta reduction, env-aware and step-capped.

use crate::ast::Expr;
use crate::engine::Environment;
use crate::error::{EvalError, EvalResultT};
use std::collections::BTreeSet;

/// Hard cap on beta/expansion steps (Section 3: divergence mitigation).
/// `(\x. x x)(\x. x x)` and friends hit this instead of looping forever.
pub const MAX_BETA_STEPS: usize = 100_000;

/// Reduce a term containing lambdas/applications to normal form. Free
/// variables that are bound in `env` are expanded (this is what makes a
/// stored lambda act as a user-defined function — Sub-Task 4).
pub fn reduce_lambda(expr: Expr, env: &Environment) -> EvalResultT<Expr> {
    let mut steps = 0usize;
    normal_form(expr, env, &mut steps)
}

fn tick(steps: &mut usize) -> EvalResultT<()> {
    *steps += 1;
    if *steps > MAX_BETA_STEPS {
        Err(EvalError::RewriteLimitExceeded)
    } else {
        Ok(())
    }
}

/// Weak head normal form: peel outermost redexes and expand the head
/// variable from the environment until the head is irreducible.
fn whnf(expr: Expr, env: &Environment, steps: &mut usize) -> EvalResultT<Expr> {
    let mut e = expr;
    loop {
        match e {
            Expr::Apply(f, a) => {
                let f = whnf(*f, env, steps)?;
                if let Expr::Lambda(param, body) = f {
                    tick(steps)?;
                    e = substitute(*body, &param, &a);
                } else {
                    return Ok(Expr::Apply(Box::new(f), a));
                }
            }
            Expr::Variable(name) => match env.get(&name) {
                Some(bound) => {
                    tick(steps)?;
                    e = bound.clone();
                }
                None => return Ok(Expr::Variable(name)),
            },
            other => return Ok(other),
        }
    }
}

/// Full normal-order normalization: head-reduce, then recurse under the
/// resulting binder / into the stuck application's parts.
fn normal_form(
    expr: Expr,
    env: &Environment,
    steps: &mut usize,
) -> EvalResultT<Expr> {
    match whnf(expr, env, steps)? {
        Expr::Lambda(param, body) => {
            let body = normal_form(*body, env, steps)?;
            Ok(Expr::Lambda(param, Box::new(body)))
        }
        Expr::Apply(f, a) => {
            let f = normal_form(*f, env, steps)?;
            let a = normal_form(*a, env, steps)?;
            Ok(Expr::Apply(Box::new(f), Box::new(a)))
        }
        other => Ok(other),
    }
}

/// Collect free variable names of `e`.
fn free_vars(e: &Expr, acc: &mut BTreeSet<String>) {
    match e {
        Expr::Variable(v) => {
            acc.insert(v.clone());
        }
        Expr::Lambda(p, b) => {
            let mut inner = BTreeSet::new();
            free_vars(b, &mut inner);
            inner.remove(p);
            acc.extend(inner);
        }
        Expr::Apply(l, r) | Expr::Binary(_, l, r) => {
            free_vars(l, acc);
            free_vars(r, acc);
        }
        Expr::Neg(x)
        | Expr::Call(_, x)
        | Expr::Simplify(x)
        | Expr::Expand(x)
        | Expr::Assign(_, x)
        | Expr::Derive(_, x) => free_vars(x, acc),
        Expr::Matrix(rows) => {
            for el in rows.iter().flatten() {
                free_vars(el, acc);
            }
        }
        Expr::Number(_) => {}
    }
}

/// Capture-avoiding substitution: replace free `Variable(var)` with `value`.
fn substitute(expr: Expr, var: &str, value: &Expr) -> Expr {
    match expr {
        Expr::Variable(v) => {
            if v == var {
                value.clone()
            } else {
                Expr::Variable(v)
            }
        }
        Expr::Lambda(param, body) => {
            if param == var {
                // `var` is shadowed by this binder: nothing to do.
                return Expr::Lambda(param, body);
            }
            let mut value_fv = BTreeSet::new();
            free_vars(value, &mut value_fv);
            if value_fv.contains(&param) {
                // Renaming `param` would capture a free var of `value`;
                // alpha-rename the binder to a fresh name first.
                let mut forbidden = value_fv;
                free_vars(&body, &mut forbidden);
                forbidden.insert(var.to_string());
                let fresh = fresh_name(&param, &forbidden);
                let body =
                    substitute(*body, &param, &Expr::Variable(fresh.clone()));
                Expr::Lambda(fresh, Box::new(substitute(body, var, value)))
            } else {
                Expr::Lambda(param, Box::new(substitute(*body, var, value)))
            }
        }
        Expr::Apply(l, r) => Expr::Apply(
            Box::new(substitute(*l, var, value)),
            Box::new(substitute(*r, var, value)),
        ),
        Expr::Binary(op, l, r) => Expr::Binary(
            op,
            Box::new(substitute(*l, var, value)),
            Box::new(substitute(*r, var, value)),
        ),
        Expr::Neg(x) => Expr::Neg(Box::new(substitute(*x, var, value))),
        Expr::Call(func, x) => {
            Expr::Call(func, Box::new(substitute(*x, var, value)))
        }
        Expr::Simplify(x) => {
            Expr::Simplify(Box::new(substitute(*x, var, value)))
        }
        Expr::Expand(x) => Expr::Expand(Box::new(substitute(*x, var, value))),
        Expr::Derive(v, x) => {
            Expr::Derive(v, Box::new(substitute(*x, var, value)))
        }
        Expr::Assign(n, x) => {
            Expr::Assign(n, Box::new(substitute(*x, var, value)))
        }
        Expr::Matrix(rows) => Expr::Matrix(
            rows.into_iter()
                .map(|row| {
                    row.into_iter()
                        .map(|el| substitute(el, var, value))
                        .collect()
                })
                .collect(),
        ),
        n @ Expr::Number(_) => n,
    }
}

fn fresh_name(base: &str, forbidden: &BTreeSet<String>) -> String {
    let mut name = format!("{base}'");
    while forbidden.contains(&name) {
        name.push('\'');
    }
    name
}
