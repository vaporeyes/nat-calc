// ABOUTME: Numeric graphing helpers for tables and plots.
// ABOUTME: Samples arithmetic expressions over f64 domains.

use crate::ast::{BinaryOp, Expr, Func};
use crate::error::{EvalError, EvalResultT};
use std::collections::{BTreeSet, HashMap};
use std::fmt;

const MAX_SAMPLES: usize = 1_000;

#[derive(Debug, Clone, PartialEq)]
pub struct ValueTable {
    pub var: String,
    pub rows: Vec<(f64, f64)>,
}

impl fmt::Display for ValueTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} | y", self.var)?;
        for (x, y) in &self.rows {
            writeln!(f, "{} | {}", fmt_num(*x), fmt_num(*y))?;
        }
        Ok(())
    }
}

pub fn table(
    expr: &Expr,
    var: &str,
    start: &Expr,
    end: &Expr,
    step: &Expr,
) -> EvalResultT<ValueTable> {
    let start = eval_numeric(start, &HashMap::new())?;
    let end = eval_numeric(end, &HashMap::new())?;
    let step = eval_numeric(step, &HashMap::new())?;
    if !start.is_finite() || !end.is_finite() || !step.is_finite() || step == 0.0 {
        return Err(EvalError::TypeMismatch("invalid table range".into()));
    }
    let mut rows = Vec::new();
    let mut env = HashMap::new();
    let mut x = start;
    let forward = step > 0.0;
    while if forward { x <= end } else { x >= end } {
        if rows.len() >= MAX_SAMPLES {
            return Err(EvalError::ExpressionTooLarge);
        }
        env.insert(var, x);
        rows.push((x, eval_numeric(expr, &env)?));
        x += step;
    }
    Ok(ValueTable {
        var: var.to_string(),
        rows,
    })
}

pub fn graph_vars(expr: &Expr) -> Vec<String> {
    let mut vars = BTreeSet::new();
    collect_vars(expr, &mut vars);
    vars.into_iter().collect()
}

pub fn eval_numeric(expr: &Expr, env: &HashMap<&str, f64>) -> EvalResultT<f64> {
    match expr {
        Expr::Number(n) => n
            .to_string()
            .parse::<f64>()
            .map_err(|_| EvalError::TypeMismatch(format!("invalid number '{n}'"))),
        Expr::Variable(name) => env
            .get(name.as_str())
            .copied()
            .ok_or_else(|| EvalError::TypeMismatch(format!("unbound graph variable '{name}'"))),
        Expr::Neg(e) => Ok(-eval_numeric(e, env)?),
        Expr::Binary(op, l, r) => {
            let a = eval_numeric(l, env)?;
            let b = eval_numeric(r, env)?;
            Ok(match op {
                BinaryOp::Add => a + b,
                BinaryOp::Sub => a - b,
                BinaryOp::Mul => a * b,
                BinaryOp::Div => a / b,
                BinaryOp::Pow => a.powf(b),
            })
        }
        Expr::Call(func, arg) => {
            let v = eval_numeric(arg, env)?;
            Ok(match func {
                Func::Sin => v.sin(),
                Func::Cos => v.cos(),
                Func::Tan => v.tan(),
                Func::Exp => v.exp(),
                Func::Ln => v.ln(),
            })
        }
        other => Err(EvalError::TypeMismatch(format!(
            "expected numeric graph expression, found {other}"
        ))),
    }
}

fn collect_vars(expr: &Expr, vars: &mut BTreeSet<String>) {
    match expr {
        Expr::Variable(name) => {
            vars.insert(name.clone());
        }
        Expr::Neg(e) | Expr::Call(_, e) => collect_vars(e, vars),
        Expr::Binary(_, l, r) => {
            collect_vars(l, vars);
            collect_vars(r, vars);
        }
        _ => {}
    }
}

fn fmt_num(value: f64) -> String {
    if value.abs() < 1e-12 {
        return "0".into();
    }
    let rounded = (value * 1_000_000.0).round() / 1_000_000.0;
    format!("{rounded:.6}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}
