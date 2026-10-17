//! Expressions: short typed strings such as `"res.dust >= arg.stone.cost"`.
//!
//! Parsed into an [`Ast`], then type-checked against a [`Scope`] (which names exist here, what
//! units attributes have) into an [`Ex`], constant-folded. Evaluation lives in the simulation,
//! which owns the state the names refer to.
//!
//! Grammar (lowest precedence first):
//!
//! ```text
//! or    := and ('||' and)*
//! and   := cmp ('&&' cmp)*
//! cmp   := sum (('==' | '!=' | '<' | '<=' | '>' | '>=') sum)?
//! sum   := prod (('+' | '-') prod)*
//! prod  := unary (('*' | '/' | '%') unary)*
//! unary := ('-' | '!') unary | post
//! post  := prim ('.' ident)*
//! prim  := number[suffix] | true | false | $param | ident ('.' ident)* | ident '(' args ')' | '(' or ')'
//! ```
//!
//! Units: `+ - %` and comparisons need the same unit on both sides (a bare `0` matches any);
//! `*` and `/` by a plain number keep the unit; `* ratio` scales (`x * 35%` = `x * 350 / 1000`);
//! `u / u` is a plain number. Anything else is a `unit_mismatch` with a suggestion.

use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::diag::{closest, codes, did_you_mean};
use crate::units::{parse_dec, scale, suffix_unit, Dec, Unit, SUFFIXES};

// ---- tokens and syntax ----

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num { text: String, suffix: String },
    Ident(String),
    Param(String),
    Op(&'static str),
    LParen,
    RParen,
    Comma,
    Dot,
    End,
}

const OPS: [&str; 15] = ["&&", "||", "==", "!=", "<=", ">=", "<", ">", "+", "-", "*", "/", "%", "!", "="];

fn lex(src: &str) -> Result<Vec<(Tok, usize)>, ExprError> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let col = i + 1;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            // A suffix touches the number: 7m, 2.2m/s, 35%, 1.5x, 10pt, 0.8s, 24t.
            let rest: String = chars[i..].iter().collect();
            let mut suffix = String::new();
            for s in ["m/s", "pt", "m", "s", "t", "%", "x"] {
                if let Some(tail) = rest.strip_prefix(s) {
                    let after = tail.chars().next();
                    if after.is_none_or(|a| !(a.is_ascii_alphanumeric() || a == '_')) {
                        suffix = s.to_owned();
                        break;
                    }
                }
            }
            if suffix.is_empty() && rest.chars().next().is_some_and(|a| a.is_ascii_alphabetic() || a == '_') {
                let word: String = rest.chars().take_while(|a| a.is_ascii_alphanumeric() || *a == '_').collect();
                return Err(ExprError::new(codes::SYNTAX, format!("'{text}{word}' is not a number with a unit"), col)
                    .expected(format!("a unit suffix: {}", SUFFIXES.join(", ")))
                    .got(format!("{text}{word}"))
                    .suggest_opt(closest(&word, SUFFIXES).map(|s| format!("did you mean '{text}{s}'?"))));
            }
            i += suffix.chars().count();
            out.push((Tok::Num { text, suffix }, col));
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' || c == '$' {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if let Some(p) = word.strip_prefix('$') {
                if p.is_empty() {
                    return Err(ExprError::new(codes::SYNTAX, "'$' must be followed by a parameter name", col).suggest("write $name"));
                }
                out.push((Tok::Param(p.to_owned()), col));
            } else {
                out.push((Tok::Ident(word), col));
            }
            continue;
        }
        match c {
            '(' => out.push((Tok::LParen, col)),
            ')' => out.push((Tok::RParen, col)),
            ',' => out.push((Tok::Comma, col)),
            '.' => out.push((Tok::Dot, col)),
            _ => {
                let two: String = chars[i..chars.len().min(i + 2)].iter().collect();
                let op = OPS.iter().find(|o| two.starts_with(**o)).copied();
                match op {
                    Some("=") => {
                        return Err(ExprError::new(codes::SYNTAX, "'=' is not an operator", col).suggest("use '==' to compare"));
                    }
                    Some(o) => {
                        out.push((Tok::Op(o), col));
                        i += o.len();
                        continue;
                    }
                    None => {
                        let hint = match c {
                            '&' => Some("use '&&' for 'and'".to_owned()),
                            '|' => Some("use '||' for 'or'".to_owned()),
                            '"' | '\'' => Some("names and tags are written without quotes".to_owned()),
                            _ => None,
                        };
                        return Err(ExprError::new(codes::SYNTAX, format!("unexpected character '{c}'"), col).suggest_opt(hint));
                    }
                }
            }
        }
        i += 1;
    }
    out.push((Tok::End, chars.len() + 1));
    Ok(out)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Ast {
    Num { dec: Dec, suffix: String, text: String, col: usize },
    Bool(bool, usize),
    /// `self.hp`, `arg.stone.next.cost`, `drops`.
    Name { parts: Vec<String>, col: usize },
    Param { name: String, col: usize },
    Member { base: Box<Ast>, name: String, col: usize },
    Neg(Box<Ast>, usize),
    Not(Box<Ast>, usize),
    Bin { op: BinOp, l: Box<Ast>, r: Box<Ast>, col: usize },
    Call { name: String, args: Vec<Ast>, col: usize },
}

impl Ast {
    pub fn col(&self) -> usize {
        match self {
            Ast::Num { col, .. }
            | Ast::Bool(_, col)
            | Ast::Name { col, .. }
            | Ast::Param { col, .. }
            | Ast::Member { col, .. }
            | Ast::Neg(_, col)
            | Ast::Not(_, col)
            | Ast::Bin { col, .. }
            | Ast::Call { col, .. } => *col,
        }
    }

    /// A unit-less number literal (for "7 has no unit" suggestions); `Some(text)`.
    fn bare_literal(&self) -> Option<&str> {
        match self {
            Ast::Num { suffix, text, .. } if suffix.is_empty() => Some(text),
            Ast::Neg(inner, _) => inner.bare_literal(),
            _ => None,
        }
    }
}

/// Longest expression string, and deepest nesting (parentheses, unary operators, operator
/// chains): expressions are short, and the bounds keep every recursive pass small.
pub const MAX_EXPR_CHARS: usize = 1000;
pub const MAX_EXPR_DEPTH: u32 = 64;

struct Parser {
    toks: Vec<(Tok, usize)>,
    i: usize,
    depth: u32,
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.i].0
    }

    fn col(&self) -> usize {
        self.toks[self.i].1
    }

    fn next(&mut self) -> (Tok, usize) {
        let t = self.toks[self.i].clone();
        if self.i + 1 < self.toks.len() {
            self.i += 1;
        }
        t
    }

    fn eat_op(&mut self, ops: &[&'static str]) -> Option<(&'static str, usize)> {
        if let Tok::Op(o) = self.peek() {
            if let Some(found) = ops.iter().find(|x| *x == o) {
                let col = self.col();
                self.next();
                return Some((found, col));
            }
        }
        None
    }

    fn nest(&mut self) -> Result<(), ExprError> {
        self.depth += 1;
        if self.depth > MAX_EXPR_DEPTH {
            return Err(ExprError::new(codes::TOO_MANY, format!("the expression nests more than {MAX_EXPR_DEPTH} deep"), self.col())
                .suggest("split it: put parts in params or attributes, or simplify"));
        }
        Ok(())
    }

    fn or(&mut self) -> Result<Ast, ExprError> {
        self.nest()?;
        let r = self.or_inner();
        self.depth -= 1;
        r
    }

    fn or_inner(&mut self) -> Result<Ast, ExprError> {
        let mut l = self.and()?;
        while let Some((_, col)) = self.eat_op(&["||"]) {
            let r = self.and()?;
            l = Ast::Bin { op: BinOp::Or, l: Box::new(l), r: Box::new(r), col };
        }
        Ok(l)
    }

    fn and(&mut self) -> Result<Ast, ExprError> {
        let mut l = self.cmp()?;
        while let Some((_, col)) = self.eat_op(&["&&"]) {
            let r = self.cmp()?;
            l = Ast::Bin { op: BinOp::And, l: Box::new(l), r: Box::new(r), col };
        }
        Ok(l)
    }

    fn cmp(&mut self) -> Result<Ast, ExprError> {
        let l = self.sum()?;
        if let Some((o, col)) = self.eat_op(&["==", "!=", "<=", ">=", "<", ">"]) {
            let op = match o {
                "==" => BinOp::Eq,
                "!=" => BinOp::Ne,
                "<=" => BinOp::Le,
                ">=" => BinOp::Ge,
                "<" => BinOp::Lt,
                _ => BinOp::Gt,
            };
            let r = self.sum()?;
            if let Tok::Op(o2 @ ("==" | "!=" | "<=" | ">=" | "<" | ">")) = self.peek() {
                return Err(ExprError::new(codes::SYNTAX, format!("comparisons don't chain ('{o2}' after '{o}')"), self.col())
                    .suggest("combine two comparisons with '&&', e.g. a < b && b < c"));
            }
            return Ok(Ast::Bin { op, l: Box::new(l), r: Box::new(r), col });
        }
        Ok(l)
    }

    fn sum(&mut self) -> Result<Ast, ExprError> {
        let mut l = self.prod()?;
        while let Some((o, col)) = self.eat_op(&["+", "-"]) {
            let r = self.prod()?;
            let op = if o == "+" { BinOp::Add } else { BinOp::Sub };
            l = Ast::Bin { op, l: Box::new(l), r: Box::new(r), col };
        }
        Ok(l)
    }

    fn prod(&mut self) -> Result<Ast, ExprError> {
        let mut l = self.unary()?;
        while let Some((o, col)) = self.eat_op(&["*", "/", "%"]) {
            let r = self.unary()?;
            let op = match o {
                "*" => BinOp::Mul,
                "/" => BinOp::Div,
                _ => BinOp::Rem,
            };
            l = Ast::Bin { op, l: Box::new(l), r: Box::new(r), col };
        }
        Ok(l)
    }

    fn unary(&mut self) -> Result<Ast, ExprError> {
        if let Some((o, col)) = self.eat_op(&["-", "!"]) {
            self.nest()?;
            let e = self.unary();
            self.depth -= 1;
            let e = e?;
            return Ok(if o == "-" { Ast::Neg(Box::new(e), col) } else { Ast::Not(Box::new(e), col) });
        }
        let mut e = self.prim()?;
        while *self.peek() == Tok::Dot {
            self.next();
            let (t, col) = self.next();
            let Tok::Ident(name) = t else {
                return Err(ExprError::new(codes::SYNTAX, "expected a name after '.'", col).suggest("e.g. self.hp"));
            };
            e = match e {
                Ast::Name { mut parts, col: c } => {
                    parts.push(name);
                    Ast::Name { parts, col: c }
                }
                other => Ast::Member { base: Box::new(other), name, col },
            };
        }
        Ok(e)
    }

    fn prim(&mut self) -> Result<Ast, ExprError> {
        let (t, col) = self.next();
        match t {
            Tok::Num { text, suffix } => {
                let dec = parse_dec(&text).ok_or_else(|| ExprError::new(codes::SYNTAX, format!("'{text}' is not a number"), col))?;
                Ok(Ast::Num { dec, suffix, text, col })
            }
            Tok::Param(name) => Ok(Ast::Param { name, col }),
            Tok::Ident(w) if w == "true" => Ok(Ast::Bool(true, col)),
            Tok::Ident(w) if w == "false" => Ok(Ast::Bool(false, col)),
            Tok::Ident(name) => {
                if *self.peek() == Tok::LParen {
                    self.next();
                    let mut args = Vec::new();
                    if *self.peek() != Tok::RParen {
                        loop {
                            args.push(self.or()?);
                            match self.next() {
                                (Tok::Comma, _) => continue,
                                (Tok::RParen, _) => break,
                                (_, c) => return Err(ExprError::new(codes::SYNTAX, format!("expected ',' or ')' in the arguments of {name}()"), c)),
                            }
                        }
                    } else {
                        self.next();
                    }
                    return Ok(Ast::Call { name, args, col });
                }
                Ok(Ast::Name { parts: vec![name], col })
            }
            Tok::LParen => {
                let e = self.or()?;
                match self.next() {
                    (Tok::RParen, _) => Ok(e),
                    (_, c) => Err(ExprError::new(codes::SYNTAX, "expected ')'", c).suggest("close the parenthesis")),
                }
            }
            Tok::End => Err(ExprError::new(codes::SYNTAX, "the expression ends too early", col).suggest("finish it: a value is missing")),
            other => Err(ExprError::new(codes::SYNTAX, format!("unexpected {}", describe(&other)), col)),
        }
    }
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Num { text, suffix } => format!("number '{text}{suffix}'"),
        Tok::Ident(s) => format!("name '{s}'"),
        Tok::Param(s) => format!("'${s}'"),
        Tok::Op(o) => format!("'{o}'"),
        Tok::LParen => "'('".into(),
        Tok::RParen => "')'".into(),
        Tok::Comma => "','".into(),
        Tok::Dot => "'.'".into(),
        Tok::End => "the end".into(),
    }
}

fn ast_depth(a: &Ast) -> usize {
    1 + match a {
        Ast::Num { .. } | Ast::Bool(..) | Ast::Name { .. } | Ast::Param { .. } => 0,
        Ast::Member { base, .. } => ast_depth(base),
        Ast::Neg(e, _) | Ast::Not(e, _) => ast_depth(e),
        Ast::Bin { l, r, .. } => ast_depth(l).max(ast_depth(r)),
        Ast::Call { args, .. } => args.iter().map(ast_depth).max().unwrap_or(0),
    }
}

/// Parses an expression string.
pub fn parse(src: &str) -> Result<Ast, ExprError> {
    if src.trim().is_empty() {
        return Err(ExprError::new(codes::SYNTAX, "the expression is empty", 1).suggest("write a value, e.g. 10 or self.hp"));
    }
    let chars = src.chars().count();
    if chars > MAX_EXPR_CHARS {
        return Err(ExprError::new(codes::TOO_MANY, format!("the expression is {chars} characters; at most {MAX_EXPR_CHARS}"), 1)
            .suggest("split it: put parts in params or attributes"));
    }
    let toks = lex(src)?;
    let mut p = Parser { toks, i: 0, depth: 0 };
    let e = p.or()?;
    let depth = ast_depth(&e);
    if depth > MAX_EXPR_DEPTH as usize {
        return Err(ExprError::new(codes::TOO_MANY, format!("the expression is {depth} operations deep; at most {MAX_EXPR_DEPTH}"), 1)
            .suggest("split it: put parts in params or attributes"));
    }
    if *p.peek() != Tok::End {
        let col = p.col();
        let what = describe(p.peek());
        return Err(ExprError::new(codes::SYNTAX, format!("unexpected {what} after a complete expression"), col)
            .suggest("an operator is missing, or there is an extra value"));
    }
    Ok(e)
}

// ---- errors ----

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExprError {
    pub code: &'static str,
    pub message: String,
    pub column: usize,
    pub expected: Option<String>,
    pub got: Option<String>,
    pub suggestion: Option<String>,
    /// Warnings (unknown tag) don't stop compilation.
    pub warning: bool,
}

impl ExprError {
    pub fn new(code: &'static str, message: impl Into<String>, column: usize) -> ExprError {
        ExprError { code, message: message.into(), column, expected: None, got: None, suggestion: None, warning: false }
    }

    pub fn expected(mut self, s: impl Into<String>) -> Self {
        self.expected = Some(s.into());
        self
    }

    pub fn got(mut self, s: impl Into<String>) -> Self {
        self.got = Some(s.into());
        self
    }

    pub fn suggest(mut self, s: impl Into<String>) -> Self {
        self.suggestion = Some(s.into());
        self
    }

    pub fn suggest_opt(mut self, s: Option<String>) -> Self {
        if s.is_some() {
            self.suggestion = s;
        }
        self
    }
}

// ---- types, scope and compiled form ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    Num(Unit),
    Bool,
    Ent,
    Arch,
    Site,
}

impl Ty {
    pub fn describe(self) -> String {
        match self {
            Ty::Num(Unit::Int) => "a plain number".into(),
            Ty::Num(u) => format!("a {} value", u.name()),
            Ty::Bool => "a boolean".into(),
            Ty::Ent => "an entity".into(),
            Ty::Arch => "an archetype".into(),
            Ty::Site => "a site".into(),
        }
    }
}

/// Which entity a name refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    Me,
    Other,
    Target,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaveQ {
    Current,
    Total,
    Remaining,
    TicksLeft,
    AllCleared,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arith {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    /// `a * b / 1000` (b is a ratio).
    MulRatio,
    /// `a * 1000 / b` (b is a ratio).
    DivRatio,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Ex {
    Int(i64),
    Bool(bool),
    Arch(u16),
    Site(u16),
    Tick,
    Res(u16),
    Magnitude,
    EventValue,
    Waves(WaveQ),
    Who(Who),
    /// Command argument `i`.
    Arg(u8),
    /// An entity's current attribute value.
    Attr(Box<Ex>, u16),
    Alive(Box<Ex>),
    PathRemaining(Box<Ex>),
    ArchOf(Box<Ex>),
    /// An archetype's `next` (from an archetype value).
    Next(Box<Ex>),
    /// An archetype's base attribute value.
    ArchAttr(Box<Ex>, u16),
    Neg(Box<Ex>),
    Not(Box<Ex>),
    Arith(Arith, Box<Ex>, Box<Ex>),
    Cmp(Cmp, Box<Ex>, Box<Ex>),
    And(Box<Ex>, Box<Ex>),
    Or(Box<Ex>, Box<Ex>),
    If(Box<Ex>, Box<Ex>, Box<Ex>),
    Min(Box<Ex>, Box<Ex>),
    Max(Box<Ex>, Box<Ex>),
    Clamp(Box<Ex>, Box<Ex>, Box<Ex>),
    Abs(Box<Ex>),
    Exists(Box<Ex>),
    /// Tag id; `u16::MAX` = a tag nothing has (always false).
    Has(Box<Ex>, u16),
    Count(u16),
    Dist(Box<Ex>, Box<Ex>),
    Occupied(Box<Ex>, u16),
    Rand(u16, Box<Ex>, Box<Ex>),
    Chance(u16, Box<Ex>),
}

/// An integer operation with the language's semantics: saturating, `/` truncating toward zero.
/// `None` = division by zero (the caller reports it and uses 0).
pub fn arith(op: Arith, a: i64, b: i64) -> Option<i64> {
    let clamp = |v: i128| v.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64;
    Some(match op {
        Arith::Add => a.saturating_add(b),
        Arith::Sub => a.saturating_sub(b),
        Arith::Mul => a.saturating_mul(b),
        Arith::Div => {
            if b == 0 {
                return None;
            }
            a.checked_div(b).unwrap_or(i64::MAX)
        }
        Arith::Rem => {
            if b == 0 {
                return None;
            }
            a.checked_rem(b).unwrap_or(0)
        }
        Arith::MulRatio => clamp(i128::from(a) * i128::from(b) / 1000),
        Arith::DivRatio => {
            if b == 0 {
                return None;
            }
            clamp(i128::from(a) * 1000 / i128::from(b))
        }
    })
}

pub fn compare(op: Cmp, a: i64, b: i64) -> bool {
    match op {
        Cmp::Eq => a == b,
        Cmp::Ne => a != b,
        Cmp::Lt => a < b,
        Cmp::Le => a <= b,
        Cmp::Gt => a > b,
        Cmp::Ge => a >= b,
    }
}

/// Names the compiler knows: filled from the items before any expression is compiled.
#[derive(Debug, Default)]
pub struct Registry {
    pub tick_hz: u32,
    /// Attribute name → (id, unit).
    pub attrs: BTreeMap<String, (u16, Unit)>,
    pub resources: BTreeMap<String, (u16, Unit)>,
    /// Parameter → (value, unit), overrides already applied.
    pub params: BTreeMap<String, (i64, Unit)>,
    pub archetypes: BTreeMap<String, u16>,
    /// Per archetype: the attribute ids it has.
    pub arch_attrs: Vec<Vec<u16>>,
    pub sites: BTreeMap<String, u16>,
    pub tags: BTreeMap<String, u16>,
    pub has_waves: bool,
    /// RNG stream names, registered as expressions use them (ids in first-use order).
    pub streams: RefCell<BTreeMap<String, u16>>,
    /// Warnings raised while compiling (unknown tags), drained by the compiler.
    pub warnings: RefCell<Vec<ExprError>>,
}

impl Registry {
    pub fn stream(&self, name: &str) -> u16 {
        let mut s = self.streams.borrow_mut();
        let n = s.len() as u16;
        *s.entry(name.to_owned()).or_insert(n)
    }

    pub fn attr_name(&self, id: u16) -> &str {
        self.attrs.iter().find(|(_, (i, _))| *i == id).map_or("?", |(n, _)| n.as_str())
    }

    fn arch_name(&self, id: u16) -> &str {
        self.archetypes.iter().find(|(_, i)| **i == id).map_or("?", |(n, _)| n.as_str())
    }
}

/// Who `self` is where an expression is used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Me {
    /// No entity (a command, a timer, an outcome).
    None,
    /// A known archetype (its behaviours and effects on it).
    Arch(u16),
    /// Some entity of an unknown archetype (rules, effects).
    Any,
}

/// The names available where an expression is used.
#[derive(Clone, Copy, Debug)]
pub struct Scope<'a> {
    pub reg: &'a Registry,
    pub me: Me,
    pub other: bool,
    pub target: bool,
    /// Inside an effect: the unit of `magnitude`.
    pub magnitude: Option<Unit>,
    /// Inside a rule: the unit of `event.value`.
    pub event: Option<Unit>,
    pub args: &'a [(String, Ty)],
    /// "a command", "an attack payload": for messages.
    pub place: &'a str,
}

impl<'a> Scope<'a> {
    pub fn new(reg: &'a Registry, place: &'a str) -> Scope<'a> {
        Scope { reg, me: Me::None, other: false, target: false, magnitude: None, event: None, args: &[], place }
    }
}

const ROOTS: [&str; 11] = ["self", "other", "target", "arg", "res", "tick", "magnitude", "event", "waves", "true", "false"];
/// The functions, for "did you mean" and "X is a function" hints.
pub const FUNCS: [&str; 12] = ["min", "max", "clamp", "abs", "if", "exists", "has", "count", "dist", "occupied", "rand", "chance"];
const ENTITY_FIELDS: [&str; 4] = ["alive", "archetype", "next", "path_remaining"];
const WAVE_FIELDS: [&str; 5] = ["current", "total", "remaining", "ticks_left", "all_cleared"];

/// Compiles a source string to a typed, folded expression.
pub fn compile(src: &str, scope: &Scope) -> Result<(Ex, Ty), ExprError> {
    let ast = parse(src)?;
    let (ex, ty) = check(&ast, scope)?;
    Ok((fold(ex)?, ty))
}

/// Compiles and requires a type.
pub fn compile_as(src: &str, scope: &Scope, want: Ty) -> Result<Ex, ExprError> {
    let ast = parse(src)?;
    let (ex, ty) = check(&ast, scope)?;
    if ty != want {
        return Err(mismatch(&ast, ty, want));
    }
    fold(ex)
}

fn mismatch(ast: &Ast, got: Ty, want: Ty) -> ExprError {
    let mut e = ExprError::new(codes::UNIT_MISMATCH, format!("{} is expected here, but this is {}", want.describe(), got.describe()), ast.col())
        .expected(want.describe())
        .got(got.describe());
    if let (Ty::Num(Unit::Int), Ty::Num(u), Some(text)) = (got, want, ast.bare_literal()) {
        e.message = format!("{} is expected here, but {text} has no unit", want.describe());
        e = e.suggest(format!("write {text}{} ({})", u.suffix(), u.human()));
    } else if let (Ty::Num(_), Ty::Num(u)) = (got, want) {
        e = e.suggest(format!("convert it: multiply or divide by a {} literal, e.g. 1{}", u.name(), u.suffix()));
    }
    e
}

fn check(ast: &Ast, sc: &Scope) -> Result<(Ex, Ty), ExprError> {
    match ast {
        Ast::Num { dec, suffix, text, col } => {
            let (unit, num, den) = suffix_unit(suffix, sc.reg.tick_hz).expect("lexer only accepts known suffixes");
            let s = scale(*dec, num, den)
                .ok_or_else(|| ExprError::new(codes::OUT_OF_RANGE, format!("{text}{suffix} is too large"), *col))?;
            if !s.exact {
                if suffix.is_empty() {
                    return Err(ExprError::new(codes::WRONG_TYPE, format!("{text} is not a whole number, and plain numbers are integers"), *col)
                        .suggest("give it a unit (e.g. 1.5m, 0.5s, 35%) or use a whole number"));
                }
                let push = ExprError::new(
                    codes::INEXACT,
                    format!("{text}{suffix} is not a whole number of stored units; it is rounded to {}", crate::units::human(s.value, unit, sc.reg.tick_hz)),
                    *col,
                )
                .suggest(format!(
                    "use {} or {} for an exact value",
                    crate::units::human(s.below, unit, sc.reg.tick_hz),
                    crate::units::human(s.above, unit, sc.reg.tick_hz)
                ));
                sc.reg.warnings.borrow_mut().push(ExprError { warning: true, ..push });
            }
            Ok((Ex::Int(s.value), Ty::Num(unit)))
        }
        Ast::Bool(b, _) => Ok((Ex::Bool(*b), Ty::Bool)),
        Ast::Param { name, col } => match sc.reg.params.get(name) {
            Some((v, u)) => Ok((Ex::Int(*v), Ty::Num(*u))),
            None => Err(ExprError::new(codes::UNKNOWN_REF, format!("there is no param '{name}'"), *col)
                .suggest_opt(did_you_mean(name, sc.reg.params.keys().map(String::as_str)).or_else(|| Some(format!("declare it: \"{name}\": {{\"kind\": \"param\", \"type\": \"int\", \"value\": 0}}"))))),
        },
        Ast::Name { parts, col } => name(parts, *col, sc),
        Ast::Member { base, name, col } => {
            let (b, ty) = check(base, sc)?;
            member(b, ty, name, *col, sc, None)
        }
        Ast::Neg(e, col) => {
            let (x, ty) = check(e, sc)?;
            match ty {
                Ty::Num(_) => Ok((Ex::Neg(Box::new(x)), ty)),
                _ => Err(ExprError::new(codes::WRONG_TYPE, format!("'-' needs a number, not {}", ty.describe()), *col)),
            }
        }
        Ast::Not(e, col) => {
            let (x, ty) = check(e, sc)?;
            match ty {
                Ty::Bool => Ok((Ex::Not(Box::new(x)), Ty::Bool)),
                _ => Err(ExprError::new(codes::WRONG_TYPE, format!("'!' needs a boolean, not {}", ty.describe()), *col)),
            }
        }
        Ast::Bin { op, l, r, col } => binary(*op, l, r, *col, sc),
        Ast::Call { name, args, col } => call(name, args, *col, sc),
    }
}

fn not_available(what: &str, sc: &Scope, col: usize, available_in: &str) -> ExprError {
    ExprError::new(codes::NOT_AVAILABLE, format!("'{what}' is not available in {}", sc.place), col).suggest(format!("'{what}' exists {available_in}"))
}

fn name(parts: &[String], col: usize, sc: &Scope) -> Result<(Ex, Ty), ExprError> {
    let root = parts[0].as_str();
    let rest = &parts[1..];
    let walk = |mut ex: Ex, mut ty: Ty, rest: &[String], me_arch: Option<u16>| -> Result<(Ex, Ty), ExprError> {
        let mut arch_hint = me_arch;
        for m in rest {
            let (e2, t2) = member(ex, ty, m, col, sc, arch_hint)?;
            ex = e2;
            ty = t2;
            arch_hint = None;
        }
        Ok((ex, ty))
    };
    match root {
        "self" => {
            let hint = match sc.me {
                Me::None => return Err(not_available("self", sc, col, "in archetypes' behaviours, effects and rules (not in commands, timers or outcomes)")),
                Me::Arch(a) => Some(a),
                Me::Any => None,
            };
            walk(Ex::Who(Who::Me), Ty::Ent, rest, hint)
        }
        "other" => {
            if !sc.other {
                return Err(not_available("other", sc, col, "in rules on events with an instigator (died: the killer) and in hits"));
            }
            walk(Ex::Who(Who::Other), Ty::Ent, rest, None)
        }
        "target" => {
            if !sc.target {
                return Err(not_available("target", sc, col, "in attack payloads, projectile hits and chase"));
            }
            walk(Ex::Who(Who::Target), Ty::Ent, rest, None)
        }
        "arg" => {
            let Some(a) = rest.first() else {
                return Err(ExprError::new(codes::SYNTAX, "'arg' needs an argument name", col).suggest("write arg.<name>"));
            };
            if sc.args.is_empty() {
                return Err(not_available("arg", sc, col, "only in commands, for their declared args"));
            }
            let Some(i) = sc.args.iter().position(|(n, _)| n == a) else {
                return Err(ExprError::new(codes::UNKNOWN_REF, format!("this command has no arg '{a}'"), col)
                    .suggest_opt(did_you_mean(a, sc.args.iter().map(|(n, _)| n.as_str()))));
            };
            walk(Ex::Arg(i as u8), sc.args[i].1, &rest[1..], None)
        }
        "res" => {
            let Some(r) = rest.first() else {
                return Err(ExprError::new(codes::SYNTAX, "'res' needs a resource name", col).suggest("write res.<resource>"));
            };
            let Some((id, unit)) = sc.reg.resources.get(r) else {
                return Err(ExprError::new(codes::UNKNOWN_REF, format!("there is no resource '{r}'"), col)
                    .suggest_opt(did_you_mean(r, sc.reg.resources.keys().map(String::as_str))));
            };
            if rest.len() > 1 {
                return Err(ExprError::new(codes::SYNTAX, format!("a resource has no member '{}'", rest[1]), col));
            }
            Ok((Ex::Res(*id), Ty::Num(*unit)))
        }
        "tick" if rest.is_empty() => Ok((Ex::Tick, Ty::Num(Unit::Duration))),
        "magnitude" if rest.is_empty() => match sc.magnitude {
            Some(u) => Ok((Ex::Magnitude, Ty::Num(u))),
            None => Err(not_available("magnitude", sc, col, "inside effects (modifier values and effect actions)")),
        },
        "event" => match (sc.event, rest) {
            (Some(u), [v]) if v == "value" => Ok((Ex::EventValue, Ty::Num(u))),
            (Some(_), _) => Err(ExprError::new(codes::UNKNOWN_REF, "events have one field: event.value", col).suggest("write event.value")),
            (None, _) => Err(not_available("event", sc, col, "only in rules")),
        },
        "waves" => {
            if !sc.reg.has_waves {
                return Err(ExprError::new(codes::NOT_AVAILABLE, "there is no waves item, so 'waves' has no value", col).suggest("add an item with \"kind\": \"waves\""));
            }
            let field = rest.first().map(String::as_str).unwrap_or("");
            let q = match field {
                "current" => (WaveQ::Current, Ty::Num(Unit::Int)),
                "total" => (WaveQ::Total, Ty::Num(Unit::Int)),
                "remaining" => (WaveQ::Remaining, Ty::Num(Unit::Int)),
                "ticks_left" => (WaveQ::TicksLeft, Ty::Num(Unit::Duration)),
                "all_cleared" => (WaveQ::AllCleared, Ty::Bool),
                _ => {
                    return Err(ExprError::new(codes::UNKNOWN_REF, format!("waves has no field '{field}'"), col)
                        .suggest_opt(did_you_mean(field, WAVE_FIELDS)));
                }
            };
            if rest.len() > 1 {
                return Err(ExprError::new(codes::SYNTAX, format!("waves.{field} has no members"), col));
            }
            Ok((Ex::Waves(q.0), q.1))
        }
        other => {
            if let Some(&a) = sc.reg.archetypes.get(other) {
                return walk(Ex::Arch(a), Ty::Arch, rest, None);
            }
            if let Some(&s) = sc.reg.sites.get(other) {
                return walk(Ex::Site(s), Ty::Site, rest, None);
            }
            let hint = if sc.reg.resources.contains_key(other) {
                Some(format!("resources are read as res.{other}"))
            } else if sc.reg.params.contains_key(other) {
                Some(format!("params are read as ${other}"))
            } else if sc.reg.attrs.contains_key(other) {
                Some(format!("attributes belong to an entity: self.{other}, target.{other}"))
            } else if FUNCS.contains(&other) {
                Some(format!("{other} is a function: call it, e.g. {other}(...)"))
            } else {
                did_you_mean(other, ROOTS)
            };
            Err(ExprError::new(codes::UNKNOWN_REF, format!("unknown name '{other}'"), col).suggest_opt(hint))
        }
    }
}

fn member(base: Ex, ty: Ty, m: &str, col: usize, sc: &Scope, arch_hint: Option<u16>) -> Result<(Ex, Ty), ExprError> {
    let reg = sc.reg;
    let attr = |arch: Option<u16>| -> Result<(u16, Unit), ExprError> {
        let Some(&(id, unit)) = reg.attrs.get(m) else {
            let mut names: Vec<&str> = ENTITY_FIELDS.to_vec();
            names.extend(reg.attrs.keys().map(String::as_str));
            return Err(ExprError::new(codes::UNKNOWN_REF, format!("no archetype has an attribute or field '{m}'"), col)
                .suggest_opt(did_you_mean(m, names)));
        };
        if let Some(a) = arch {
            if !reg.arch_attrs[usize::from(a)].contains(&id) {
                let names: Vec<&str> = reg.arch_attrs[usize::from(a)].iter().map(|i| reg.attr_name(*i)).collect();
                return Err(ExprError::new(codes::UNKNOWN_REF, format!("archetype '{}' has no attribute '{m}'", reg.arch_name(a)), col)
                    .suggest_opt(did_you_mean(m, names).or_else(|| Some(format!("add \"{m}\" to its attributes")))));
            }
        }
        Ok((id, unit))
    };
    match ty {
        Ty::Ent => match m {
            "alive" => Ok((Ex::Alive(Box::new(base)), Ty::Bool)),
            "archetype" => Ok((Ex::ArchOf(Box::new(base)), Ty::Arch)),
            "next" => Ok((Ex::Next(Box::new(Ex::ArchOf(Box::new(base)))), Ty::Arch)),
            "path_remaining" => Ok((Ex::PathRemaining(Box::new(base)), Ty::Num(Unit::Distance))),
            _ => {
                let (id, unit) = attr(arch_hint)?;
                Ok((Ex::Attr(Box::new(base), id), Ty::Num(unit)))
            }
        },
        Ty::Arch => match m {
            "next" => Ok((Ex::Next(Box::new(base)), Ty::Arch)),
            _ => {
                let known = if let Ex::Arch(a) = base { Some(a) } else { None };
                let (id, unit) = attr(known)?;
                Ok((Ex::ArchAttr(Box::new(base), id), Ty::Num(unit)))
            }
        },
        other => Err(ExprError::new(codes::WRONG_TYPE, format!("{} has no member '{m}'", other.describe()), col)),
    }
}

fn binary(op: BinOp, l: &Ast, r: &Ast, col: usize, sc: &Scope) -> Result<(Ex, Ty), ExprError> {
    let (a, ta) = check(l, sc)?;
    let (b, tb) = check(r, sc)?;
    let b2 = |x: Ex| Box::new(x);
    let zero = |ast: &Ast, ex: &Ex| matches!(ex, Ex::Int(0)) && ast.bare_literal().is_some();
    let unit_err = |what: &str| {
        let mut e = ExprError::new(codes::UNIT_MISMATCH, format!("'{}' {what}: {} and {}", op.symbol(), ta.describe(), tb.describe()), col)
            .expected("matching units")
            .got(format!("{} {} {}", ta.describe(), op.symbol(), tb.describe()));
        if let (Ty::Num(Unit::Int), Ty::Num(u), Some(t)) = (ta, tb, l.bare_literal()) {
            e = e.suggest(format!("write {t}{} on the left", u.suffix()));
        } else if let (Ty::Num(u), Ty::Num(Unit::Int), Some(t)) = (ta, tb, r.bare_literal()) {
            e = e.suggest(format!("write {t}{} on the right", u.suffix()));
        }
        e
    };
    match op {
        BinOp::And | BinOp::Or => {
            if ta != Ty::Bool || tb != Ty::Bool {
                return Err(ExprError::new(codes::WRONG_TYPE, format!("'{}' needs booleans on both sides", op.symbol()), col)
                    .got(format!("{} and {}", ta.describe(), tb.describe()))
                    .suggest("compare numbers first, e.g. self.hp > 0pt"));
            }
            Ok((if op == BinOp::And { Ex::And(b2(a), b2(b)) } else { Ex::Or(b2(a), b2(b)) }, Ty::Bool))
        }
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let cmp = match op {
                BinOp::Eq => Cmp::Eq,
                BinOp::Ne => Cmp::Ne,
                BinOp::Lt => Cmp::Lt,
                BinOp::Le => Cmp::Le,
                BinOp::Gt => Cmp::Gt,
                _ => Cmp::Ge,
            };
            let ordered = !matches!(op, BinOp::Eq | BinOp::Ne);
            match (ta, tb) {
                (Ty::Num(x), Ty::Num(y)) if x == y || zero(l, &a) || zero(r, &b) => Ok((Ex::Cmp(cmp, b2(a), b2(b)), Ty::Bool)),
                (Ty::Num(_), Ty::Num(_)) => Err(unit_err("compares different units")),
                (x, y) if x == y && !ordered => Ok((Ex::Cmp(cmp, b2(a), b2(b)), Ty::Bool)),
                _ => Err(ExprError::new(codes::WRONG_TYPE, format!("'{}' can't compare {} with {}", op.symbol(), ta.describe(), tb.describe()), col)),
            }
        }
        BinOp::Add | BinOp::Sub | BinOp::Rem => {
            let ar = match op {
                BinOp::Add => Arith::Add,
                BinOp::Sub => Arith::Sub,
                _ => Arith::Rem,
            };
            match (ta, tb) {
                (Ty::Num(x), Ty::Num(y)) if x == y => Ok((Ex::Arith(ar, b2(a), b2(b)), ta)),
                (Ty::Num(_), Ty::Num(y)) if zero(l, &a) => Ok((Ex::Arith(ar, b2(a), b2(b)), Ty::Num(y))),
                (Ty::Num(x), Ty::Num(_)) if zero(r, &b) => Ok((Ex::Arith(ar, b2(a), b2(b)), Ty::Num(x))),
                (Ty::Num(x), Ty::Num(Unit::Int)) if op == BinOp::Rem => Ok((Ex::Arith(ar, b2(a), b2(b)), Ty::Num(x))),
                (Ty::Num(_), Ty::Num(_)) => Err(unit_err("needs the same unit on both sides")),
                _ => Err(ExprError::new(codes::WRONG_TYPE, format!("'{}' needs numbers", op.symbol()), col).got(format!("{} and {}", ta.describe(), tb.describe()))),
            }
        }
        BinOp::Mul => match (ta, tb) {
            (Ty::Num(x), Ty::Num(Unit::Int)) => Ok((Ex::Arith(Arith::Mul, b2(a), b2(b)), Ty::Num(x))),
            (Ty::Num(Unit::Int), Ty::Num(y)) => {
                if y == Unit::Ratio {
                    Ok((Ex::Arith(Arith::MulRatio, b2(a), b2(b)), Ty::Num(Unit::Int)))
                } else {
                    Ok((Ex::Arith(Arith::Mul, b2(a), b2(b)), Ty::Num(y)))
                }
            }
            (Ty::Num(x), Ty::Num(Unit::Ratio)) => Ok((Ex::Arith(Arith::MulRatio, b2(a), b2(b)), Ty::Num(x))),
            (Ty::Num(Unit::Ratio), Ty::Num(y)) => Ok((Ex::Arith(Arith::MulRatio, b2(a), b2(b)), Ty::Num(y))),
            (Ty::Num(_), Ty::Num(_)) => Err(unit_err("can't multiply these units").suggest("multiply by a plain number or a ratio (35%, 1.5x)")),
            _ => Err(ExprError::new(codes::WRONG_TYPE, "'*' needs numbers", col).got(format!("{} and {}", ta.describe(), tb.describe()))),
        },
        BinOp::Div => match (ta, tb) {
            (Ty::Num(x), Ty::Num(Unit::Int)) => Ok((Ex::Arith(Arith::Div, b2(a), b2(b)), Ty::Num(x))),
            (Ty::Num(x), Ty::Num(y)) if x == y => Ok((Ex::Arith(Arith::Div, b2(a), b2(b)), Ty::Num(Unit::Int))),
            (Ty::Num(x), Ty::Num(Unit::Ratio)) => Ok((Ex::Arith(Arith::DivRatio, b2(a), b2(b)), Ty::Num(x))),
            (Ty::Num(_), Ty::Num(_)) => Err(unit_err("can't divide these units").suggest("divide by a plain number, a ratio, or the same unit")),
            _ => Err(ExprError::new(codes::WRONG_TYPE, "'/' needs numbers", col).got(format!("{} and {}", ta.describe(), tb.describe()))),
        },
    }
}

fn tag_arg(ast: &Ast, sc: &Scope, func: &str) -> Result<u16, ExprError> {
    let Ast::Name { parts, col } = ast else {
        return Err(ExprError::new(codes::WRONG_TYPE, format!("{func}() takes a tag name here (written bare, e.g. enemy)"), ast.col()));
    };
    let tag = parts.join(".");
    match sc.reg.tags.get(&tag) {
        Some(t) => Ok(*t),
        None => {
            let w = ExprError::new(codes::UNKNOWN_REF, format!("no archetype or effect has the tag '{tag}', so this is always empty"), *col)
                .suggest_opt(did_you_mean(&tag, sc.reg.tags.keys().map(String::as_str)));
            sc.reg.warnings.borrow_mut().push(ExprError { warning: true, ..w });
            Ok(u16::MAX)
        }
    }
}

fn call(fname: &str, args: &[Ast], col: usize, sc: &Scope) -> Result<(Ex, Ty), ExprError> {
    let arity = |n: usize, sig: &str| -> Result<(), ExprError> {
        if args.len() == n {
            Ok(())
        } else {
            Err(ExprError::new(codes::WRONG_TYPE, format!("{fname}() takes {n} argument{}, got {}", if n == 1 { "" } else { "s" }, args.len()), col)
                .expected(sig.to_owned()))
        }
    };
    let num = |i: usize| -> Result<(Ex, Unit), ExprError> {
        let (e, t) = check(&args[i], sc)?;
        match t {
            Ty::Num(u) => Ok((e, u)),
            other => Err(ExprError::new(codes::WRONG_TYPE, format!("argument {} of {fname}() must be a number, not {}", i + 1, other.describe()), args[i].col())),
        }
    };
    let same = |units: &[Unit], asts: &[&Ast]| -> Result<Unit, ExprError> {
        // A bare 0 adopts the others' unit.
        let mut unit: Option<Unit> = None;
        for (u, a) in units.iter().zip(asts) {
            let is_zero = matches!(a, Ast::Num { dec, suffix, .. } if suffix.is_empty() && dec.mant == 0);
            if is_zero {
                continue;
            }
            match unit {
                None => unit = Some(*u),
                Some(x) if x == *u => {}
                Some(x) => {
                    return Err(ExprError::new(codes::UNIT_MISMATCH, format!("{fname}() needs the same unit for all values: {} and {}", x.name(), u.name()), a.col())
                        .suggest_opt(a.bare_literal().map(|t| format!("write {t}{}", x.suffix()))));
                }
            }
        }
        Ok(unit.unwrap_or(Unit::Int))
    };
    let b = Box::new;
    match fname {
        "min" | "max" => {
            arity(2, "min(a, b)")?;
            let ((x, ux), (y, uy)) = (num(0)?, num(1)?);
            let u = same(&[ux, uy], &[&args[0], &args[1]])?;
            Ok((if fname == "min" { Ex::Min(b(x), b(y)) } else { Ex::Max(b(x), b(y)) }, Ty::Num(u)))
        }
        "clamp" => {
            arity(3, "clamp(value, lo, hi)")?;
            let ((x, ux), (lo, ul), (hi, uh)) = (num(0)?, num(1)?, num(2)?);
            let u = same(&[ux, ul, uh], &[&args[0], &args[1], &args[2]])?;
            Ok((Ex::Clamp(b(x), b(lo), b(hi)), Ty::Num(u)))
        }
        "abs" => {
            arity(1, "abs(x)")?;
            let (x, u) = num(0)?;
            Ok((Ex::Abs(b(x)), Ty::Num(u)))
        }
        "if" => {
            arity(3, "if(condition, then, else)")?;
            let (c, tc) = check(&args[0], sc)?;
            if tc != Ty::Bool {
                return Err(ExprError::new(codes::WRONG_TYPE, "the first argument of if() must be a boolean", args[0].col()).got(tc.describe()));
            }
            let (x, tx) = check(&args[1], sc)?;
            let (y, ty) = check(&args[2], sc)?;
            let t = match (tx, ty) {
                (a, c2) if a == c2 => a,
                (Ty::Num(u), Ty::Num(_)) if matches!(args[2], Ast::Num { ref dec, ref suffix, .. } if suffix.is_empty() && dec.mant == 0) => Ty::Num(u),
                (Ty::Num(_), Ty::Num(u)) if matches!(args[1], Ast::Num { ref dec, ref suffix, .. } if suffix.is_empty() && dec.mant == 0) => Ty::Num(u),
                _ => {
                    return Err(ExprError::new(codes::UNIT_MISMATCH, format!("both branches of if() must have the same type: {} and {}", tx.describe(), ty.describe()), col));
                }
            };
            Ok((Ex::If(b(c), b(x), b(y)), t))
        }
        "exists" => {
            arity(1, "exists(entity or archetype)")?;
            let (x, t) = check(&args[0], sc)?;
            match t {
                Ty::Ent | Ty::Arch => Ok((Ex::Exists(b(x)), Ty::Bool)),
                other => Err(ExprError::new(codes::WRONG_TYPE, format!("exists() takes an entity or an archetype, not {}", other.describe()), args[0].col())),
            }
        }
        "has" => {
            arity(2, "has(entity, tag)")?;
            let (x, t) = check(&args[0], sc)?;
            if t != Ty::Ent {
                return Err(ExprError::new(codes::WRONG_TYPE, format!("has() takes an entity first, not {}", t.describe()), args[0].col()).suggest("e.g. has(target, flying)"));
            }
            Ok((Ex::Has(b(x), tag_arg(&args[1], sc, "has")?), Ty::Bool))
        }
        "count" => {
            arity(1, "count(tag)")?;
            Ok((Ex::Count(tag_arg(&args[0], sc, "count")?), Ty::Num(Unit::Int)))
        }
        "dist" => {
            arity(2, "dist(entity, entity)")?;
            let (x, tx) = check(&args[0], sc)?;
            let (y, ty) = check(&args[1], sc)?;
            if tx != Ty::Ent || ty != Ty::Ent {
                return Err(ExprError::new(codes::WRONG_TYPE, "dist() takes two entities", col).got(format!("{} and {}", tx.describe(), ty.describe())));
            }
            Ok((Ex::Dist(b(x), b(y)), Ty::Num(Unit::Distance)))
        }
        "occupied" => {
            arity(2, "occupied(site, tag)")?;
            let (x, t) = check(&args[0], sc)?;
            if t != Ty::Site {
                return Err(ExprError::new(codes::WRONG_TYPE, format!("occupied() takes a site first, not {}", t.describe()), args[0].col()));
            }
            Ok((Ex::Occupied(b(x), tag_arg(&args[1], sc, "occupied")?), Ty::Bool))
        }
        "rand" => {
            arity(3, "rand(stream, lo, hi)")?;
            let Ast::Name { parts, .. } = &args[0] else {
                return Err(ExprError::new(codes::WRONG_TYPE, "rand() takes a stream name first (written bare, e.g. drops)", args[0].col()));
            };
            if parts.len() != 1 {
                return Err(ExprError::new(codes::WRONG_TYPE, "stream names are single words", args[0].col()));
            }
            let stream = sc.reg.stream(&parts[0]);
            let ((lo, ul), (hi, uh)) = (num(1)?, num(2)?);
            let u = same(&[ul, uh], &[&args[1], &args[2]])?;
            Ok((Ex::Rand(stream, b(lo), b(hi)), Ty::Num(u)))
        }
        "chance" => {
            arity(2, "chance(stream, ratio)")?;
            let Ast::Name { parts, .. } = &args[0] else {
                return Err(ExprError::new(codes::WRONG_TYPE, "chance() takes a stream name first (written bare, e.g. crits)", args[0].col()));
            };
            if parts.len() != 1 {
                return Err(ExprError::new(codes::WRONG_TYPE, "stream names are single words", args[0].col()));
            }
            let stream = sc.reg.stream(&parts[0]);
            let (p, u) = num(1)?;
            if u != Unit::Ratio {
                return Err(ExprError::new(codes::UNIT_MISMATCH, "chance() takes a ratio", args[1].col())
                    .suggest_opt(args[1].bare_literal().map(|t| format!("write {t}%"))));
            }
            Ok((Ex::Chance(stream, b(p)), Ty::Bool))
        }
        other => Err(ExprError::new(codes::UNKNOWN_REF, format!("unknown function '{other}'"), col)
            .suggest_opt(did_you_mean(other, FUNCS))),
    }
}

/// Whether evaluating `ex` walks the entity list (`count`, `occupied`): such expressions cost
/// O(entities), so outcomes that use them are checked once per phase, not after every step.
pub fn scans_entities(ex: &Ex) -> bool {
    use Ex::*;
    match ex {
        Count(_) | Occupied(..) => true,
        Attr(a, _) | Alive(a) | PathRemaining(a) | ArchOf(a) | Next(a) | ArchAttr(a, _) | Neg(a) | Not(a) | Abs(a) | Exists(a) | Has(a, _) | Chance(_, a) => scans_entities(a),
        Arith(_, a, b) | Cmp(_, a, b) | And(a, b) | Or(a, b) | Min(a, b) | Max(a, b) | Dist(a, b) | Rand(_, a, b) => scans_entities(a) || scans_entities(b),
        If(a, b, c) | Clamp(a, b, c) => scans_entities(a) || scans_entities(b) || scans_entities(c),
        _ => false,
    }
}

/// Folds constant subexpressions (pure ones only: never `rand`, `chance` or state reads).
fn fold(ex: Ex) -> Result<Ex, ExprError> {
    use Ex::*;
    let f = |e: Box<Ex>| fold(*e).map(Box::new);
    Ok(match ex {
        Neg(e) => match fold(*e)? {
            Int(v) => Int(v.saturating_neg()),
            e => Neg(Box::new(e)),
        },
        Not(e) => match fold(*e)? {
            Bool(v) => Bool(!v),
            e => Not(Box::new(e)),
        },
        Arith(op, a, b) => {
            let (a, b) = (f(a)?, f(b)?);
            match (&*a, &*b) {
                (Int(x), Int(y)) => match arith(op, *x, *y) {
                    Some(v) => Int(v),
                    None => return Err(ExprError::new(codes::INVALID, "division by zero in a constant expression", 1)),
                },
                _ => Arith(op, a, b),
            }
        }
        Cmp(op, a, b) => {
            let (a, b) = (f(a)?, f(b)?);
            match (&*a, &*b) {
                (Int(x), Int(y)) => Bool(compare(op, *x, *y)),
                _ => Cmp(op, a, b),
            }
        }
        And(a, b) => {
            let (a, b) = (f(a)?, f(b)?);
            match (&*a, &*b) {
                (Bool(false), _) => Bool(false),
                (Bool(true), _) => *b,
                _ => And(a, b),
            }
        }
        Or(a, b) => {
            let (a, b) = (f(a)?, f(b)?);
            match (&*a, &*b) {
                (Bool(true), _) => Bool(true),
                (Bool(false), _) => *b,
                _ => Or(a, b),
            }
        }
        If(c, a, b) => {
            let (c, a, b) = (f(c)?, f(a)?, f(b)?);
            match *c {
                Bool(true) => *a,
                Bool(false) => *b,
                c => If(Box::new(c), a, b),
            }
        }
        Min(a, b) => {
            let (a, b) = (f(a)?, f(b)?);
            match (&*a, &*b) {
                (Int(x), Int(y)) => Int(*x.min(y)),
                _ => Min(a, b),
            }
        }
        Max(a, b) => {
            let (a, b) = (f(a)?, f(b)?);
            match (&*a, &*b) {
                (Int(x), Int(y)) => Int(*x.max(y)),
                _ => Max(a, b),
            }
        }
        Clamp(x, lo, hi) => {
            let (x, lo, hi) = (f(x)?, f(lo)?, f(hi)?);
            match (&*x, &*lo, &*hi) {
                (Int(v), Int(l), Int(h)) => Int((*v).max(*l).min(*h)),
                _ => Clamp(x, lo, hi),
            }
        }
        Abs(x) => match fold(*x)? {
            Int(v) => Int(v.saturating_abs()),
            e => Abs(Box::new(e)),
        },
        Attr(e, a) => Attr(f(e)?, a),
        Alive(e) => Alive(f(e)?),
        PathRemaining(e) => PathRemaining(f(e)?),
        ArchOf(e) => ArchOf(f(e)?),
        Next(e) => Next(f(e)?),
        ArchAttr(e, a) => ArchAttr(f(e)?, a),
        Exists(e) => Exists(f(e)?),
        Has(e, t) => Has(f(e)?, t),
        Dist(a, b) => Dist(f(a)?, f(b)?),
        Occupied(s, t) => Occupied(f(s)?, t),
        Rand(s, a, b) => Rand(s, f(a)?, f(b)?),
        Chance(s, p) => Chance(s, f(p)?),
        other => other,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg() -> Registry {
        let mut r = Registry { tick_hz: 30, has_waves: true, ..Default::default() };
        r.attrs.insert("hp".into(), (0, Unit::Points));
        r.attrs.insert("range".into(), (1, Unit::Distance));
        r.attrs.insert("cost".into(), (2, Unit::Int));
        r.resources.insert("dust".into(), (0, Unit::Int));
        r.params.insert("bonus".into(), (5, Unit::Int));
        r.archetypes.insert("warding".into(), 0);
        r.arch_attrs.push(vec![1, 2]);
        r.tags.insert("enemy".into(), 0);
        r
    }

    fn ok(src: &str, sc: &Scope) -> (Ex, Ty) {
        compile(src, sc).unwrap_or_else(|e| panic!("{src}: {e:?}"))
    }

    fn err(src: &str, sc: &Scope) -> ExprError {
        compile(src, sc).expect_err(src)
    }

    #[test]
    fn literals_fold_to_stored_units() {
        let r = reg();
        let sc = Scope::new(&r, "a test");
        assert_eq!(ok("7m", &sc), (Ex::Int(7000), Ty::Num(Unit::Distance)));
        assert_eq!(ok("0.8s", &sc), (Ex::Int(24), Ty::Num(Unit::Duration)));
        assert_eq!(ok("24t", &sc), (Ex::Int(24), Ty::Num(Unit::Duration)));
        assert_eq!(ok("35%", &sc), (Ex::Int(350), Ty::Num(Unit::Ratio)));
        assert_eq!(ok("1.35x", &sc), (Ex::Int(1350), Ty::Num(Unit::Ratio)));
        assert_eq!(ok("2.2m/s", &sc), (Ex::Int(2200), Ty::Num(Unit::Speed)));
        assert_eq!(ok("10pt * 35%", &sc), (Ex::Int(3500), Ty::Num(Unit::Points)));
        assert_eq!(ok("100% - 35%", &sc), (Ex::Int(650), Ty::Num(Unit::Ratio)));
        assert_eq!(ok("$bonus * 2 + 1", &sc), (Ex::Int(11), Ty::Num(Unit::Int)));
        assert_eq!(ok("7m / 2m", &sc), (Ex::Int(3), Ty::Num(Unit::Int)));
        assert_eq!(ok("-7 / 2", &sc).0, Ex::Int(-3), "truncates toward zero");
        assert_eq!(ok("min(3m, 2m) == 2m && !false", &sc).0, Ex::Bool(true));
        assert_eq!(ok("if(1 < 2, 5s, 0)", &sc).0, Ex::Int(150));
    }

    #[test]
    fn unit_errors_suggest_the_literal() {
        let r = reg();
        let mut sc = Scope::new(&r, "a behaviour");
        sc.me = Me::Arch(0);
        let e = err("self.range > 7", &sc);
        assert_eq!(e.code, "unit_mismatch");
        assert_eq!(e.suggestion.as_deref(), Some("write 7m on the right"));
        let e = compile_as("7", &sc, Ty::Num(Unit::Distance)).unwrap_err();
        assert_eq!(e.suggestion.as_deref(), Some("write 7m (metres)"));
        let e = err("2m + 1s", &sc);
        assert_eq!(e.code, "unit_mismatch");
        assert_eq!(e.column, 4);
        assert_eq!(ok("self.range > 0", &sc).1, Ty::Bool, "a bare 0 matches any unit");
        let e = err("1.5", &sc);
        assert_eq!(e.code, "wrong_type");
    }

    #[test]
    fn names_are_checked_where_they_are_used() {
        let r = reg();
        let sc = Scope::new(&r, "a command");
        let e = err("self.hp", &sc);
        assert_eq!(e.code, "not_available");
        let e = err("res.dusst", &sc);
        assert_eq!(e.suggestion.as_deref(), Some("did you mean 'dust'?"));
        let e = err("dust > 3", &sc);
        assert_eq!(e.suggestion.as_deref(), Some("resources are read as res.dust"));
        let e = err("$bonsu", &sc);
        assert_eq!(e.suggestion.as_deref(), Some("did you mean 'bonus'?"));
        let mut sc2 = Scope::new(&r, "a behaviour");
        sc2.me = Me::Arch(0);
        let e = err("self.hp", &sc2);
        assert!(e.message.contains("archetype 'warding' has no attribute 'hp'"), "{}", e.message);
        assert_eq!(ok("warding.cost + 1", &sc2).1, Ty::Num(Unit::Int));
        let e = err("mni(1, 2)", &sc2);
        assert_eq!(e.suggestion.as_deref(), Some("did you mean 'min'?"));
        assert_eq!(ok("rand(drops, 3, 5)", &sc).1, Ty::Num(Unit::Int));
        assert_eq!(r.streams.borrow().get("drops"), Some(&0));
        assert_eq!(ok("waves.ticks_left * 50% / 1s", &sc).1, Ty::Num(Unit::Int));
        assert_eq!(ok("count(enemy) > 0", &sc).1, Ty::Bool);
        ok("count(ghosts) > 0", &sc);
        assert_eq!(r.warnings.borrow().len(), 1, "an unknown tag is a warning");
    }

    #[test]
    fn syntax_errors_point_at_the_column() {
        let r = reg();
        let sc = Scope::new(&r, "a test");
        let e = err("1 +", &sc);
        assert_eq!((e.code, e.column), ("syntax", 4));
        let e = err("res.dust = 3", &sc);
        assert_eq!(e.suggestion.as_deref(), Some("use '==' to compare"));
        let e = err("7mm", &sc);
        assert_eq!(e.code, "syntax");
        let e = err("1 < 2 < 3", &sc);
        assert!(e.message.contains("don't chain"));
        let e = err("res.dust & 1", &sc);
        assert_eq!(e.suggestion.as_deref(), Some("use '&&' for 'and'"));
        let e = err("(1 + 2", &sc);
        assert_eq!(e.suggestion.as_deref(), Some("close the parenthesis"));
    }

    #[test]
    fn inexact_literals_warn_with_both_neighbours() {
        let r = reg();
        let sc = Scope::new(&r, "a test");
        assert_eq!(ok("0.25s", &sc).0, Ex::Int(8));
        let w = r.warnings.borrow();
        assert_eq!(w[0].code, "inexact");
        assert_eq!(w[0].suggestion.as_deref(), Some("use 7t or 8t for an exact value"));
    }

    #[test]
    fn arithmetic_saturates_and_reports_division_by_zero() {
        assert_eq!(arith(Arith::Mul, i64::MAX, 2), Some(i64::MAX));
        assert_eq!(arith(Arith::Div, 1, 0), None);
        assert_eq!(arith(Arith::Div, i64::MIN, -1), Some(i64::MAX));
        assert_eq!(arith(Arith::MulRatio, 55, 650), Some(35));
        assert_eq!(arith(Arith::DivRatio, 35, 500), Some(70));
    }
}
