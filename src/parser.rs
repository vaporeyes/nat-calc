// ABOUTME: Pratt (precedence-climbing) parser producing an Expr AST.
// ABOUTME: Handles assignment, binary ops, unary minus, calls, matrices.

use crate::ast::{BinaryOp, Expr, Func, LogicOp};
use crate::error::{EvalError, EvalResultT};
use crate::lexer::{Token, lex};
use bigdecimal::BigDecimal;
use std::str::FromStr;

pub fn parse(src: &str) -> EvalResultT<Expr> {
    let tokens = lex(src)?;
    let mut p = Parser { tokens, pos: 0 };
    let expr = p.parse_statement()?;
    p.expect(Token::Eof)?;
    Ok(expr)
}

/// Names with dedicated call parsing (commands + transcendental functions).
fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "simplify" | "expand" | "derive" | "truth" | "circuit" | "logic_simplify" | "equiv"
            | "kmap" | "half_adder" | "full_adder" | "sin" | "cos" | "tan" | "exp" | "ln"
    )
}

fn logic_op(name: &str) -> Option<LogicOp> {
    match name {
        "and" => Some(LogicOp::And),
        "or" => Some(LogicOp::Or),
        "xor" => Some(LogicOp::Xor),
        "nand" => Some(LogicOp::Nand),
        "nor" => Some(LogicOp::Nor),
        _ => None,
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn next(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        self.pos += 1;
        t
    }

    fn expect(&mut self, want: Token) -> EvalResultT<()> {
        if *self.peek() == want {
            self.pos += 1;
            Ok(())
        } else {
            Err(EvalError::Parse(format!(
                "expected {want:?}, found {:?}",
                self.peek()
            )))
        }
    }

    /// statement := IDENT '=' expr | expr
    fn parse_statement(&mut self) -> EvalResultT<Expr> {
        if let (Token::Ident(name), Token::Equals) =
            (&self.tokens[self.pos], &self.tokens[self.pos + 1])
        {
            let name = name.clone();
            self.pos += 2; // consume IDENT '='
            let rhs = self.parse_expr(0)?;
            return Ok(Expr::Assign(name, Box::new(rhs)));
        }
        self.parse_expr(0)
    }

    /// Precedence-climbing. `min_bp` is the minimum binding power that may
    /// bind the next infix operator.
    fn parse_expr(&mut self, min_bp: u8) -> EvalResultT<Expr> {
        let mut lhs = self.parse_prefix()?;

        loop {
            let (op, l_bp, r_bp) = match self.peek() {
                Token::Plus => (BinaryOp::Add, 10, 11),
                Token::Minus => (BinaryOp::Sub, 10, 11),
                Token::Star => (BinaryOp::Mul, 20, 21),
                Token::Slash => (BinaryOp::Div, 20, 21),
                // Right-associative: left bp > right bp.
                Token::Caret => (BinaryOp::Pow, 31, 30),
                Token::Ident(name) if logic_op(name).is_some() => {
                    let op = logic_op(name).expect("guarded by is_some");
                    let (l_bp, r_bp) = match op {
                        LogicOp::And | LogicOp::Nand => (8, 9),
                        LogicOp::Xor => (6, 7),
                        LogicOp::Or | LogicOp::Nor => (4, 5),
                    };
                    if l_bp < min_bp {
                        break;
                    }
                    self.pos += 1;
                    let rhs = self.parse_expr(r_bp)?;
                    lhs = Expr::logic(op, lhs, rhs);
                    continue;
                }
                _ => break,
            };
            if l_bp < min_bp {
                break;
            }
            self.pos += 1; // consume operator
            let rhs = self.parse_expr(r_bp)?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// Prefix position: lambda, unary minus, or an applied primary.
    fn parse_prefix(&mut self) -> EvalResultT<Expr> {
        if *self.peek() == Token::Backslash {
            return self.parse_lambda();
        }
        if *self.peek() == Token::Minus {
            self.pos += 1;
            // Unary minus binds tighter than +,-,*,/ but looser than ^.
            let operand = self.parse_expr(25)?;
            return Ok(Expr::Neg(Box::new(operand)));
        }
        if let Token::Ident(name) = self.peek()
            && name == "not"
        {
            self.pos += 1;
            let operand = self.parse_expr(25)?;
            return Ok(Expr::not(operand));
        }
        self.parse_postfix()
    }

    /// `\x. e` and the multi-parameter sugar `\x y z. e`, which desugars to
    /// nested single-parameter lambdas (right-associative).
    fn parse_lambda(&mut self) -> EvalResultT<Expr> {
        self.expect(Token::Backslash)?;
        let mut params = Vec::new();
        while let Token::Ident(_) = self.peek() {
            if let Token::Ident(p) = self.next() {
                params.push(p);
            }
        }
        if params.is_empty() {
            return Err(EvalError::Parse(
                "lambda needs at least one parameter".into(),
            ));
        }
        self.expect(Token::Dot)?;
        let body = self.parse_expr(0)?;
        let mut e = body;
        for p in params.into_iter().rev() {
            e = Expr::Lambda(p, Box::new(e));
        }
        Ok(e)
    }

    /// Application is a left-associative postfix: `f(a)`, `f(a)(b)`, and the
    /// multi-argument sugar `f(a, b)` == `f(a)(b)`. Binds tighter than any
    /// binary operator.
    fn parse_postfix(&mut self) -> EvalResultT<Expr> {
        let mut e = self.parse_primary()?;
        while *self.peek() == Token::LParen {
            self.pos += 1; // consume '('
            loop {
                let arg = self.parse_expr(0)?;
                e = Expr::Apply(Box::new(e), Box::new(arg));
                match self.next() {
                    Token::Comma => continue,
                    Token::RParen => break,
                    other => {
                        return Err(EvalError::Parse(format!(
                            "expected ',' or ')' in application, found {other:?}"
                        )));
                    }
                }
            }
        }
        Ok(e)
    }

    fn parse_primary(&mut self) -> EvalResultT<Expr> {
        match self.next() {
            Token::Number(s) => {
                let n = BigDecimal::from_str(&s)
                    .map_err(|_| EvalError::Parse(format!("invalid number '{s}'")))?;
                Ok(Expr::Number(n.normalized()))
            }
            Token::Ident(name) => {
                if name == "true" {
                    return Ok(Expr::Bool(true));
                }
                if name == "false" {
                    return Ok(Expr::Bool(false));
                }
                // Built-in functions/commands keep their dedicated parsing;
                // any other `name(...)` is generic application handled by
                // `parse_postfix`, so just yield a variable here.
                if *self.peek() == Token::LParen && is_builtin(&name) {
                    self.parse_call(&name)
                } else {
                    Ok(Expr::Variable(name))
                }
            }
            Token::LParen => {
                let e = self.parse_expr(0)?;
                self.expect(Token::RParen)?;
                Ok(e)
            }
            Token::LBracket => self.parse_matrix(),
            other => Err(EvalError::Parse(format!(
                "unexpected token {other:?} in expression"
            ))),
        }
    }

    /// A call site: either a known function/command or a generic call.
    fn parse_call(&mut self, name: &str) -> EvalResultT<Expr> {
        self.expect(Token::LParen)?;

        if name == "derive" {
            // derive(<var>, <expr>)
            let var = match self.next() {
                Token::Ident(v) => v,
                other => {
                    return Err(EvalError::Parse(format!(
                        "derive expects a variable as first argument, found {other:?}"
                    )));
                }
            };
            self.expect(Token::Comma)?;
            let body = self.parse_expr(0)?;
            self.expect(Token::RParen)?;
            return Ok(Expr::Derive(var, Box::new(body)));
        }

        if name == "equiv" {
            let left = self.parse_expr(0)?;
            self.expect(Token::Comma)?;
            let right = self.parse_expr(0)?;
            self.expect(Token::RParen)?;
            return Ok(Expr::Equiv(Box::new(left), Box::new(right)));
        }

        if name == "kmap" {
            let args = self.parse_arg_list()?;
            if args.is_empty() {
                return Err(EvalError::Parse("kmap expects an expression".into()));
            }
            if args.len() == 1 {
                return Ok(Expr::KMap(Vec::new(), Box::new(args[0].clone())));
            }
            let mut vars = Vec::new();
            for arg in &args[..args.len() - 1] {
                if let Expr::Variable(name) = arg {
                    vars.push(name.clone());
                } else {
                    return Err(EvalError::Parse(
                        "kmap variable arguments must be names".into(),
                    ));
                }
            }
            return Ok(Expr::KMap(vars, Box::new(args[args.len() - 1].clone())));
        }

        if name == "half_adder" {
            let args = self.parse_arg_list()?;
            if args.len() != 2 {
                return Err(EvalError::Parse("half_adder expects two inputs".into()));
            }
            return Ok(Expr::HalfAdder(
                Box::new(args[0].clone()),
                Box::new(args[1].clone()),
            ));
        }

        if name == "full_adder" {
            let args = self.parse_arg_list()?;
            if args.len() != 3 {
                return Err(EvalError::Parse("full_adder expects three inputs".into()));
            }
            return Ok(Expr::FullAdder(
                Box::new(args[0].clone()),
                Box::new(args[1].clone()),
                Box::new(args[2].clone()),
            ));
        }

        let arg = self.parse_expr(0)?;
        self.expect(Token::RParen)?;

        let built = match name {
            "simplify" => Expr::Simplify(Box::new(arg)),
            "expand" => Expr::Expand(Box::new(arg)),
            "truth" => Expr::Truth(Box::new(arg)),
            "circuit" => Expr::Circuit(Box::new(arg)),
            "logic_simplify" => Expr::LogicSimplify(Box::new(arg)),
            "sin" => Expr::Call(Func::Sin, Box::new(arg)),
            "cos" => Expr::Call(Func::Cos, Box::new(arg)),
            "tan" => Expr::Call(Func::Tan, Box::new(arg)),
            "exp" => Expr::Call(Func::Exp, Box::new(arg)),
            "ln" => Expr::Call(Func::Ln, Box::new(arg)),
            other => {
                return Err(EvalError::Parse(format!("unknown function '{other}'")));
            }
        };
        Ok(built)
    }

    fn parse_arg_list(&mut self) -> EvalResultT<Vec<Expr>> {
        let mut args = Vec::new();
        loop {
            args.push(self.parse_expr(0)?);
            match self.next() {
                Token::Comma => continue,
                Token::RParen => return Ok(args),
                other => {
                    return Err(EvalError::Parse(format!(
                        "expected ',' or ')' in argument list, found {other:?}"
                    )));
                }
            }
        }
    }

    /// Matrix literal: `[a, b; c, d]`. Rows separated by `;`, columns by `,`.
    fn parse_matrix(&mut self) -> EvalResultT<Expr> {
        let mut rows: Vec<Vec<Expr>> = Vec::new();
        let mut row: Vec<Expr> = Vec::new();

        if *self.peek() == Token::RBracket {
            self.pos += 1;
            return Err(EvalError::Parse("matrix cannot be empty".into()));
        }

        loop {
            row.push(self.parse_expr(0)?);
            match self.next() {
                Token::Comma => {}
                Token::Semicolon => {
                    rows.push(std::mem::take(&mut row));
                }
                Token::RBracket => {
                    rows.push(row);
                    break;
                }
                other => {
                    return Err(EvalError::Parse(format!(
                        "expected ',' ';' or ']' in matrix, found {other:?}"
                    )));
                }
            }
        }

        let width = rows[0].len();
        if rows.iter().any(|r| r.len() != width) {
            return Err(EvalError::Parse(
                "matrix rows have inconsistent lengths".into(),
            ));
        }
        Ok(Expr::Matrix(rows))
    }
}
