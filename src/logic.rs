// ABOUTME: Boolean logic helpers for truth tables and related commands.
// ABOUTME: Evaluates logic expressions structurally against boolean rows.

use crate::ast::{Expr, LogicOp};
use crate::error::{EvalError, EvalResultT};
use std::collections::{BTreeSet, HashMap};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruthTable {
    pub vars: Vec<String>,
    pub rows: Vec<(Vec<bool>, bool)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CircuitDiagram {
    pub lines: Vec<String>,
}

impl fmt::Display for CircuitDiagram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for line in &self.lines {
            writeln!(f, "{line}")?;
        }
        Ok(())
    }
}

impl fmt::Display for TruthTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for name in &self.vars {
            write!(f, "{name} ")?;
        }
        writeln!(f, "| out")?;
        for (values, out) in &self.rows {
            for value in values {
                write!(f, "{} ", bool_digit(*value))?;
            }
            writeln!(f, "| {}", bool_digit(*out))?;
        }
        Ok(())
    }
}

pub fn circuit_diagram(expr: &Expr) -> EvalResultT<CircuitDiagram> {
    validate_logic_expr(expr)?;
    let mut lines = vec!["OUT".to_string()];
    circuit_lines(expr, "", true, &mut lines);
    Ok(CircuitDiagram { lines })
}

pub fn truth_table(expr: &Expr) -> EvalResultT<TruthTable> {
    let vars = logic_vars(expr);
    if vars.len() > 8 {
        return Err(EvalError::ExpressionTooLarge);
    }
    let mut rows = Vec::new();
    let count = 1usize << vars.len();
    for mask in 0..count {
        let mut env = HashMap::new();
        let mut values = Vec::new();
        for (i, name) in vars.iter().enumerate() {
            let value = ((mask >> (vars.len() - i - 1)) & 1) == 1;
            env.insert(name.as_str(), value);
            values.push(value);
        }
        rows.push((values, eval_logic(expr, &env)?));
    }
    Ok(TruthTable { vars, rows })
}

fn circuit_lines(expr: &Expr, prefix: &str, last: bool, lines: &mut Vec<String>) {
    let branch = if last { "`- " } else { "|- " };
    lines.push(format!("{prefix}{branch}{}", gate_label(expr)));
    let child_prefix = format!("{prefix}{}", if last { "   " } else { "|  " });
    match expr {
        Expr::Not(e) => circuit_lines(e, &child_prefix, true, lines),
        Expr::Logic(_, l, r) => {
            circuit_lines(l, &child_prefix, false, lines);
            circuit_lines(r, &child_prefix, true, lines);
        }
        _ => {}
    }
}

fn gate_label(expr: &Expr) -> String {
    match expr {
        Expr::Bool(true) => "TRUE".to_string(),
        Expr::Bool(false) => "FALSE".to_string(),
        Expr::Variable(name) => name.clone(),
        Expr::Not(_) => "NOT".to_string(),
        Expr::Logic(op, _, _) => op.name().to_uppercase(),
        other => other.to_string(),
    }
}

fn validate_logic_expr(expr: &Expr) -> EvalResultT<()> {
    match expr {
        Expr::Bool(_) | Expr::Variable(_) => Ok(()),
        Expr::Not(e) => validate_logic_expr(e),
        Expr::Logic(_, l, r) => {
            validate_logic_expr(l)?;
            validate_logic_expr(r)
        }
        other => Err(EvalError::TypeMismatch(format!(
            "expected logic expression, found {other}"
        ))),
    }
}

pub fn logic_vars(expr: &Expr) -> Vec<String> {
    let mut vars = BTreeSet::new();
    collect_logic_vars(expr, &mut vars);
    vars.into_iter().collect()
}

pub fn eval_logic(expr: &Expr, env: &HashMap<&str, bool>) -> EvalResultT<bool> {
    match expr {
        Expr::Bool(b) => Ok(*b),
        Expr::Variable(name) => env
            .get(name.as_str())
            .copied()
            .ok_or_else(|| EvalError::TypeMismatch(format!("unbound logic variable '{name}'"))),
        Expr::Not(e) => Ok(!eval_logic(e, env)?),
        Expr::Logic(op, l, r) => {
            let a = eval_logic(l, env)?;
            let b = eval_logic(r, env)?;
            Ok(match op {
                LogicOp::And => a && b,
                LogicOp::Or => a || b,
                LogicOp::Xor => a ^ b,
                LogicOp::Nand => !(a && b),
                LogicOp::Nor => !(a || b),
            })
        }
        other => Err(EvalError::TypeMismatch(format!(
            "expected logic expression, found {other}"
        ))),
    }
}

fn collect_logic_vars(expr: &Expr, vars: &mut BTreeSet<String>) {
    match expr {
        Expr::Variable(name) => {
            vars.insert(name.clone());
        }
        Expr::Not(e) | Expr::Truth(e) | Expr::Circuit(e) => collect_logic_vars(e, vars),
        Expr::Logic(_, l, r) => {
            collect_logic_vars(l, vars);
            collect_logic_vars(r, vars);
        }
        _ => {}
    }
}

fn bool_digit(value: bool) -> u8 {
    if value { 1 } else { 0 }
}
