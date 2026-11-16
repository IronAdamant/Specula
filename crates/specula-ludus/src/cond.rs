//! Conditions over what a game keeps: when a run is won, when it is lost.
//!
//! A condition names a key and what must hold of it, with the matchers a playtest's `expect`
//! already uses (`eq`, `ne`, `lt`, `lte`, `gt`, `gte`, `in`), and joins conditions with `all`,
//! `any` and `not`:
//!
//! ```json
//! { "all": [ { "key": "gate.wave", "gte": 5 }, { "key": "gate.runners", "eq": 0 } ] }
//! ```
//!
//! A bare key is "this is true": `"gate.won"`. **A key that is not there makes its test false**,
//! whatever the matcher: a run is not lost because nothing has been counted yet.
//!
//! Deterministic: a condition is a pure function of the values it is shown.

use specula_core::Json;

use crate::Problem;

/// Most conditions in one tree, and how deep it may nest.
pub const MAX_NODES: usize = 64;
pub const MAX_DEPTH: usize = 8;
/// Most characters in a key, and most values in an `in` list.
pub const MAX_KEY: usize = 128;
pub const MAX_IN: usize = 32;

/// A value a key holds, or a condition compares with.
#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Bool(bool),
    Num(f64),
    Text(String),
}

impl Val {
    /// A JSON value as one of these (`None` for null, lists and maps).
    pub fn of(json: &Json) -> Option<Val> {
        match json {
            Json::Bool(b) => Some(Val::Bool(*b)),
            Json::Num(n) if n.is_finite() => Some(Val::Num(*n)),
            Json::Str(s) => Some(Val::Text(s.clone())),
            _ => None,
        }
    }

    pub fn to_json(&self) -> Json {
        match self {
            Val::Bool(b) => Json::Bool(*b),
            Val::Num(n) => Json::Num(*n),
            Val::Text(s) => Json::Str(s.clone()),
        }
    }

    /// Whether the value counts as true on its own: `true`, a number that is not zero, text that
    /// is not empty.
    pub fn truthy(&self) -> bool {
        match self {
            Val::Bool(b) => *b,
            Val::Num(n) => *n != 0.0,
            Val::Text(s) => !s.is_empty(),
        }
    }
}

/// How a key's value is compared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
}

impl Op {
    const ALL: [(&'static str, Op); 6] = [("eq", Op::Eq), ("ne", Op::Ne), ("lt", Op::Lt), ("lte", Op::Lte), ("gt", Op::Gt), ("gte", Op::Gte)];

    fn name(self) -> &'static str {
        Op::ALL.iter().find(|(_, op)| *op == self).map_or("", |(name, _)| name)
    }

    /// Numbers compare as numbers; text and true/false only for being the same or not.
    fn holds(self, have: &Val, want: &Val) -> bool {
        match (have, want) {
            (Val::Num(a), Val::Num(b)) => match self {
                Op::Eq => a == b,
                Op::Ne => a != b,
                Op::Lt => a < b,
                Op::Lte => a <= b,
                Op::Gt => a > b,
                Op::Gte => a >= b,
            },
            _ => match self {
                Op::Eq => have == want,
                Op::Ne => have != want,
                _ => false,
            },
        }
    }
}

/// A condition.
#[derive(Clone, Debug, PartialEq)]
pub enum Cond {
    /// The key holds a value that counts as true.
    Is(String),
    /// The key's value compares so with this one.
    Test { key: String, op: Op, value: Val },
    /// The key's value is one of these.
    In { key: String, values: Vec<Val> },
    All(Vec<Cond>),
    Any(Vec<Cond>),
    Not(Box<Cond>),
}

/// What a condition is shown: the value a key holds now, if it holds one.
pub trait Lookup {
    fn value(&self, key: &str) -> Option<Val>;
}

impl<F: Fn(&str) -> Option<Val>> Lookup for F {
    fn value(&self, key: &str) -> Option<Val> {
        self(key)
    }
}

/// A key: 1 to [`MAX_KEY`] characters, none of them a blank or a control character.
pub fn key_ok(key: &str) -> bool {
    !key.is_empty() && key.chars().count() <= MAX_KEY && key.chars().all(|c| !c.is_whitespace() && !c.is_control())
}

impl Cond {
    /// Whether the condition holds of what `look` shows.
    pub fn holds(&self, look: &dyn Lookup) -> bool {
        match self {
            Cond::Is(key) => look.value(key).is_some_and(|v| v.truthy()),
            Cond::Test { key, op, value } => look.value(key).is_some_and(|have| op.holds(&have, value)),
            Cond::In { key, values } => look.value(key).is_some_and(|have| values.contains(&have)),
            Cond::All(all) => all.iter().all(|c| c.holds(look)),
            Cond::Any(any) => any.iter().any(|c| c.holds(look)),
            Cond::Not(c) => !c.holds(look),
        }
    }

    /// Every key the condition reads, in the order it names them (a key may be there twice).
    pub fn keys(&self) -> Vec<&str> {
        let mut out = Vec::new();
        self.keys_into(&mut out);
        out
    }

    fn keys_into<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            Cond::Is(key) | Cond::Test { key, .. } | Cond::In { key, .. } => out.push(key),
            Cond::All(list) | Cond::Any(list) => list.iter().for_each(|c| c.keys_into(out)),
            Cond::Not(c) => c.keys_into(out),
        }
    }

    /// Reads a condition from JSON found at `at` (a path for the error, such as `outcome.won`).
    pub fn parse(json: &Json, at: &str) -> Result<Cond, Problem> {
        let mut nodes = 0;
        Cond::parse_at(json, at, 0, &mut nodes)
    }

    fn parse_at(json: &Json, at: &str, depth: usize, nodes: &mut usize) -> Result<Cond, Problem> {
        *nodes += 1;
        if *nodes > MAX_NODES {
            return Err(Problem::new(at, "too_many", format!("a condition has at most {MAX_NODES} parts")));
        }
        if depth > MAX_DEPTH {
            return Err(Problem::new(at, "too_deep", format!("conditions nest at most {MAX_DEPTH} deep")));
        }
        let key_at = |key: &str, at: &str| {
            if key_ok(key) {
                Ok(key.to_owned())
            } else {
                Err(Problem::new(at, "bad_key", format!("a key is 1 to {MAX_KEY} characters with no blanks, such as \"gate.hp\" (got {key:?})")))
            }
        };
        if let Json::Str(key) = json {
            return Ok(Cond::Is(key_at(key, at)?));
        }
        let Some(fields) = json.as_object() else {
            return Err(Problem::new(at, "not_a_condition", "a condition is a key (\"gate.won\") or an object: {\"key\": \"gate.hp\", \"lte\": 0}, {\"all\": [...]}, {\"any\": [...]} or {\"not\": {...}}"));
        };
        let list = |value: &Json, word: &str, nodes: &mut usize| -> Result<Vec<Cond>, Problem> {
            let here = format!("{at}.{word}");
            let items = value.as_array().filter(|a| !a.is_empty()).ok_or_else(|| Problem::new(&here, "not_a_list", format!("\"{word}\" takes a list of one or more conditions")))?;
            items.iter().enumerate().map(|(i, c)| Cond::parse_at(c, &format!("{here}[{i}]"), depth + 1, nodes)).collect()
        };
        match fields {
            [(word, value)] if word == "all" => return Ok(Cond::All(list(value, "all", nodes)?)),
            [(word, value)] if word == "any" => return Ok(Cond::Any(list(value, "any", nodes)?)),
            [(word, value)] if word == "not" => return Ok(Cond::Not(Box::new(Cond::parse_at(value, &format!("{at}.not"), depth + 1, nodes)?))),
            _ => {}
        }
        let Some(key) = json.get("key").and_then(Json::as_str) else {
            let names: Vec<&str> = fields.iter().map(|(k, _)| k.as_str()).collect();
            return Err(Problem::new(at, "not_a_condition", format!("a condition has \"key\" and one matcher, or is one of all, any, not alone (got {names:?})")));
        };
        let key = key_at(key, &format!("{at}.key"))?;
        let matchers: Vec<&(String, Json)> = fields.iter().filter(|(k, _)| k != "key").collect();
        let known = || format!("one of {}, in", Op::ALL.map(|(n, _)| n).join(", "));
        let [(word, value)] = matchers.as_slice() else {
            let names: Vec<&str> = matchers.iter().map(|(k, _)| k.as_str()).collect();
            return Err(Problem::new(at, "one_matcher", format!("a condition on a key has exactly one matcher, {} (got {names:?}); a key alone, as text, tests that it is true", known())));
        };
        let here = format!("{at}.{word}");
        let val = |v: &Json, here: &str| Val::of(v).ok_or_else(|| Problem::new(here, "bad_value", "a value to compare with is a number, text, true or false"));
        if word == "in" {
            let items = value.as_array().filter(|a| !a.is_empty() && a.len() <= MAX_IN).ok_or_else(|| Problem::new(&here, "not_a_list", format!("\"in\" takes a list of 1 to {MAX_IN} values")))?;
            let values = items.iter().enumerate().map(|(i, v)| val(v, &format!("{here}[{i}]"))).collect::<Result<_, _>>()?;
            return Ok(Cond::In { key, values });
        }
        let Some((_, op)) = Op::ALL.iter().find(|(name, _)| name == word) else {
            return Err(Problem::new(&here, "unknown_matcher", format!("\"{word}\" is not a matcher: {}", known())));
        };
        let value = val(value, &here)?;
        if !matches!(op, Op::Eq | Op::Ne) && !matches!(value, Val::Num(_)) {
            return Err(Problem::new(&here, "bad_value", format!("\"{word}\" compares numbers; text and true or false are compared with eq and ne")));
        }
        Ok(Cond::Test { key, op: *op, value })
    }

    pub fn to_json(&self) -> Json {
        match self {
            Cond::Is(key) => Json::Str(key.clone()),
            Cond::Test { key, op, value } => Json::obj([("key", Json::Str(key.clone())), (op.name(), value.to_json())]),
            Cond::In { key, values } => Json::obj([("key", Json::Str(key.clone())), ("in", Json::arr(values.iter().map(Val::to_json)))]),
            Cond::All(list) => Json::obj([("all", Json::arr(list.iter().map(Cond::to_json)))]),
            Cond::Any(list) => Json::obj([("any", Json::arr(list.iter().map(Cond::to_json)))]),
            Cond::Not(c) => Json::obj([("not", c.to_json())]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cond(text: &str) -> Result<Cond, Problem> {
        Cond::parse(&Json::parse(text).expect("the test's JSON"), "won")
    }

    fn look(key: &str) -> Option<Val> {
        match key {
            "gate.hp" => Some(Val::Num(3.0)),
            "gate.wave" => Some(Val::Num(5.0)),
            "gate.runners" => Some(Val::Num(0.0)),
            "gate.mode" => Some(Val::Text("hard".to_owned())),
            "gate.won" => Some(Val::Bool(true)),
            "gate.empty" => Some(Val::Text(String::new())),
            _ => None,
        }
    }

    fn holds(text: &str) -> bool {
        cond(text).unwrap_or_else(|p| panic!("{text}: {p:?}")).holds(&look)
    }

    #[test]
    fn numbers_compare_and_the_rest_are_the_same_or_not() {
        assert!(holds(r#"{"key": "gate.hp", "gt": 0}"#) && holds(r#"{"key": "gate.hp", "lte": 3}"#));
        assert!(!holds(r#"{"key": "gate.hp", "lt": 3}"#) && holds(r#"{"key": "gate.hp", "gte": 3}"#));
        assert!(holds(r#"{"key": "gate.hp", "eq": 3}"#) && holds(r#"{"key": "gate.hp", "ne": 4}"#));
        assert!(holds(r#"{"key": "gate.mode", "eq": "hard"}"#) && holds(r#"{"key": "gate.mode", "ne": "easy"}"#));
        assert!(holds(r#"{"key": "gate.won", "eq": true}"#) && !holds(r#"{"key": "gate.won", "eq": 1}"#), "true is not the number one");
        assert!(holds(r#"{"key": "gate.mode", "in": ["normal", "hard"]}"#) && !holds(r#"{"key": "gate.mode", "in": ["easy"]}"#));
        assert!(holds(r#""gate.won""#) && holds(r#""gate.hp""#) && !holds(r#""gate.runners""#) && !holds(r#""gate.empty""#));
    }

    #[test]
    fn all_any_and_not_join_them() {
        assert!(holds(r#"{"all": [{"key": "gate.wave", "gte": 5}, {"key": "gate.runners", "eq": 0}]}"#));
        assert!(!holds(r#"{"all": [{"key": "gate.wave", "gte": 5}, {"key": "gate.hp", "eq": 0}]}"#));
        assert!(holds(r#"{"any": [{"key": "gate.hp", "eq": 0}, "gate.won"]}"#));
        assert!(holds(r#"{"not": {"key": "gate.hp", "lte": 0}}"#));
    }

    /// A run is not lost because nothing has been counted yet.
    #[test]
    fn a_key_that_is_not_there_makes_its_test_false() {
        for matcher in [r#""lte": 0"#, r#""eq": 0"#, r#""ne": 0"#, r#""in": [0]"#] {
            assert!(!holds(&format!(r#"{{"key": "gate.nothing", {matcher}}}"#)), "{matcher}");
        }
        assert!(!holds(r#""gate.nothing""#));
        assert!(holds(r#"{"not": "gate.nothing"}"#), "and its negation is true: say what you mean with all/any");
    }

    #[test]
    fn a_condition_that_is_not_one_says_what_to_write() {
        let code = |text: &str| cond(text).unwrap_err().code;
        assert_eq!(code("3"), "not_a_condition");
        assert_eq!(code(r#"{"gate.hp": 0}"#), "not_a_condition");
        assert_eq!(code(r#"{"key": "gate.hp"}"#), "one_matcher");
        assert_eq!(code(r#"{"key": "gate.hp", "lte": 0, "gte": 0}"#), "one_matcher");
        assert_eq!(code(r#"{"key": "gate.hp", "below": 0}"#), "unknown_matcher");
        assert_eq!(code(r#"{"key": "gate.hp", "lt": "low"}"#), "bad_value");
        assert_eq!(code(r#"{"key": "gate.hp", "eq": [1]}"#), "bad_value");
        assert_eq!(code(r#"{"key": "gate hp", "eq": 1}"#), "bad_key");
        assert_eq!(code(r#"{"all": []}"#), "not_a_list");
        assert_eq!(code(r#"{"key": "gate.hp", "in": []}"#), "not_a_list");
        let p = cond(r#"{"all": [{"key": "gate.hp", "eq": 1}, {"any": [{"key": "x", "is": 1}]}]}"#).unwrap_err();
        assert_eq!((p.at.as_str(), p.code), ("won.all[1].any[0].is", "unknown_matcher"), "the path says where");
    }

    #[test]
    fn a_condition_is_bounded() {
        let deep = (0..=MAX_DEPTH).fold(r#""a""#.to_owned(), |inner, _| format!(r#"{{"not": {inner}}}"#));
        assert_eq!(cond(&deep).unwrap_err().code, "too_deep");
        let wide = format!(r#"{{"all": [{}]}}"#, vec![r#""a""#; MAX_NODES].join(","));
        assert_eq!(cond(&wide).unwrap_err().code, "too_many");
        assert!(cond(&format!(r#"{{"all": [{}]}}"#, vec![r#""a""#; MAX_NODES - 1].join(","))).is_ok());
    }

    #[test]
    fn a_condition_names_its_keys_and_writes_itself_back() {
        let text = r#"{"all":[{"key":"gate.wave","gte":5},{"any":["gate.won",{"key":"gate.mode","in":["hard",2,true]}]},{"not":{"key":"gate.hp","ne":0}}]}"#;
        let c = cond(text).unwrap();
        assert_eq!(c.keys(), ["gate.wave", "gate.won", "gate.mode", "gate.hp"]);
        assert_eq!(c.to_json().to_string(), text);
        assert_eq!(Cond::parse(&c.to_json(), "x").unwrap(), c);
    }
}
