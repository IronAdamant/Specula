//! Tiny arithmetic expressions for recipe numbers: `"2.4 + 0.4 * (tier - 1)"`,
//! `"tier >= 2 ? 1 : 0"`, `"max(0.05, w * 0.06)"`.
//!
//! Grammar (usual precedence, left associative):
//!
//! ```text
//! expr    = or ( "?" expr ":" expr )?
//! or      = and ( "||" and )*
//! and     = cmp ( "&&" cmp )*
//! cmp     = sum ( ("<" | "<=" | ">" | ">=" | "==" | "!=") sum )?
//! sum     = product ( ("+" | "-") product )*
//! product = unary ( ("*" | "/" | "%") unary )*
//! unary   = ("-" | "!") unary | atom
//! atom    = number | name | name "(" expr ("," expr)* ")" | "(" expr ")"
//! ```
//!
//! Functions: `min max clamp abs floor ceil round lerp sqrt sin cos` (degrees). Constant `pi`.
//! Booleans are 1 and 0. Evaluated in `f64` with deterministic maths only.

use crate::dmath;

/// Longest expression text accepted.
pub const MAX_EXPR_BYTES: usize = 1024;
/// Most nodes in one expression (numbers, names, operators, calls).
pub const MAX_EXPR_NODES: usize = 256;

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Num(f64),
    Var(String),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(Func, Vec<Expr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Func {
    Min,
    Max,
    Clamp,
    Abs,
    Floor,
    Ceil,
    Round,
    Lerp,
    Sqrt,
    Sin,
    Cos,
}

impl Func {
    fn parse(name: &str) -> Option<(Func, usize)> {
        Some(match name {
            "min" => (Func::Min, 2),
            "max" => (Func::Max, 2),
            "clamp" => (Func::Clamp, 3),
            "abs" => (Func::Abs, 1),
            "floor" => (Func::Floor, 1),
            "ceil" => (Func::Ceil, 1),
            "round" => (Func::Round, 1),
            "lerp" => (Func::Lerp, 3),
            "sqrt" => (Func::Sqrt, 1),
            "sin" => (Func::Sin, 1),
            "cos" => (Func::Cos, 1),
            _ => return None,
        })
    }
}

/// A syntax error at a byte offset into the expression text.
#[derive(Clone, Debug, PartialEq)]
pub struct ExprError {
    pub offset: usize,
    pub message: String,
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn err<T>(&self, message: impl Into<String>) -> Result<T, ExprError> {
        Err(ExprError { offset: self.i, message: message.into() })
    }

    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn eat(&mut self, tok: &str) -> bool {
        self.ws();
        if self.s[self.i..].starts_with(tok.as_bytes()) {
            self.i += tok.len();
            true
        } else {
            false
        }
    }

    fn expr(&mut self, depth: u32) -> Result<Expr, ExprError> {
        if depth > 64 {
            return self.err("expression nested too deeply");
        }
        let c = self.or(depth)?;
        if self.eat("?") {
            let a = self.expr(depth + 1)?;
            if !self.eat(":") {
                return self.err("expected ':' in conditional");
            }
            let b = self.expr(depth + 1)?;
            return Ok(Expr::If(Box::new(c), Box::new(a), Box::new(b)));
        }
        Ok(c)
    }

    fn or(&mut self, d: u32) -> Result<Expr, ExprError> {
        let mut l = self.and(d)?;
        while self.eat("||") {
            let r = self.and(d)?;
            l = Expr::Bin(Op::Or, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn and(&mut self, d: u32) -> Result<Expr, ExprError> {
        let mut l = self.cmp(d)?;
        while self.eat("&&") {
            let r = self.cmp(d)?;
            l = Expr::Bin(Op::And, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn cmp(&mut self, d: u32) -> Result<Expr, ExprError> {
        let l = self.sum(d)?;
        for (tok, op) in [("<=", Op::Le), (">=", Op::Ge), ("==", Op::Eq), ("!=", Op::Ne), ("<", Op::Lt), (">", Op::Gt)] {
            if self.eat(tok) {
                let r = self.sum(d)?;
                return Ok(Expr::Bin(op, Box::new(l), Box::new(r)));
            }
        }
        Ok(l)
    }

    fn sum(&mut self, d: u32) -> Result<Expr, ExprError> {
        let mut l = self.product(d)?;
        loop {
            let op = if self.eat("+") {
                Op::Add
            } else if self.eat("-") {
                Op::Sub
            } else {
                return Ok(l);
            };
            let r = self.product(d)?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
    }

    fn product(&mut self, d: u32) -> Result<Expr, ExprError> {
        let mut l = self.unary(d)?;
        loop {
            let op = if self.eat("*") {
                Op::Mul
            } else if self.eat("/") {
                Op::Div
            } else if self.eat("%") {
                Op::Rem
            } else {
                return Ok(l);
            };
            let r = self.unary(d)?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
    }

    fn unary(&mut self, d: u32) -> Result<Expr, ExprError> {
        if d > 64 {
            return self.err("expression nested too deeply");
        }
        self.ws();
        if self.eat("-") {
            return Ok(Expr::Neg(Box::new(self.unary(d + 1)?)));
        }
        if self.s[self.i..].starts_with(b"!") && !self.s[self.i..].starts_with(b"!=") {
            self.i += 1;
            return Ok(Expr::Not(Box::new(self.unary(d + 1)?)));
        }
        self.atom(d)
    }

    fn atom(&mut self, d: u32) -> Result<Expr, ExprError> {
        self.ws();
        let start = self.i;
        let Some(&c) = self.s.get(self.i) else { return self.err("unexpected end of expression") };
        if c == b'(' {
            self.i += 1;
            let e = self.expr(d + 1)?;
            if !self.eat(")") {
                return self.err("expected ')'");
            }
            return Ok(e);
        }
        if c.is_ascii_digit() || c == b'.' {
            while self.i < self.s.len() && (self.s[self.i].is_ascii_digit() || self.s[self.i] == b'.') {
                self.i += 1;
            }
            if self.i < self.s.len() && (self.s[self.i] == b'e' || self.s[self.i] == b'E') {
                self.i += 1;
                if self.i < self.s.len() && (self.s[self.i] == b'+' || self.s[self.i] == b'-') {
                    self.i += 1;
                }
                while self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
                    self.i += 1;
                }
            }
            let text = std::str::from_utf8(&self.s[start..self.i]).unwrap_or("");
            return match text.parse::<f64>() {
                Ok(v) => Ok(Expr::Num(v)),
                Err(_) => {
                    self.i = start;
                    self.err(format!("bad number '{text}'"))
                }
            };
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            while self.i < self.s.len() && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_') {
                self.i += 1;
            }
            let name = std::str::from_utf8(&self.s[start..self.i]).unwrap_or("").to_string();
            if self.eat("(") {
                let Some((f, arity)) = Func::parse(&name) else {
                    self.i = start;
                    return self.err(format!("unknown function '{name}'"));
                };
                let mut args = vec![self.expr(d + 1)?];
                while self.eat(",") {
                    args.push(self.expr(d + 1)?);
                }
                if !self.eat(")") {
                    return self.err("expected ')' after arguments");
                }
                if args.len() != arity {
                    self.i = start;
                    return self.err(format!("{name}() takes {arity} argument(s), got {}", args.len()));
                }
                return Ok(Expr::Call(f, args));
            }
            if name == "pi" {
                return Ok(Expr::Num(dmath::PI));
            }
            return Ok(Expr::Var(name));
        }
        self.err(format!("unexpected '{}'", c as char))
    }
}

impl Expr {
    /// Parses `text`. Expressions are capped at [`MAX_EXPR_BYTES`] bytes and [`MAX_EXPR_NODES`]
    /// nodes, which also bounds the tree depth, so evaluating or dropping a hostile expression
    /// can't overflow the stack.
    pub fn parse(text: &str) -> Result<Expr, ExprError> {
        if text.len() > MAX_EXPR_BYTES {
            return Err(ExprError { offset: MAX_EXPR_BYTES, message: format!("expression longer than {MAX_EXPR_BYTES} bytes") });
        }
        let mut p = Parser { s: text.as_bytes(), i: 0 };
        let e = p.expr(0)?;
        p.ws();
        if p.i != p.s.len() {
            return p.err("unexpected trailing characters");
        }
        let n = e.node_count();
        if n > MAX_EXPR_NODES {
            return Err(ExprError { offset: 0, message: format!("expression has {n} nodes; at most {MAX_EXPR_NODES}") });
        }
        Ok(e)
    }

    /// Number of nodes in the tree.
    pub fn node_count(&self) -> usize {
        1 + match self {
            Expr::Num(_) | Expr::Var(_) => 0,
            Expr::Neg(a) | Expr::Not(a) => a.node_count(),
            Expr::Bin(_, a, b) => a.node_count() + b.node_count(),
            Expr::If(c, a, b) => c.node_count() + a.node_count() + b.node_count(),
            Expr::Call(_, args) => args.iter().map(Expr::node_count).sum(),
        }
    }

    /// Every variable name used.
    pub fn vars(&self, out: &mut Vec<String>) {
        match self {
            Expr::Num(_) => {}
            Expr::Var(v) => out.push(v.clone()),
            Expr::Neg(a) | Expr::Not(a) => a.vars(out),
            Expr::Bin(_, a, b) => {
                a.vars(out);
                b.vars(out);
            }
            Expr::If(c, a, b) => {
                c.vars(out);
                a.vars(out);
                b.vars(out);
            }
            Expr::Call(_, args) => args.iter().for_each(|a| a.vars(out)),
        }
    }

    /// Evaluates with `lookup` for variables (the caller has checked they exist).
    pub fn eval(&self, lookup: &dyn Fn(&str) -> Option<f64>) -> f64 {
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        match self {
            Expr::Num(v) => *v,
            Expr::Var(name) => lookup(name).unwrap_or(f64::NAN),
            Expr::Neg(a) => -a.eval(lookup),
            Expr::Not(a) => b(a.eval(lookup) == 0.0),
            Expr::Bin(op, l, r) => {
                let x = l.eval(lookup);
                // Short-circuit the logical operators.
                match op {
                    Op::And => return b(x != 0.0 && r.eval(lookup) != 0.0),
                    Op::Or => return b(x != 0.0 || r.eval(lookup) != 0.0),
                    _ => {}
                }
                let y = r.eval(lookup);
                match op {
                    Op::Add => x + y,
                    Op::Sub => x - y,
                    Op::Mul => x * y,
                    Op::Div => x / y,
                    Op::Rem => x - y * dmath::floor_f64(x / y),
                    Op::Lt => b(x < y),
                    Op::Le => b(x <= y),
                    Op::Gt => b(x > y),
                    Op::Ge => b(x >= y),
                    Op::Eq => b(x == y),
                    Op::Ne => b(x != y),
                    Op::And | Op::Or => unreachable!(),
                }
            }
            Expr::If(c, a, e) => {
                if c.eval(lookup) != 0.0 {
                    a.eval(lookup)
                } else {
                    e.eval(lookup)
                }
            }
            Expr::Call(f, args) => {
                let v: Vec<f64> = args.iter().map(|a| a.eval(lookup)).collect();
                match f {
                    Func::Min => v[0].min(v[1]),
                    Func::Max => v[0].max(v[1]),
                    Func::Clamp => v[0].max(v[1]).min(v[2]),
                    Func::Abs => v[0].abs(),
                    Func::Floor => dmath::floor_f64(v[0]),
                    Func::Ceil => dmath::ceil_f64(v[0]),
                    Func::Round => dmath::round_f64(v[0]),
                    Func::Lerp => v[0] + (v[1] - v[0]) * v[2],
                    Func::Sqrt => v[0].sqrt(),
                    Func::Sin => dmath::sin_cos_deg(v[0]).0,
                    Func::Cos => dmath::sin_cos_deg(v[0]).1,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(s: &str) -> f64 {
        Expr::parse(s).unwrap().eval(&|n| match n {
            "tier" => Some(2.0),
            "h" => Some(2.4),
            _ => None,
        })
    }

    #[test]
    fn precedence_and_functions() {
        assert_eq!(ev("1 + 2 * 3"), 7.0);
        assert_eq!(ev("(1 + 2) * 3"), 9.0);
        assert_eq!(ev("-2 * -3"), 6.0);
        assert_eq!(ev("10 - 4 - 3"), 3.0);
        assert_eq!(ev("7 % 3"), 1.0);
        assert_eq!(ev("tier >= 2 ? 1.5 : 0"), 1.5);
        assert_eq!(ev("tier > 2 || h > 2"), 1.0);
        assert_eq!(ev("tier > 2 && h > 2"), 0.0);
        assert_eq!(ev("!(tier == 2)"), 0.0);
        assert_eq!(ev("tier != 3"), 1.0);
        assert_eq!(ev("max(1, min(5, 3))"), 3.0);
        assert_eq!(ev("clamp(9, 0, 4)"), 4.0);
        assert_eq!(ev("lerp(2, 4, 0.5)"), 3.0);
        assert_eq!(ev("round(2.5) + floor(-0.5) + ceil(0.2)"), 3.0 - 1.0 + 1.0);
        assert_eq!(ev("sin(90) + cos(180)"), 0.0);
        assert!((ev("h * 0.5 + 1e-1") - 1.3).abs() < 1e-12);
        assert!((ev("pi") - std::f64::consts::PI).abs() < 1e-15);
    }

    #[test]
    fn errors_have_offsets() {
        let e = Expr::parse("1 + * 2").unwrap_err();
        assert_eq!(e.offset, 4);
        assert!(Expr::parse("foo(1)").unwrap_err().message.contains("unknown function"));
        assert!(Expr::parse("min(1)").unwrap_err().message.contains("takes 2"));
        assert!(Expr::parse("(1 + 2").is_err());
        assert!(Expr::parse("1 2").is_err());
        assert!(Expr::parse("").is_err());
        // Hostile sizes are structured errors, not stack overflows.
        let chain = format!("1{}", "+1".repeat(20_000));
        assert!(Expr::parse(&chain).unwrap_err().message.contains("longer than"));
        let chain = format!("1{}", "+1".repeat(300));
        assert!(Expr::parse(&chain).unwrap_err().message.contains("nodes"));
        let ok = format!("1{}", "+1".repeat(120));
        assert_eq!(ev(&ok), 121.0);
        let deep = format!("{}1{}", "(".repeat(200), ")".repeat(200));
        assert!(Expr::parse(&deep).is_err());
        let mut v = Vec::new();
        Expr::parse("a + b * c(1)").map(|e| e.vars(&mut v)).unwrap_err();
        Expr::parse("a + max(b, 2)").unwrap().vars(&mut v);
        assert_eq!(v, ["a", "b"]);
    }
}
