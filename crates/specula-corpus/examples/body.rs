//! Prints a body: its frame, what its parts weigh, and what the checks say of it.
//!
//! ```text
//! cargo run -p specula-corpus --example body -- female 1.70 0 legs=1.5 shoulders=-1 age=40
//! cargo run -p specula-corpus --example body -- male            (the typical one)
//! cargo run -p specula-corpus --example body -- traits          (the traits and what each moves most)
//! ```
//!
//! The arguments are a sex, a stature in metres, a build (0 is the usual weight for the
//! stature), then traits as `name=value` in standard deviations, and `age=years`.

use specula_corpus::{ansur, balance, parts, proportions, Body, BodyError, Joint, Sex};

fn body(args: &[String]) -> Result<Body, String> {
    let sex = args.first().and_then(|s| Sex::parse(s)).ok_or("the first argument is female or male")?;
    let Some(stature) = args.get(1) else { return Ok(Body::typical(sex)) };
    let stature: f64 = stature.parse().map_err(|_| "the stature is a number of metres")?;
    let build: f64 = args.get(2).map_or(Ok(0.0), |b| b.parse()).map_err(|_| "the build is a number")?;
    let said = |e: BodyError| format!("{}: {e}", e.code());
    let mut body = Body::with_build(sex, stature, build).map_err(said)?;
    for arg in args.iter().skip(3) {
        let (name, value) = arg.split_once('=').ok_or("a trait is written name=value")?;
        let value: f64 = value.parse().map_err(|_| format!("{name}: not a number"))?;
        body = if name == "age" { body.with_age(value) } else { body.with_trait(name, value) }.map_err(said)?;
    }
    Ok(body)
}

fn traits() {
    for sex in Sex::BOTH {
        println!("{}", sex.name());
        for (k, (name, driver)) in ansur::traits().iter().enumerate() {
            let mut moved: Vec<(&str, f64)> = ansur::names().iter().map(|n| (*n, ansur::row(sex, n).map_or(0.0, |r| r.t[k] / r.sd.max(1e-9)))).collect();
            moved.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));
            let most: Vec<String> = moved.iter().take(6).map(|(n, share)| format!("{n} {share:+.2}")).collect();
            println!("  {name:10} read on {driver}; one deviation moves, in each one's own deviations: {}", most.join(", "));
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("traits") {
        return traits();
    }
    let body = match body(&args) {
        Ok(body) => body,
        Err(why) => {
            println!("{why}\n\nbody SEX [STATURE_M [BUILD [TRAIT=VALUE ...] [age=YEARS]]]   or   body traits");
            std::process::exit(2);
        }
    };
    let sex = body.sex();
    println!("{} of {:.3} m and {:.1} kg (build {:+.2}), {:.0} years", sex.name(), body.stature(), body.weight(), body.build(), body.age());
    let frame = body.frame();
    println!("\njoints (metres; +Y up, facing +Z, left is +X)");
    for joint in Joint::all() {
        let [x, y, z] = frame.at(joint);
        println!("  {:12} {x:+.3} {y:.3} {z:+.3}", joint.name());
    }
    println!("\nparts (share of the mass, kilograms, centre)");
    for part in parts(sex, &frame) {
        let side = part.side.map_or("", |s| if s.sign() > 0.0 { " l" } else { " r" });
        println!("  {:14} {:5.2}% {:6.2} kg at {:+.3} {:.3} {:+.3}", format!("{}{side}", part.name), part.share * 100.0, part.share * body.weight(), part.centre[0], part.centre[1], part.centre[2]);
    }
    println!("\nproportions against {} of this stature (measured, expected, how many spreads off)", if sex == Sex::Female { "women" } else { "men" });
    for p in proportions(sex, &frame).iter().filter(|p| p.side.is_none_or(|s| s.sign() > 0.0)) {
        println!("  {:14} {:.3} {:.3} {:+.2}", p.name, p.measured, p.expected, p.z);
    }
    let stands = balance(sex, &frame);
    println!("\nbalance: the centre of mass is at {:+.3} {:.3} {:+.3} ({:.1}% of the stature up), {:.3} m inside what the feet cover", stands.centre[0], stands.centre[1], stands.centre[2], stands.centre[1] / body.stature() * 100.0, stands.margin);
    println!("\ngirths and breadths (millimetres; the spread the body does not say)");
    for name in ["head_circumference", "neck_circumference", "bideltoid_breadth", "chest_circumference", "waist_circumference", "hip_breadth", "buttock_circumference", "thigh_circumference", "calf_circumference", "ankle_circumference", "biceps_circumference_flexed", "forearm_circumference_flexed", "wrist_circumference"] {
        println!("  {name:30} {:7.1} ({:.1})", body.dimension(name).unwrap_or(0.0), body.spread(name).unwrap_or(0.0));
    }
}
