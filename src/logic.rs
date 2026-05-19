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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquivResult {
    pub equivalent: bool,
    pub vars: Vec<String>,
    pub counterexample: Option<Vec<bool>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KMap {
    pub vars: Vec<String>,
    pub row_vars: Vec<String>,
    pub col_vars: Vec<String>,
    pub rows: Vec<(String, Vec<(String, bool)>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdderResult {
    pub name: String,
    pub outputs: Vec<(String, Expr)>,
}

impl fmt::Display for KMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "KMAP {}", self.vars.join(" "))?;
        write!(f, "{}\\{}", self.row_vars.join(""), self.col_vars.join(""))?;
        for (col, _) in self.rows.first().map(|(_, cols)| cols).into_iter().flatten() {
            write!(f, " {col}")?;
        }
        writeln!(f)?;
        for (row, cols) in &self.rows {
            write!(f, "{row}")?;
            for (_, value) in cols {
                write!(f, " {}", bool_digit(*value))?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

impl fmt::Display for AdderResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.name)?;
        for (name, expr) in &self.outputs {
            writeln!(f, "{name} = {expr}")?;
        }
        Ok(())
    }
}

impl fmt::Display for EquivResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.equivalent {
            return write!(f, "true");
        }
        write!(f, "false counterexample:")?;
        if let Some(values) = &self.counterexample {
            for (name, value) in self.vars.iter().zip(values) {
                write!(f, " {name}={}", bool_digit(*value))?;
            }
        }
        Ok(())
    }
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

pub fn simplify_logic(expr: &Expr) -> EvalResultT<Expr> {
    validate_logic_expr(expr)?;
    Ok(simplify_logic_inner(expr))
}

pub fn equivalence(left: &Expr, right: &Expr) -> EvalResultT<EquivResult> {
    validate_logic_expr(left)?;
    validate_logic_expr(right)?;
    let vars = merged_vars(left, right);
    if vars.len() > 8 {
        return Err(EvalError::ExpressionTooLarge);
    }
    let count = 1usize << vars.len();
    for mask in 0..count {
        let mut env = HashMap::new();
        let mut values = Vec::new();
        for (i, name) in vars.iter().enumerate() {
            let value = ((mask >> (vars.len() - i - 1)) & 1) == 1;
            env.insert(name.as_str(), value);
            values.push(value);
        }
        if eval_logic(left, &env)? != eval_logic(right, &env)? {
            return Ok(EquivResult {
                equivalent: false,
                vars,
                counterexample: Some(values),
            });
        }
    }
    Ok(EquivResult {
        equivalent: true,
        vars,
        counterexample: None,
    })
}

pub fn kmap(explicit_vars: &[String], expr: &Expr) -> EvalResultT<KMap> {
    validate_logic_expr(expr)?;
    let vars = if explicit_vars.is_empty() {
        logic_vars(expr)
    } else {
        explicit_vars.to_vec()
    };
    if vars.is_empty() || vars.len() > 4 {
        return Err(EvalError::TypeMismatch(
            "kmap expects one to four variables".into(),
        ));
    }
    let row_count = vars.len() / 2;
    let row_vars = vars[..row_count].to_vec();
    let col_vars = vars[row_count..].to_vec();
    let row_labels = gray_labels(row_vars.len());
    let col_labels = gray_labels(col_vars.len());
    let mut rows = Vec::new();
    for row in &row_labels {
        let mut cols = Vec::new();
        for col in &col_labels {
            let bits = format!("{row}{col}");
            let mut env = HashMap::new();
            for (name, bit) in vars.iter().zip(bits.chars()) {
                env.insert(name.as_str(), bit == '1');
            }
            cols.push((col.clone(), eval_logic(expr, &env)?));
        }
        rows.push((row.clone(), cols));
    }
    Ok(KMap {
        vars,
        row_vars,
        col_vars,
        rows,
    })
}

pub fn adder_preset(name: &str, outputs: Vec<(String, Expr)>) -> EvalResultT<AdderResult> {
    let outputs = outputs
        .into_iter()
        .map(|(name, expr)| simplify_logic(&expr).map(|expr| (name, expr)))
        .collect::<EvalResultT<Vec<_>>>()?;
    Ok(AdderResult {
        name: name.into(),
        outputs,
    })
}

fn gray_labels(width: usize) -> Vec<String> {
    match width {
        0 => vec!["".into()],
        1 => vec!["0".into(), "1".into()],
        2 => vec!["00".into(), "01".into(), "11".into(), "10".into()],
        _ => Vec::new(),
    }
}

fn merged_vars(left: &Expr, right: &Expr) -> Vec<String> {
    let mut vars = BTreeSet::new();
    collect_logic_vars(left, &mut vars);
    collect_logic_vars(right, &mut vars);
    vars.into_iter().collect()
}

fn simplify_logic_inner(expr: &Expr) -> Expr {
    match expr {
        Expr::Bool(_) | Expr::Variable(_) => expr.clone(),
        Expr::Not(e) => simplify_not(simplify_logic_inner(e)),
        Expr::Logic(op, l, r) => {
            let left = simplify_logic_inner(l);
            let right = simplify_logic_inner(r);
            simplify_logic_node(*op, left, right)
        }
        _ => expr.clone(),
    }
}

fn simplify_not(expr: Expr) -> Expr {
    match expr {
        Expr::Bool(b) => Expr::Bool(!b),
        Expr::Not(inner) => *inner,
        Expr::Logic(LogicOp::And, l, r) => simplify_logic_node(
            LogicOp::Or,
            simplify_not(*l),
            simplify_not(*r),
        ),
        Expr::Logic(LogicOp::Or, l, r) => simplify_logic_node(
            LogicOp::And,
            simplify_not(*l),
            simplify_not(*r),
        ),
        other => Expr::not(other),
    }
}

fn simplify_logic_node(op: LogicOp, left: Expr, right: Expr) -> Expr {
    if let (Expr::Bool(a), Expr::Bool(b)) = (&left, &right) {
        let mut env = HashMap::new();
        env.insert("a", *a);
        env.insert("b", *b);
        return Expr::Bool(
            eval_logic(
                &Expr::logic(op, Expr::Variable("a".into()), Expr::Variable("b".into())),
                &env,
            )
            .expect("boolean operands are valid"),
        );
    }
    match op {
        LogicOp::And => match (&left, &right) {
            (Expr::Bool(false), _) | (_, Expr::Bool(false)) => Expr::Bool(false),
            (Expr::Bool(true), _) => right,
            (_, Expr::Bool(true)) => left,
            _ if left == right => left,
            _ => Expr::logic(op, left, right),
        },
        LogicOp::Or => match (&left, &right) {
            (Expr::Bool(true), _) | (_, Expr::Bool(true)) => Expr::Bool(true),
            (Expr::Bool(false), _) => right,
            (_, Expr::Bool(false)) => left,
            _ if left == right => left,
            _ => Expr::logic(op, left, right),
        },
        LogicOp::Xor => match (&left, &right) {
            (Expr::Bool(false), _) => right,
            (_, Expr::Bool(false)) => left,
            (Expr::Bool(true), _) => simplify_not(right),
            (_, Expr::Bool(true)) => simplify_not(left),
            _ if left == right => Expr::Bool(false),
            _ => Expr::logic(op, left, right),
        },
        LogicOp::Nand => simplify_not(simplify_logic_node(LogicOp::And, left, right)),
        LogicOp::Nor => simplify_not(simplify_logic_node(LogicOp::Or, left, right)),
    }
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
        Expr::Not(e)
        | Expr::Truth(e)
        | Expr::Circuit(e)
        | Expr::LogicSimplify(e)
        | Expr::KMap(_, e) => {
            collect_logic_vars(e, vars)
        }
        Expr::Equiv(l, r) => {
            collect_logic_vars(l, vars);
            collect_logic_vars(r, vars);
        }
        Expr::HalfAdder(l, r) => {
            collect_logic_vars(l, vars);
            collect_logic_vars(r, vars);
        }
        Expr::FullAdder(a, b, c) => {
            collect_logic_vars(a, vars);
            collect_logic_vars(b, vars);
            collect_logic_vars(c, vars);
        }
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
