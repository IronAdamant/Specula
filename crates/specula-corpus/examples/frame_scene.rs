//! Prints a body's frame as a scene for the engine: a ball at each joint and a rod for each
//! bone, as the arguments of one `world_batch`. It is the body before it has a surface, for
//! looking at in the engine's window.
//!
//! ```text
//! cargo run -q -p specula-corpus --example frame_scene -- /fig_a 0 female 1.70 0 legs=1.5 > a.json
//! specula call world_batch @a.json
//! ```
//!
//! The arguments are the scene path of the figure's group, how far along X it stands (metres),
//! then a body as the `body` example takes it: a sex, a stature, a build, traits as
//! `name=value`, `age=years`.

use specula_corpus::{Body, Frame, Joint, Sex, Side};

fn body(args: &[String]) -> Result<Body, String> {
    let sex = args.first().and_then(|s| Sex::parse(s)).ok_or("the sex is female or male")?;
    let Some(stature) = args.get(1) else { return Ok(Body::typical(sex)) };
    let stature: f64 = stature.parse().map_err(|_| "the stature is a number of metres")?;
    let build: f64 = args.get(2).map_or(Ok(0.0), |b| b.parse()).map_err(|_| "the build is a number")?;
    let mut body = Body::with_build(sex, stature, build).map_err(|e| e.to_string())?;
    let mut traits = Vec::new();
    for arg in args.iter().skip(3) {
        let (name, value) = arg.split_once('=').ok_or("a trait is written name=value")?;
        let value: f64 = value.parse().map_err(|_| format!("{name}: not a number"))?;
        if name == "age" {
            body = body.with_age(value).map_err(|e| e.to_string())?;
        } else {
            traits.push((name, value));
        }
    }
    body.with_traits(&traits).map_err(|e| e.to_string())
}

/// The bones drawn: each between two joints.
fn bones() -> Vec<(Joint, Joint)> {
    let mut out = vec![(Joint::Pelvis, Joint::Waist), (Joint::Waist, Joint::Ribs), (Joint::Ribs, Joint::Chest), (Joint::Chest, Joint::Neck), (Joint::Neck, Joint::Head), (Joint::Head, Joint::Vertex)];
    out.push((Joint::Shoulder(Side::Left), Joint::Shoulder(Side::Right)));
    out.push((Joint::Hip(Side::Left), Joint::Hip(Side::Right)));
    for side in Side::BOTH {
        out.extend([
            (Joint::Shoulder(side), Joint::Elbow(side)),
            (Joint::Elbow(side), Joint::Wrist(side)),
            (Joint::Wrist(side), Joint::Knuckle(side)),
            (Joint::Knuckle(side), Joint::Fingertip(side)),
            (Joint::Hip(side), Joint::Knee(side)),
            (Joint::Knee(side), Joint::Ankle(side)),
            (Joint::Ankle(side), Joint::Heel(side)),
            (Joint::Heel(side), Joint::Ball(side)),
            (Joint::Ball(side), Joint::Toe(side)),
            (Joint::Ankle(side), Joint::Ball(side)),
        ]);
    }
    out
}

/// One rod between two points, as a spawn. A rod of the engine stands along Y: it is turned
/// about X by the angle its direction makes with Y in the YZ plane, then about Z for what is
/// left of its lean toward X. For a rod along an axis that is 0 or 90 degrees exactly.
fn rod(group: &str, name: &str, a: [f64; 3], b: [f64; 3], thick: f64, color: &str) -> String {
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let in_yz = (d[1] * d[1] + d[2] * d[2]).sqrt();
    let about_x = specula_core::dmath::atan2_f64(d[2], d[1]).to_degrees();
    let about_z = specula_core::dmath::atan2_f64(-d[0], in_yz).to_degrees();
    let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0];
    format!(
        "{{\"op\":\"spawn\",\"shape\":\"cylinder\",\"path\":\"{group}/{name}\",\"position\":[{:.4},{:.4},{:.4}],\"rotation_deg\":[{about_x:.3},0,{about_z:.3}],\"scale\":[{thick:.4},{length:.4},{thick:.4}],\"color\":\"{color}\"}}",
        mid[0], mid[1], mid[2]
    )
}

fn scene(group: &str, x: f64, frame: &Frame) -> String {
    let at = |joint: Joint| {
        let p = frame.at(joint);
        [p[0] + x, p[1], p[2]]
    };
    let mut ops = vec![format!("{{\"op\":\"spawn\",\"shape\":\"group\",\"path\":\"{group}\"}}")];
    for (from, to) in bones() {
        ops.push(rod(group, &format!("{}__{}", from.name(), to.name()), at(from), at(to), 0.022, "#c9c2b4"));
    }
    for joint in Joint::all() {
        let p = at(joint);
        let (size, color) = match joint {
            Joint::Vertex | Joint::Fingertip(_) | Joint::Toe(_) | Joint::Heel(_) | Joint::Ball(_) | Joint::Knuckle(_) => (0.03, "#8a8f98"),
            Joint::Waist | Joint::Ribs | Joint::Chest | Joint::Head => (0.04, "#5a8fd0"),
            _ => (0.055, "#e0703a"),
        };
        ops.push(format!("{{\"op\":\"spawn\",\"shape\":\"sphere\",\"path\":\"{group}/{}\",\"position\":[{:.4},{:.4},{:.4}],\"scale\":{size},\"color\":\"{color}\"}}", joint.name(), p[0], p[1], p[2]));
    }
    format!("{{\"ops\":[\n{}\n]}}", ops.join(",\n"))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = (|| -> Result<(String, f64, Body), String> {
        let group = args.first().filter(|g| g.starts_with('/')).ok_or("the first argument is the figure's scene path, such as /fig_a")?.clone();
        let x: f64 = args.get(1).ok_or("the second argument is where it stands along X, metres")?.parse().map_err(|_| "where it stands is a number")?;
        Ok((group, x, body(&args[2..])?))
    })();
    match parsed {
        Ok((group, x, body)) => println!("{}", scene(&group, x, &body.frame())),
        Err(why) => {
            println!("{why}\n\nframe_scene PATH X SEX [STATURE_M [BUILD [TRAIT=VALUE ...] [age=YEARS]]]");
            std::process::exit(2);
        }
    }
}
