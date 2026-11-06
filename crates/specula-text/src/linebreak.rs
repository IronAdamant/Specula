//! Line break opportunities: a documented subset of Unicode's line breaking algorithm (UAX #14),
//! enough for Latin, Cyrillic, Greek and CJK text (design: `docs/design/TEXT.md`).
//!
//! # Classes
//!
//! Characters are classified by ranges ([`class_of`]): mandatory breaks (BK, CR, LF, NL), spaces
//! (SP), zero width space (ZW), word joiner and no-break spaces (WJ, GL), combining marks and
//! joiners (CM, ZWJ), hyphens (HY), break-after punctuation such as en dash, soft hyphen and tab
//! (BA), break-before (BB), the em dash (B2), opening and closing punctuation (OP, CL, CP),
//! quotation marks (QU), exclamation (EX), infix separators (IS: `, . : ;`), slash (SY), prefix
//! and postfix numeric (PR, PO: currency, percent), digits (NU), ideographs, kana, hangul and
//! emoji (ID), CJK non-starters (NS: small kana, the prolonged sound mark, iteration marks) and
//! alphabetic (AL, everything else).
//!
//! # Rules (in order; `×` no break, `÷` break, `!` mandatory)
//!
//! - LB4–6: `BK !`, `CR × LF`, `(CR|LF|NL) !`; no break before them.
//! - LB7: `× SP`, `× ZW`. LB8: `ZW SP* ÷`.
//! - LB9–10: combining marks and ZWJ take the class of what they follow (alone: AL).
//! - LB11: `× WJ`, `WJ ×`. LB12: `GL ×`. LB12a: `[^SP BA HY] × GL`.
//! - LB13: `× (CL|CP|EX|IS|SY)`. LB14: `OP SP* ×`. LB15: `QU SP* × OP`.
//! - LB16: `(CL|CP) SP* × NS`. LB17: `B2 SP* × B2`. LB18: `SP ÷`.
//! - LB19: `× QU`, `QU ×`. LB21: `× (BA|HY|NS)`, `BB ×`. LB21a: `HY ×` after a letter is kept
//!   (`well-known` breaks after the hyphen only).
//! - LB23–25 (simplified): `(AL) × NU`, `NU × (AL)`, `NU × NU`, `(PR|PO) × NU`, `NU × (PR|PO)`,
//!   `PR × (AL|ID)`, `IS × NU` after a digit.
//! - LB28: `AL × AL`. LB30: `(AL|NU) × OP`, `CP × (AL|NU)`.
//! - LB31: `÷` everywhere else (so ideographs break between each other).
//!
//! Not covered: Hebrew letters (HL), regional indicator pairs, emoji modifier sequences,
//! Southeast Asian dictionary breaking (SA), the conditional Japanese starter (CJ, treated as
//! NS: strict breaking), and Korean syllable blocks (treated as ID). The tables are code, not
//! data files; [`breaks`] never panics and is linear in the text.

/// A line breaking class (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    BK,
    CR,
    LF,
    NL,
    SP,
    ZW,
    WJ,
    GL,
    CM,
    ZWJ,
    HY,
    BA,
    BB,
    B2,
    OP,
    CL,
    CP,
    QU,
    EX,
    IS,
    SY,
    PR,
    PO,
    NU,
    ID,
    NS,
    AL,
}

/// The class of `c`.
pub fn class_of(c: char) -> Class {
    use Class::*;
    let u = c as u32;
    match u {
        0x0A => LF,
        0x0D => CR,
        0x0B | 0x0C | 0x2028 | 0x2029 => BK,
        0x85 => NL,
        0x09 => BA,
        0x20 => SP,
        0x200B => ZW,
        0x2060 | 0xFEFF => WJ,
        0xA0 | 0x202F | 0x2007 | 0x2011 | 0x0F0C | 0x034F => GL,
        0x200D => ZWJ,
        0x0300..=0x036F | 0x0483..=0x0489 | 0x0591..=0x05BD | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE00..=0xFE0F | 0xFE20..=0xFE2F | 0x200C => CM,
        0x00..=0x1F | 0x7F..=0x9F => CM,
        0x2D => HY,
        0xAD | 0x2010 | 0x2012 | 0x2013 | 0x058A | 0x2027 | 0x1680 | 0x2000..=0x2006 | 0x2008..=0x200A | 0x205F | 0x3000 => BA,
        0xB4 | 0x2C8 | 0x2CC | 0x2DF | 0x1FFD => BB,
        0x2014 => B2,
        0x28 | 0x5B | 0x7B | 0xA1 | 0xBF | 0x2018 | 0x201A | 0x201C | 0x201E | 0x2045 | 0x207D | 0x208D | 0x2329 | 0x3008 | 0x300A
        | 0x300C | 0x300E | 0x3010 | 0x3014 | 0x3016 | 0x3018 | 0x301A | 0x301D | 0xFF08 | 0xFF3B | 0xFF5B | 0xFF5F | 0xFF62 => OP,
        0x29 | 0x5D => CP,
        0x7D | 0x2046 | 0x207E | 0x208E | 0x232A | 0x3001 | 0x3002 | 0x3009 | 0x300B | 0x300D | 0x300F | 0x3011 | 0x3015
        | 0x3017 | 0x3019 | 0x301B | 0x301E | 0x301F | 0xFE10 | 0xFE11 | 0xFE12 | 0xFE50 | 0xFE52 | 0xFF09 | 0xFF0C | 0xFF0E
        | 0xFF3D | 0xFF5D | 0xFF60 | 0xFF61 | 0xFF63 | 0xFF64 => CL,
        0x22 | 0x27 | 0xAB | 0xBB | 0x2019 | 0x201B | 0x201D | 0x201F | 0x2039 | 0x203A => QU,
        0x21 | 0x3F | 0x5BE | 0x61F | 0xFF01 | 0xFF1F => EX,
        0x2C | 0x2E | 0x3A | 0x3B | 0x37E | 0x589 | 0x60C | 0x60D | 0x7F8 | 0x2044 | 0xFE10..=0xFE13 => IS,
        0x2F => SY,
        0x24 | 0x2B | 0x5C | 0xA3..=0xA5 | 0xB1 | 0x20A0..=0x20A6 | 0x20A8..=0x20B5 | 0x20B7..=0x20BA | 0x20BC..=0x20BF | 0x2116 | 0xFFE1 | 0xFFE5 | 0xFFE6 => PR,
        0x25 | 0xA2 | 0xB0 | 0x2030..=0x2037 | 0x2103 | 0x2109 | 0x20A7 | 0x20B6 | 0x20BB | 0xFFE0 | 0xFF05 => PO,
        0x30..=0x39 | 0x660..=0x669 | 0x6F0..=0x6F9 | 0xFF10..=0xFF19 => NU,
        // CJK non-starters: small kana, iteration marks, the prolonged sound mark, middle dots.
        0x3005 | 0x303B | 0x309B..=0x309E | 0x30A0 | 0x30FB..=0x30FE | 0x3041 | 0x3043 | 0x3045 | 0x3047 | 0x3049 | 0x3063 | 0x3083
        | 0x3085 | 0x3087 | 0x308E | 0x3095 | 0x3096 | 0x30A1 | 0x30A3 | 0x30A5 | 0x30A7 | 0x30A9 | 0x30C3 | 0x30E3 | 0x30E5
        | 0x30E7 | 0x30EE | 0x30F5 | 0x30F6 | 0x31F0..=0x31FF | 0xFF67..=0xFF70 | 0x203C | 0x2047..=0x2049 | 0xFF1A | 0xFF1B => NS,
        0x1100..=0x115F | 0x2E80..=0x2FFF | 0x3040..=0x30FF | 0x3100..=0x31EF | 0x3200..=0x4DBF | 0x4E00..=0x9FFF | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x1F000..=0x1FAFF | 0x20000..=0x3FFFD => ID,
        _ => AL,
    }
}

/// A break opportunity before the character at `byte` (or at the end of the text).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Break {
    pub byte: usize,
    /// A mandatory break (newline, paragraph separator): the line must end here.
    pub mandatory: bool,
}

/// Every break opportunity in `text`, in order: at each, a line may end before the character at
/// `byte`. The end of the text is always a (mandatory) break; the start never is.
pub fn breaks(text: &str) -> Vec<Break> {
    let mut out = Vec::new();
    breaks_into(text, &mut out);
    out
}

/// [`breaks`] into `out` (cleared first; no allocation once warm).
pub fn breaks_into(text: &str, out: &mut Vec<Break>) {
    use Class::*;
    out.clear();
    // State: the class before the current position after LB9/10 resolution, the class before
    // any spaces (for the `X SP* ×` rules), and whether the run before was all digits.
    let mut prev: Option<Class> = None;
    let mut before_spaces: Option<Class> = None;
    let mut prev_raw: Option<Class> = None;
    let mut in_number = false;
    for (byte, c) in text.char_indices() {
        let mut cur = class_of(c);
        let Some(p) = prev else {
            // LB10: a combining mark at the start is AL.
            if matches!(cur, CM | ZWJ) {
                cur = AL;
            }
            prev = Some(cur);
            prev_raw = Some(cur);
            before_spaces = Some(cur);
            in_number = cur == NU;
            continue;
        };
        // LB9: combining marks and ZWJ attach to the previous character (not after breaks or
        // spaces, where they are AL).
        if matches!(cur, CM | ZWJ) {
            if matches!(p, BK | CR | LF | NL | SP | ZW) {
                cur = AL;
            } else {
                prev_raw = Some(cur);
                continue;
            }
        }
        let decision = decide(p, cur, before_spaces, prev_raw, in_number);
        match decision {
            Decision::Mandatory => out.push(Break { byte, mandatory: true }),
            Decision::Allow => out.push(Break { byte, mandatory: false }),
            Decision::Prohibit => {}
        }
        if cur != SP {
            before_spaces = Some(cur);
        } else if p != SP {
            before_spaces = Some(p);
        }
        in_number = match cur {
            NU => true,
            IS | SY | CL | CP if in_number => true,
            _ => false,
        };
        prev = Some(cur);
        prev_raw = Some(cur);
    }
    out.push(Break { byte: text.len(), mandatory: true });
}

enum Decision {
    Mandatory,
    Allow,
    Prohibit,
}

fn decide(p: Class, c: Class, before_spaces: Option<Class>, prev_raw: Option<Class>, in_number: bool) -> Decision {
    use Class::*;
    use Decision::*;
    // LB4, LB5.
    if p == CR && c == LF {
        return Prohibit;
    }
    if matches!(p, BK | CR | LF | NL) {
        return Mandatory;
    }
    // LB6.
    if matches!(c, BK | CR | LF | NL) {
        return Prohibit;
    }
    // LB7.
    if matches!(c, SP | ZW) {
        return Prohibit;
    }
    // LB8: after a ZW (and spaces after it).
    if p == ZW || (p == SP && before_spaces == Some(ZW)) {
        return Allow;
    }
    // LB8a: ZWJ ×.
    if prev_raw == Some(ZWJ) {
        return Prohibit;
    }
    // LB11.
    if c == WJ || p == WJ {
        return Prohibit;
    }
    // LB12, LB12a.
    if p == GL {
        return Prohibit;
    }
    if c == GL && !matches!(p, SP | BA | HY) {
        return Prohibit;
    }
    // LB13.
    if matches!(c, CL | CP | EX | IS | SY) {
        return Prohibit;
    }
    // LB14: OP SP* ×.
    if p == OP || (p == SP && before_spaces == Some(OP)) {
        return Prohibit;
    }
    // LB15: QU SP* × OP.
    if c == OP && (p == QU || (p == SP && before_spaces == Some(QU))) {
        return Prohibit;
    }
    // LB16: (CL|CP) SP* × NS.
    if c == NS && (matches!(p, CL | CP) || (p == SP && matches!(before_spaces, Some(CL | CP)))) {
        return Prohibit;
    }
    // LB17: B2 SP* × B2.
    if c == B2 && (p == B2 || (p == SP && before_spaces == Some(B2))) {
        return Prohibit;
    }
    // LB18: SP ÷.
    if p == SP {
        return Allow;
    }
    // LB19.
    if c == QU || p == QU {
        return Prohibit;
    }
    // LB21.
    if matches!(c, BA | HY | NS) || p == BB {
        return Prohibit;
    }
    // LB23–25 (simplified numbers).
    match (p, c) {
        (AL, NU) | (NU, AL) | (NU, NU) | (PR, NU) | (PO, NU) | (NU, PR) | (NU, PO) | (PR, AL) | (PR, ID) | (PO, AL) | (OP, NU) => {
            return Prohibit
        }
        (IS | SY, NU) if in_number => return Prohibit,
        (CL | CP, PO | PR) if in_number => return Prohibit,
        _ => {}
    }
    // LB28, LB30.
    if matches!((p, c), (AL, AL) | (AL | NU, OP) | (CP, AL | NU)) {
        return Prohibit;
    }
    // LB31.
    Allow
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text with `|` at every optional break and `!` at every mandatory one (the final
    /// break left out).
    fn marked(text: &str) -> String {
        let b = breaks(text);
        let mut s = String::new();
        for (i, c) in text.char_indices() {
            if let Some(k) = b.iter().find(|k| k.byte == i) {
                s.push(if k.mandatory { '!' } else { '|' });
            }
            s.push(c);
        }
        assert_eq!(b.last(), Some(&Break { byte: text.len(), mandatory: true }));
        s
    }

    #[test]
    fn latin_cyrillic_and_greek_break_after_spaces() {
        assert_eq!(marked("Rest in peace, old friend."), "Rest |in |peace, |old |friend.");
        assert_eq!(marked("Покойся с миром"), "Покойся |с |миром");
        assert_eq!(marked("Ἐνθάδε κεῖται"), "Ἐνθάδε |κεῖται", "combining accents stay with their letter");
        assert_eq!(marked("e\u{301}te\u{301} x"), "e\u{301}te\u{301} |x");
    }

    #[test]
    fn punctuation_numbers_and_hyphens() {
        assert_eq!(marked("well-known (sic) \"quoted\" 3.14 $20 50% wave 3/10!"), "well-|known |(sic) |\"quoted\" |3.14 |$20 |50% |wave |3/10!");
        assert_eq!(marked("a — b"), "a |— |b");
        assert_eq!(marked("no\u{a0}break here"), "no\u{a0}break |here");
        assert_eq!(marked("zero\u{200b}width"), "zero\u{200b}|width");
        assert_eq!(marked("word\u{2060}joined"), "word\u{2060}joined");
        assert_eq!(marked("soft\u{ad}hyphen"), "soft\u{ad}|hyphen");
    }

    #[test]
    fn mandatory_breaks_and_crlf() {
        assert_eq!(marked("one\ntwo\r\nthree\u{2028}four"), "one\n!two\r\n!three\u{2028}!four");
        assert_eq!(marked("a  \n b"), "a  \n! |b");
    }

    #[test]
    fn cjk_breaks_between_ideographs_but_not_before_closing_marks() {
        assert_eq!(marked("墓地の夜。静かだ"), "墓|地|の|夜。|静|か|だ");
        assert_eq!(marked("「こんにちは」と"), "「こ|ん|に|ち|は」|と");
        assert_eq!(marked("ちょっと"), "ちょっ|と", "small kana never start a line");
        assert_eq!(marked("한국어 문장"), "한|국|어 |문|장");
    }

    #[test]
    fn hostile_text_never_panics() {
        let mut seed = 0x1234_5678u32;
        for _ in 0..300 {
            let n = (seed % 40) as usize;
            let s: String = (0..n)
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    char::from_u32(seed % 0x3_0000).unwrap_or('?')
                })
                .collect();
            let b = breaks(&s);
            assert!(b.windows(2).all(|w| w[0].byte < w[1].byte), "{s:?}");
            assert!(b.iter().all(|k| s.is_char_boundary(k.byte) && k.byte > 0 || s.is_empty()));
        }
        assert_eq!(breaks(""), vec![Break { byte: 0, mandatory: true }]);
        assert_eq!(marked("\u{301}"), "\u{301}");
    }
}
