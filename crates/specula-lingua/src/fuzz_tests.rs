//! Fuzz-style robustness tests: mutated tables, rules, messages and locale ids must give
//! structured errors or values, never a panic. Deterministic; `SPECULA_LINGUA_FUZZ_ITERS` runs
//! longer.

use crate::catalog::Catalog;
use crate::check::check;
use crate::message::{format, pieces, pseudo, Arg};
use crate::plural::{Condition, Operands, PluralRules};
use crate::table::Table;
use crate::{Locale, NumberFormat};
use specula_core::rng::Rng;

const SEED_TABLE: &str = r##"{
  "format": "specula.strings.v1", "locale": "ru", "name": "Русский", "status": "unreviewed",
  "plural": {"one": "v = 0 and i % 10 = 1 and i % 100 != 11", "few": "v = 0 and i % 10 = 2..4"},
  "number": {"decimal": ",", "group": " ", "min_group": 2, "percent": "# %"},
  "fonts": {"ui": "system:ja", "title": "display"},
  "strings": {
    "hud.wave": "Волна {wave}/{waves}",
    "hud.enemies": {"plural": "n", "one": "{n} враг", "few": "{n} врага", "other": "{n} врагов"},
    "hud.menu": {"text": "Меню {{x}} {t:time} {r:pct} {d:.2} {s:plain}", "status": "reviewed", "src": "0123456789abcdef", "notes": "n"}
  }
}"##;

const ALPHABET: &[u8] = b"{}[]\":,.\\ 0123456789abcdefghijklmnopqrstuvwxyz%=!@-_\n\x01\xff";

fn iters() -> usize {
    std::env::var("SPECULA_LINGUA_FUZZ_ITERS").ok().and_then(|s| s.parse().ok()).unwrap_or(3000)
}

fn mutate(rng: &mut Rng, base: &[u8]) -> Vec<u8> {
    let mut b = base.to_vec();
    for _ in 0..=rng.below(6) {
        if b.is_empty() {
            break;
        }
        let at = rng.below(b.len() as u32) as usize;
        match rng.below(4) {
            0 => b[at] = ALPHABET[rng.below(ALPHABET.len() as u32) as usize],
            1 => {
                b.remove(at);
            }
            2 => b.insert(at, ALPHABET[rng.below(ALPHABET.len() as u32) as usize]),
            _ => b.truncate(at),
        }
    }
    b
}

#[test]
fn mutated_tables_never_panic() {
    let mut rng = Rng::named(7, "lingua-fuzz-tables");
    let en = Table::parse(r#"{"format": "specula.strings.v1", "locale": "en", "source": true, "strings": {"hud.wave": "Wave {wave}/{waves}", "hud.enemies": {"plural": "n", "one": "{n} enemy", "other": "{n} enemies"}, "hud.menu": "Menu"}}"#).unwrap();
    let mut parsed = 0;
    for _ in 0..iters() {
        let bytes = mutate(&mut rng, SEED_TABLE.as_bytes());
        let Ok(text) = std::str::from_utf8(&bytes) else { continue };
        match Table::parse(text) {
            Ok(t) => {
                parsed += 1;
                // Whatever parses resolves, formats and checks without panicking.
                let ts = [&en, &t];
                if let Ok(c) = Catalog::build(&t.locale, &ts) {
                    let mut out = String::new();
                    for k in ["hud.wave", "hud.enemies", "hud.menu"] {
                        c.format(k, &[("n", Arg::Int(3)), ("wave", Arg::Num(1.5)), ("t", Arg::Num(-1.0)), ("r", Arg::Num(f64::NAN)), ("d", Arg::Num(1e300))], &mut out);
                    }
                }
                let _ = check(&[("en.json", &en), ("x.json", &t)], None);
            }
            Err(e) => {
                assert!(!e.code.is_empty());
                let _ = e.to_json();
            }
        }
    }
    assert!(parsed > 0, "some mutations should still parse");
}

#[test]
fn hostile_rules_messages_and_locales_never_panic() {
    let mut rng = Rng::named(7, "lingua-fuzz-text");
    let rules = "v = 0 and i % 10 = 2..4 and i % 100 != 12..14 or n = 0,1 or e != 0..5";
    let message = "Wave {wave}/{waves:plain} {{x}} {t:time} {r:pct} {d:.6}";
    let nf = NumberFormat::builtin(&Locale::parse("fr").unwrap());
    for _ in 0..iters() {
        let r = mutate(&mut rng, rules.as_bytes());
        if let Ok(s) = std::str::from_utf8(&r) {
            if let Ok(c) = Condition::parse(s, "p") {
                for n in ["0", "1", "1.5", "12", "999999999999999999999.123"] {
                    let _ = c.holds(&Operands::from_decimal(n).unwrap());
                }
            }
            let _ = PluralRules::parse(&[("one", s)], "plural");
            let _ = Locale::parse(s);
            let _ = Operands::from_decimal(s);
        }
        let m = mutate(&mut rng, message.as_bytes());
        if let Ok(s) = std::str::from_utf8(&m) {
            let _ = pieces(s, "t");
            let mut out = String::new();
            format(s, &[("wave", Arg::Int(i64::MIN)), ("t", Arg::Num(f64::INFINITY)), ("r", Arg::Num(-1e20))], &nf, &mut out);
            out.clear();
            pseudo(s, &mut out);
        }
    }
}
