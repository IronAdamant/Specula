//! `examen corpus tables FEMALE.csv MALE.csv`: the body's tables, computed from their source.
//!
//! `specula-corpus` holds how a human body's dimensions are spread, how each follows stature
//! and weight, and how each moves with a handful of named traits (long legs, broad shoulders).
//! Those numbers are not typed from a paper: they are computed here from the two public data
//! files of the US Army's 2012 anthropometric survey (ANSUR II: 1,986 women and 4,082 men,
//! each measured on 93 dimensions; a US Government work, in the public domain), which anyone
//! can fetch and run this on. The files are never in this repository:
//!
//! ```text
//! https://tools.openlab.psu.edu/publicData/ANSUR_II_FEMALE_Public.csv
//! https://tools.openlab.psu.edu/publicData/ANSUR_II_MALE_Public.csv
//! cargo run -p specula-examen -- corpus tables FEMALE.csv MALE.csv > crates/specula-corpus/src/ansur_data.rs
//! ```
//!
//! The means, spreads and fitted lines were computed independently for the library research
//! (docs/research/library/A1-segment-lengths.md, Tables A1-1 to A1-3) and agree with these
//! to the digits printed there; the means agree with the survey's own report (Gordon et al.
//! 2014, NATICK/TR-15/007). `specula-corpus` holds a sample of both in its tests.
//!
//! **Where a joint's centre is** comes from de Leva (1996, J Biomech 29(9):1223-1230, Table
//! 2: how far each centre lies from the landmark a tape finds, in millimetres on his subjects
//! of 1.735 m and 1.741 m), scaled to each person's stature. The numbers below were read on
//! the page's image, not on a copy's text.

use std::path::Path;

/// The words the survey's column names are made of (`acromionradialelength`).
const WORDS: &[&str] = &[
    "abdominal", "extension", "depth", "sitting", "acromial", "height", "acromion", "radiale", "length", "ankle", "circumference", "axilla", "ball", "of", "foot", "biacromial",
    "breadth", "biceps", "flexed", "bicristal", "bideltoid", "bimalleolar", "bitragion", "chin", "arc", "submandibular", "bizygomatic", "buttock", "knee", "popliteal", "calf",
    "cervicale", "chest", "crotch", "omphalion", "posterior", "ear", "protrusion", "elbow", "rest", "eye", "horizontal", "forearm", "center", "grip", "hand", "functional", "leg",
    "head", "heel", "hip", "iliocristale", "interpupillary", "interscye", "ii", "i", "midpatella", "lateral", "femoral", "epicondyle", "malleolus", "lower", "thigh", "menton",
    "sellion", "neck", "base", "overhead", "fingertip", "reach", "palm", "stylion", "shoulder", "sleeve", "spine", "wrist", "outseam", "span", "stature", "suprasternale", "tenth",
    "rib", "clearance", "thumbtip", "tibial", "tragion", "top", "trochanterion", "vertical", "trunk", "usa", "waist", "back", "front", "weight", "kg",
];

/// A column's name cut into its words (`acromion_radiale_length`), or `None` when it is not
/// made of the survey's words. The longest word that fits is tried first (`ii` before `i`).
pub fn readable(column: &str) -> Option<String> {
    fn cut<'w>(rest: &str, words: &[&'w str]) -> Option<Vec<&'w str>> {
        if rest.is_empty() {
            return Some(Vec::new());
        }
        let mut fits: Vec<&&str> = words.iter().filter(|w| rest.starts_with(**w)).collect();
        fits.sort_by_key(|w| std::cmp::Reverse(w.len()));
        fits.into_iter().find_map(|w| {
            cut(&rest[w.len()..], words).map(|mut tail| {
                tail.insert(0, *w);
                tail
            })
        })
    }
    cut(&column.to_ascii_lowercase(), WORDS).map(|parts| parts.join("_"))
}

/// de Leva's subjects' statures (millimetres; women, men): what his millimetres are scaled by.
const DE_LEVA_STATURE: [f64; 2] = [1735.0, 1741.0];

/// de Leva's Table 3: the breadth between the two front points of the hip bones, as a share of
/// the height from trochanterion to the top of the head (women, men; one survey each).
const BISPINOUS: [f64; 2] = [0.2983, 0.2940];

/// Each hip joint's centre lies 0.36 of that breadth to the side of the pelvis's middle (Bell,
/// Brand and Pedersen 1989, Hum Mov Sci 8:3-16, as the motion-capture literature gives it:
/// the paper itself was not opened), so the two centres are 0.72 of it apart.
const HIP_CENTRES: f64 = 0.72;

/// One person while their links are worked out.
struct Person<'a> {
    names: &'a [String],
    row: &'a [f64],
    sex: usize,
    /// The hip centres' spacing as a share of this sex's bicristal breadth.
    hip_share: f64,
}

impl Person<'_> {
    fn v(&self, name: &str) -> f64 {
        self.names.iter().position(|n| n == name).and_then(|i| self.row.get(i).copied()).unwrap_or(f64::NAN)
    }

    /// One of de Leva's distances (millimetres on his women, on his men), at this stature.
    fn jc(&self, female: f64, male: f64) -> f64 {
        [female, male][self.sex] * self.v("stature") / DE_LEVA_STATURE[self.sex]
    }
}

/// A link of the body made of the measured dimensions, and how.
type Derived = (&'static str, &'static str, fn(&Person) -> f64);

/// The links (millimetres), in the order they are worked out: one may use those before it.
/// First the survey's own (its report's derived dimensions, by subtraction), then the
/// skeleton's: lengths between joint centres, and the heights of the centres on a person
/// standing.
const DERIVED: &[Derived] = &[
    ("thigh_link", "the survey's: trochanterion height less lateral femoral epicondyle height", |p| p.v("trochanterion_height") - p.v("lateral_femoral_epicondyle_height")),
    ("calf_link", "the survey's: lateral femoral epicondyle height less lateral malleolus height", |p| p.v("lateral_femoral_epicondyle_height") - p.v("lateral_malleolus_height")),
    ("tragion_height", "the survey's: stature less tragion to the top of the head", |p| p.v("stature") - p.v("tragion_top_of_head")),
    ("neck_link", "the survey's: tragion height less cervicale height", |p| p.v("tragion_height") - p.v("cervicale_height")),
    ("thorax_link", "the survey's: cervicale height less tenth rib height", |p| p.v("cervicale_height") - p.v("tenth_rib_height")),
    ("abdominal_link", "the survey's: tenth rib height less iliocristale height", |p| p.v("tenth_rib_height") - p.v("iliocristale_height")),
    ("pelvic_link", "the survey's: iliocristale height less trochanterion height", |p| p.v("iliocristale_height") - p.v("trochanterion_height")),
    ("vertex_to_cervicale", "stature less cervicale height: the head and the neck", |p| p.v("stature") - p.v("cervicale_height")),
    ("subischial_leg", "stature less sitting height", |p| p.v("stature") - p.v("sitting_height")),
    ("shoulder_height", "the shoulder joint's centre: acromial height less 33.7 mm (women) or 34.5 mm (men) of de Leva's", |p| p.v("acromial_height") - p.jc(33.7, 34.5)),
    (
        "upper_arm",
        "shoulder centre to elbow centre: acromion to radiale, less the shoulder's 33.7 or 34.5 mm and the elbow's 15.25 or 15.55 mm above the radiale (the mean of de Leva's two estimates)",
        |p| p.v("acromion_radiale_length") - p.jc(33.7, 34.5) - p.jc(15.25, 15.55),
    ),
    (
        "forearm",
        "elbow centre to wrist centre: radiale to stylion, and the elbow's 15.25 or 15.55 mm above the radiale, and the wrist's 1.85 or 1.95 mm past the stylion (the mean of two estimates)",
        |p| p.v("radiale_stylion_length") + p.jc(15.25, 15.55) + p.jc(1.85, 1.95),
    ),
    ("hand", "wrist centre to the tip of the middle finger: hand length less the wrist's 1.85 or 1.95 mm", |p| p.v("hand_length") - p.jc(1.85, 1.95)),
    ("arm", "shoulder centre to wrist centre: the upper arm and the forearm", |p| p.v("upper_arm") + p.v("forearm")),
    ("hip_height", "the hip joint's centre: trochanterion height and 2.8 or 3.2 mm", |p| p.v("trochanterion_height") + p.jc(2.8, 3.2)),
    ("knee_height", "the knee's centre: tibial height and 32.2 or 34.25 mm (the mean of de Leva's two estimates)", |p| p.v("tibial_height") + p.jc(32.2, 34.25)),
    ("thigh", "hip centre to knee centre", |p| p.v("hip_height") - p.v("knee_height")),
    ("shank", "knee centre to the lateral malleolus, which is taken for the ankle's height", |p| p.v("knee_height") - p.v("lateral_malleolus_height")),
    ("trunk", "cervicale height less the hip centres' height", |p| p.v("cervicale_height") - p.v("hip_height")),
    (
        "hip_spacing",
        "between the two hip centres: 0.72 of the breadth between the front points of the hip bones (Bell and others 1989), that breadth being de Leva's share of trochanterion to vertex at the mean, and following each person's bicristal breadth",
        |p| p.v("bicristal_breadth") * p.hip_share,
    ),
];

/// The traits a body is told apart by after its stature and weight, in the order they are
/// taken out, and the dimension each is read on. A trait is what is left of its dimension once
/// stature, weight and the traits before it are accounted for: so the traits are independent
/// of each other in the people measured, and one unit of a trait is one standard deviation.
const TRAITS: &[(&str, &str)] = &[
    ("age", "age"),
    ("legs", "hip_height"),
    ("arms", "arm"),
    ("shoulders", "biacromial_breadth"),
    ("hips", "hip_breadth"),
    ("chest", "chest_circumference"),
    ("waist", "waist_circumference"),
    ("neck", "neck_circumference"),
    ("biceps", "biceps_circumference_flexed"),
    ("thighs", "thigh_circumference"),
    ("calves", "calf_circumference"),
    ("head", "head_circumference"),
    ("hands", "hand_length"),
    ("feet", "foot_length"),
];

/// One sex's people: every dimension's name, and each person's values in that order.
struct People {
    names: Vec<String>,
    rows: Vec<Vec<f64>>,
    hip_share: f64,
    /// The least and the greatest stature among them (millimetres), and the least and the
    /// greatest body mass index (kilograms over the square of the stature in metres).
    statures: [f64; 2],
    indices: [f64; 2],
}

fn mean(xs: &[f64]) -> f64 {
    xs.iter().sum::<f64>() / xs.len() as f64
}

/// Reads a data file: the measured columns (from the one after the subject's id to
/// `wristheight`) and the age, each person a row, lengths in millimetres, weight in kilograms
/// (the file has it in tenths of one) and age in years, with the links after them.
fn read(path: &Path, sex: usize) -> Result<People, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().ok_or("an empty file")?.split(',').collect();
    let last = header.iter().position(|c| c.eq_ignore_ascii_case("wristheight")).ok_or("no wristheight column: is this an ANSUR II public data file?")?;
    let age = header.iter().position(|c| c.eq_ignore_ascii_case("Age")).ok_or("no Age column")?;
    let mut names = Vec::new();
    for column in &header[1..=last] {
        let name = readable(column).ok_or_else(|| format!("the column '{column}' is not made of the survey's words"))?;
        names.push(if name == "weight_kg" { "weight".to_owned() } else { name });
    }
    names.push("age".to_owned());
    let at = |name: &str| names.iter().position(|n| n == name).ok_or_else(|| format!("no {name} column"));
    let (weight, stature, trochanterion, bicristal) = (at("weight")?, at("stature")?, at("trochanterion_height")?, at("bicristal_breadth")?);
    let mut rows = Vec::new();
    for (n, line) in lines.enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let cells: Vec<&str> = line.split(',').collect();
        let number = |cell: &str| cell.trim().parse::<f64>().map_err(|_| format!("line {}: '{cell}' is not a number", n + 2));
        let mut row = Vec::with_capacity(names.len() + DERIVED.len());
        for cell in cells.get(1..=last).ok_or_else(|| format!("line {}: too few columns", n + 2))? {
            row.push(number(cell)?);
        }
        row.push(number(cells.get(age).ok_or_else(|| format!("line {}: no age", n + 2))?)?);
        row[weight] /= 10.0;
        rows.push(row);
    }
    if rows.len() < 100 {
        return Err(format!("{}: {} people: not a survey's file", path.display(), rows.len()));
    }
    let column = |i: usize| rows.iter().map(|r| r[i]).collect::<Vec<_>>();
    let above: Vec<f64> = rows.iter().map(|r| r[stature] - r[trochanterion]).collect();
    let hip_share = HIP_CENTRES * BISPINOUS[sex] * mean(&above) / mean(&column(bicristal));
    let span = |xs: Vec<f64>| [xs.iter().copied().fold(f64::INFINITY, f64::min), xs.iter().copied().fold(f64::NEG_INFINITY, f64::max)];
    let statures = span(column(stature));
    let indices = span(rows.iter().map(|r| r[weight] / (r[stature] / 1000.0).powi(2)).collect());
    let measured = names.len();
    for (name, _, _) in DERIVED {
        names.push((*name).to_owned());
    }
    for row in &mut rows {
        for (i, (name, _, of)) in DERIVED.iter().enumerate() {
            let value = of(&Person { names: &names[..measured + i], row, sex, hip_share });
            if !value.is_finite() {
                return Err(format!("the link '{name}' names a dimension that is not there yet"));
            }
            row.push(value);
        }
    }
    Ok(People { names, rows, hip_share, statures, indices })
}

/// What is kept of one dimension of one sex.
struct Row {
    mean: f64,
    sd: f64,
    p5: f64,
    p95: f64,
    /// On stature alone: `a + b * stature`, its standard error of estimate, and r.
    a: f64,
    b: f64,
    see: f64,
    r: f64,
    /// On stature and weight: `c0 + c1 * stature + c2 * weight`, and its standard error.
    c: [f64; 3],
    see2: f64,
    /// What one standard deviation of each trait adds.
    t: Vec<f64>,
    /// The spread the traits leave unexplained.
    left: f64,
}

/// A percentile by linear interpolation between the two nearest ranks (the survey's report
/// prints the same 5th and 95th for the dimensions compared).
fn percentile(sorted: &[f64], p: f64) -> f64 {
    let at = (sorted.len() - 1) as f64 * p;
    let (lo, hi) = (at.floor() as usize, (at.ceil() as usize).min(sorted.len() - 1));
    sorted[lo] + (sorted[hi] - sorted[lo]) * (at - lo as f64)
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// One dimension's row without its traits, and what its fit on stature and weight leaves of
/// each person.
fn row(ys: &[f64], stature: &[f64], weight: &[f64]) -> (Row, Vec<f64>) {
    let n = ys.len() as f64;
    let (my, ms, mw) = (mean(ys), mean(stature), mean(weight));
    let centred = |xs: &[f64], m: f64| xs.iter().map(|x| x - m).collect::<Vec<_>>();
    let (y, s, w) = (centred(ys, my), centred(stature, ms), centred(weight, mw));
    let (syy, sss, sww, ssw, ssy, swy) = (dot(&y, &y), dot(&s, &s), dot(&w, &w), dot(&s, &w), dot(&s, &y), dot(&w, &y));
    let mut sorted = ys.to_vec();
    sorted.sort_by(f64::total_cmp);
    // On stature alone.
    let b = ssy / sss;
    let a = my - b * ms;
    let left: f64 = (0..ys.len()).map(|i| (y[i] - b * s[i]).powi(2)).sum();
    let r = if syy > 0.0 { ssy / (syy * sss).sqrt() } else { 0.0 };
    // On stature and weight (two equations, by Cramer's rule: the two are far from collinear).
    let det = sss * sww - ssw * ssw;
    let (c1, c2) = ((ssy * sww - swy * ssw) / det, (swy * sss - ssy * ssw) / det);
    let residual: Vec<f64> = (0..ys.len()).map(|i| y[i] - c1 * s[i] - c2 * w[i]).collect();
    let row = Row {
        mean: my,
        sd: (syy / (n - 1.0)).sqrt(),
        p5: percentile(&sorted, 0.05),
        p95: percentile(&sorted, 0.95),
        a,
        b,
        see: (left / (n - 2.0)).sqrt(),
        r,
        c: [my - c1 * ms - c2 * mw, c1, c2],
        see2: (dot(&residual, &residual) / (n - 3.0)).sqrt(),
        t: Vec::new(),
        left: 0.0,
    };
    (row, residual)
}

/// Every dimension's row. The traits are taken out one after the other: each is what is left
/// of its dimension, every dimension's share of it is noted (as what one standard deviation
/// of the trait adds), and that share is removed before the next trait is read.
fn rows(people: &People) -> Result<Vec<Row>, String> {
    let at = |name: &str| people.names.iter().position(|n| n == name).ok_or_else(|| format!("no dimension named {name}"));
    let column = |i: usize| people.rows.iter().map(|r| r[i]).collect::<Vec<_>>();
    let (stature, weight) = (column(at("stature")?), column(at("weight")?));
    let n = people.rows.len() as f64;
    let (mut out, mut left): (Vec<Row>, Vec<Vec<f64>>) = (0..people.names.len()).map(|i| row(&column(i), &stature, &weight)).unzip();
    for (name, driver) in TRAITS {
        let u = left[at(driver)?].clone();
        let uu = dot(&u, &u);
        let sd = (uu / (n - 1.0)).sqrt();
        if sd.partial_cmp(&1e-6) != Some(std::cmp::Ordering::Greater) {
            return Err(format!("nothing is left of {driver} for the trait {name}"));
        }
        for (row, left) in out.iter_mut().zip(&mut left) {
            let share = dot(left, &u) / uu;
            row.t.push(share * sd);
            for (l, u) in left.iter_mut().zip(&u) {
                *l -= share * u;
            }
        }
    }
    for (row, left) in out.iter_mut().zip(&left) {
        row.left = (dot(left, left) / (n - 1.0)).sqrt();
    }
    Ok(out)
}

/// A number as the table prints it: six significant digits, which is past what a tape or a
/// caliper reads and keeps the file's lines comparable between two runs.
fn num(x: f64) -> String {
    if !x.is_finite() || x.abs() < 1e-9 {
        return "0.0".to_owned();
    }
    let digits = (5 - x.abs().log10().floor() as i32).clamp(0, 12) as usize;
    let text = format!("{x:.digits$}");
    let text = if text.contains('.') { text.trim_end_matches('0').trim_end_matches('.').to_owned() } else { text };
    if text.contains('.') {
        text
    } else {
        format!("{text}.0")
    }
}

fn table(name: &str, rows: &[Row]) -> String {
    let mut out = format!("pub(crate) static {name}: [Row; {}] = [\n", rows.len());
    for r in rows {
        let list = |xs: &[f64]| xs.iter().map(|x| num(*x)).collect::<Vec<_>>().join(", ");
        out += &format!(
            "    Row {{ mean: {}, sd: {}, p5: {}, p95: {}, a: {}, b: {}, see: {}, r: {}, c: [{}], see2: {}, t: [{}], left: {} }},\n",
            num(r.mean),
            num(r.sd),
            num(r.p5),
            num(r.p95),
            num(r.a),
            num(r.b),
            num(r.see),
            num(r.r),
            list(&r.c),
            num(r.see2),
            list(&r.t),
            num(r.left)
        );
    }
    out + "];\n"
}

/// The whole of `ansur_data.rs`, from the two data files.
pub fn tables(female: &Path, male: &Path) -> Result<String, String> {
    let (f, m) = (read(female, 0)?, read(male, 1)?);
    if f.names != m.names {
        return Err("the two files do not have the same columns".to_owned());
    }
    let (fr, mr) = (rows(&f)?, rows(&m)?);
    let measured = f.names.len() - DERIVED.len();
    let mut out = String::new();
    out += "//! The survey's dimensions and how each is spread, by sex. **Computed, not typed**: this file\n";
    out += "//! is what `cargo run -p specula-examen -- corpus tables FEMALE.csv MALE.csv` prints for the two\n";
    out += "//! public data files of ANSUR II (tools/examen/src/corpus.rs says where they are, and\n";
    out += "//! crate::ansur what each number of a row is). Lengths in millimetres, weight in kilograms,\n";
    out += "//! age in years.\n\n";
    out += "use crate::ansur::Row;\n\n";
    out += &format!("/// How many people each sex's rows are computed from (women, men).\npub(crate) const PEOPLE: [u32; 2] = [{}, {}];\n\n", f.rows.len(), m.rows.len());
    out += &format!(
        "/// The hip centres' spacing as a share of the bicristal breadth (women, men).\npub(crate) const HIP_SHARE: [f64; 2] = [{}, {}];\n\n",
        num(f.hip_share),
        num(m.hip_share)
    );
    let pair = |p: [f64; 2]| format!("[{}, {}]", num(p[0]), num(p[1]));
    out += &format!(
        "/// The least and the greatest stature among them, millimetres (women, men).
pub(crate) const STATURES: [[f64; 2]; 2] = [{}, {}];

",
        pair(f.statures),
        pair(m.statures)
    );
    out += &format!(
        "/// The least and the greatest body mass index among them: kilograms over the square of the
/// stature in metres (women, men).
pub(crate) const MASS_INDICES: [[f64; 2]; 2] = [{}, {}];

",
        pair(f.indices),
        pair(m.indices)
    );
    out += &format!(
        "/// The dimensions, in the order of the rows: the survey's {} measured ones and the age, then\n/// {} links made of them.\npub(crate) const NAMES: [&str; {}] = [\n",
        measured - 1,
        DERIVED.len(),
        f.names.len()
    );
    for name in &f.names {
        out += &format!("    \"{name}\",\n");
    }
    out += "];\n\n";
    out += &format!("/// What each link is made of.\npub(crate) const LINKS: [(&str, &str); {}] = [\n", DERIVED.len());
    for (name, how, _) in DERIVED {
        out += &format!("    (\"{name}\", \"{how}\"),\n");
    }
    out += "];\n\n";
    out += &format!("/// The traits, in the order they are taken out, and the dimension each is read on.\npub(crate) const TRAITS: [(&str, &str); {}] = [\n", TRAITS.len());
    for (name, driver) in TRAITS {
        out += &format!("    (\"{name}\", \"{driver}\"),\n");
    }
    out += "];\n\n";
    out += &table("FEMALE", &fr);
    out += "\n";
    out += &table("MALE", &mr);
    Ok(out)
}

/// `examen corpus tables FEMALE.csv MALE.csv`.
pub fn main(args: crate::cli::Args) -> i32 {
    let rest = args.rest();
    match rest.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["tables", female, male] => match tables(Path::new(female), Path::new(male)) {
            Ok(text) => {
                print!("{text}");
                0
            }
            Err(e) => {
                println!("{e}");
                1
            }
        },
        _ => {
            println!("examen corpus tables FEMALE.csv MALE.csv (the two public data files of ANSUR II)");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_columns_name_is_cut_into_its_words_one_way() {
        assert_eq!(readable("acromionradialelength").as_deref(), Some("acromion_radiale_length"));
        assert_eq!(readable("SubjectId"), None);
        assert_eq!(readable("interscyei").as_deref(), Some("interscye_i"));
        assert_eq!(readable("interscyeii").as_deref(), Some("interscye_ii"));
        assert_eq!(readable("balloffootlength").as_deref(), Some("ball_of_foot_length"));
        assert_eq!(readable("thumbtipreach").as_deref(), Some("thumbtip_reach"));
        assert_eq!(readable("weightkg").as_deref(), Some("weight_kg"));
        assert_eq!(readable("heelanklecircumference").as_deref(), Some("heel_ankle_circumference"));
    }

    /// A fit is held against one worked by hand: five people on a plane, and off it.
    #[test]
    fn the_statistics_are_what_a_hand_computes() {
        let stature = [1600.0, 1650.0, 1700.0, 1750.0, 1800.0];
        let weight = [55.0, 70.0, 60.0, 80.0, 75.0];
        // Exactly 0.2 of stature and 1.5 a kilogram, plus 10.
        let ys: Vec<f64> = (0..5).map(|i| 10.0 + 0.2 * stature[i] + 1.5 * weight[i]).collect();
        let (r, left) = row(&ys, &stature, &weight);
        assert!((r.c[1] - 0.2).abs() < 1e-9 && (r.c[2] - 1.5).abs() < 1e-9 && (r.c[0] - 10.0).abs() < 1e-6 && r.see2 < 1e-6, "{:?} {}", r.c, r.see2);
        assert!(left.iter().all(|l| l.abs() < 1e-9));
        assert!((r.mean - mean(&ys)).abs() < 1e-9 && r.p5 < r.mean && r.mean < r.p95);
        // Stature against itself: the line is itself, and nothing is left.
        let (s, _) = row(&stature, &stature, &weight);
        assert!((s.b - 1.0).abs() < 1e-12 && s.a.abs() < 1e-9 && s.see < 1e-9 && (s.r - 1.0).abs() < 1e-12);
        assert!((s.sd - 79.0569415).abs() < 1e-6, "{}", s.sd);
        assert_eq!((percentile(&stature, 0.05), percentile(&stature, 0.5), percentile(&stature, 1.0)), (1610.0, 1700.0, 1800.0));
        let printed = [num(1628.4712), num(0.191123456), num(-34.04), num(0.0), num(1986.0), num(1e-13)];
        assert_eq!(printed, ["1628.47", "0.191123", "-34.04", "0.0", "1986.0", "0.0"]);
    }

    /// Traits are taken out in order: a trait's own dimension keeps nothing after it, the
    /// shares are independent (their squares add up to what the fit left), and a sum of two
    /// dimensions has the sum of their shares, which is what keeps a body's links adding up.
    #[test]
    fn traits_are_taken_out_in_order_and_add_up() {
        let noise = |i: usize, k: usize| (((i * 7 + k * 13 + i * i * (k + 1)) % 23) as f64) - 11.0;
        let mut names: Vec<String> = ["stature", "weight"].iter().map(|s| (*s).to_owned()).collect();
        names.extend(TRAITS.iter().map(|(_, driver)| (*driver).to_owned()));
        names.push("sum".to_owned());
        let people_count = 60;
        let measured: Vec<Vec<f64>> = (0..people_count)
            .map(|i| {
                let mut row = vec![1600.0 + 5.0 * i as f64 + noise(i, 1), 60.0 + 0.3 * i as f64 + noise(i, 2)];
                for k in 0..TRAITS.len() {
                    row.push(300.0 + 0.1 * row[0] + noise(i, k + 3) * (k as f64 + 1.0) + noise(i + k, 5));
                }
                row.push(row[2] + row[3]);
                row
            })
            .collect();
        let people = People { names, rows: measured, hip_share: 0.6, statures: [0.0; 2], indices: [0.0; 2] };
        let out = rows(&people).unwrap();
        let at = |name: &str| people.names.iter().position(|n| n == name).unwrap();
        let n = people_count as f64;
        for (k, (_, driver)) in TRAITS.iter().enumerate() {
            let row = &out[at(driver)];
            assert!(row.t[k].abs() > 1e-6, "{driver} has nothing of its own trait");
            assert!(row.t[k + 1..].iter().all(|t| t.abs() < 1e-8) && row.left < 1e-8, "{driver} keeps something after its trait");
            let whole = row.t.iter().map(|t| t * t).sum::<f64>().sqrt();
            assert!((whole - row.see2 * ((n - 3.0) / (n - 1.0)).sqrt()).abs() < 1e-8, "{driver}: {whole} against {}", row.see2);
        }
        let (a, b, sum) = (&out[2], &out[3], &out[at("sum")]);
        for k in 0..TRAITS.len() {
            assert!((sum.t[k] - a.t[k] - b.t[k]).abs() < 1e-8);
        }
        assert!((sum.c[1] - a.c[1] - b.c[1]).abs() < 1e-9 && (sum.mean - a.mean - b.mean).abs() < 1e-9 && sum.left < 1e-8);
    }

    /// The links are worked out in order, each from what is before it, and the chain from the
    /// floor to the top of the head adds up to the stature.
    #[test]
    fn every_link_is_made_of_what_is_there() {
        let measured = [
            ("trochanterion_height", 845.35),
            ("lateral_femoral_epicondyle_height", 465.94),
            ("lateral_malleolus_height", 62.75),
            ("stature", 1628.47),
            ("tragion_top_of_head", 126.5),
            ("cervicale_height", 1395.67),
            ("tenth_rib_height", 1056.3),
            ("iliocristale_height", 995.7),
            ("biacromial_breadth", 365.35),
            ("sitting_height", 856.63),
            ("acromial_height", 1335.1),
            ("acromion_radiale_length", 311.2),
            ("radiale_stylion_length", 241.33),
            ("hand_length", 181.08),
            ("tibial_height", 437.77),
            ("bicristal_breadth", 273.3),
        ];
        // A woman at the survey's means (millimetres).
        let mut names: Vec<String> = measured.iter().map(|(n, _)| (*n).to_owned()).collect();
        let mut row: Vec<f64> = measured.iter().map(|(_, v)| *v).collect();
        for (name, _, of) in DERIVED {
            let value = of(&Person { names: &names, row: &row, sex: 0, hip_share: 0.6 });
            assert!(value.is_finite() && value > 0.0, "{name} is {value}");
            names.push((*name).to_owned());
            row.push(value);
        }
        let v = |name: &str| row[names.iter().position(|n| n == name).unwrap()];
        let chain = v("lateral_malleolus_height") + v("shank") + v("thigh") + v("trunk") + v("vertex_to_cervicale");
        assert!((chain - v("stature")).abs() < 1e-9, "{chain}");
        // Between centres the upper arm is 0.159 of stature on de Leva's women and 0.163 on
        // the survey's; the shank to the lateral malleolus is 0.249 on both.
        assert!((v("upper_arm") / v("stature") - 0.163).abs() < 0.001, "{}", v("upper_arm") / v("stature"));
        assert!((v("shank") / v("stature") - 0.249).abs() < 0.001, "{}", v("shank") / v("stature"));
        assert!((v("hip_spacing") - 0.6 * 273.3).abs() < 1e-9);
    }
}
