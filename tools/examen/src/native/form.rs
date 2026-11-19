//! `form_sheet_check`: the form views on the asset studio's contact sheets (docs/design/FORM.md),
//! through a running engine as an agent with a shell uses it. A new project gets one small
//! recipe; a sheet of it is asked for with a colour pass, two form passes and the reference
//! figure; and what comes back is held to what the design says:
//!
//! - a form tile and a silhouette tile are framed as the colour tile of their view is (the
//!   references' room kept, the references left out): the subject is where the view's box
//!   says, read from the pixels (the packet gives every tile of a view the one box, so a tile
//!   framed otherwise shows its subject outside it);
//! - a form tile shows the subject against the form views' neutral ground and a silhouette
//!   against white, and nothing of the stage (no floor, no figure);
//! - clay is a warm grey, lit; normals are colours;
//! - a line-up asked for in clay is shape alone too: the subject where each view's box says, on
//!   the form views' ground with no floor, and the reference figure and cube beside it (a
//!   line-up is for scale: its references stay), and the packet says which view it is;
//! - no frame of either compiled a pipeline (the form views' set is built on a worker).

use specula_core::Json;

use super::{at, expect, text, Cx};

/// A post with a ball on it: a hard-edged part and a round one, taller than it is wide, so the
/// reference figure beside it changes the framing.
const RECIPE: &str = r#"{
  "recipe": 1,
  "name": "post",
  "category": "prop",
  "description": "The form sheet check's subject: a square post with a ball on top.",
  "seed": 1,
  "collider": "aabb",
  "parts": [
    { "name": "shaft", "material": "matte", "color": "stone",
      "shape": { "op": "box", "size": [0.3, 0.9, 0.3] } },
    { "name": "ball", "material": "painted", "color": "accent",
      "shape": { "op": "sphere", "radius": 0.24, "subdivisions": 3 },
      "ops": [ { "op": "move", "by": [0, 0.88, 0] } ] }
  ]
}
"#;

/// The form views' ground (`sight::FORM_BACKGROUND`, as the bytes a capture holds).
const GROUND: [u8; 3] = [23, 23, 26];

fn rect(tile: &Json, key: &str) -> Option<[u32; 4]> {
    let a = at(tile, key)?.as_array()?;
    let v: Vec<u32> = a.iter().filter_map(Json::as_u64).map(|x| x as u32).collect();
    <[u32; 4]>::try_from(v).ok()
}

pub fn form_sheet(cx: &mut Cx) -> Result<String, String> {
    let lead = Some(("lead", "lead"));
    let (port, settings) = (cx.port, cx.settings());
    let projects = cx.out.join("p");
    let project = projects.join("formsheet");
    if project.exists() {
        return Err(format!("{} is there already: give the check an output folder of its own (it never deletes one)", project.display()));
    }
    let (code, made) = cx.cli(&["new", "formsheet", "--in", &projects.to_string_lossy(), "--settings", &settings])?;
    expect(code == 0, "`specula new` did not make the project", &made)?;
    let recipes = project.join("assets").join("recipes");
    std::fs::create_dir_all(&recipes).and_then(|()| std::fs::write(recipes.join("post.json"), RECIPE)).map_err(|e| format!("{}: {e}", recipes.display()))?;

    cx.start(&project, port, &["--lead", "lead"])?;
    let args = r#"{"recipe": "recipes/post.json", "views": ["front", "three_quarter"], "passes": ["colour", "clay", "normals", "silhouette"], "references": true, "tile": 192, "response_format": "detailed"}"#;
    let sheet = cx.call(port, lead, "studio_sheet", args)?;
    let tiles = at(&sheet, "subjects.0.tiles").and_then(Json::as_array).ok_or_else(|| format!("the sheet's answer has no tiles: {}", super::short(&sheet)))?.to_vec();
    expect(tiles.len() == 8, "a sheet of two views and four passes does not have eight tiles", &sheet)?;
    let path = text(&sheet, "image.path").ok_or("the sheet's answer has no picture")?.to_owned();
    let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    let image = specula_core::png::decode(&bytes).map_err(|e| format!("{path}: {e:?}"))?;
    let px = |x: u32, y: u32| -> [u8; 3] {
        let i = ((y * image.width + x) * 4) as usize;
        [image.rgba[i], image.rgba[i + 1], image.rgba[i + 2]]
    };
    let is_ground = |p: [u8; 3]| p.iter().zip(GROUND).all(|(a, b)| a.abs_diff(b) <= 2);

    let mut seen = 0;
    for view in ["front", "three_quarter"] {
        let of = |pass: &str| tiles.iter().find(|t| text(t, "view") == Some(view) && text(t, "pass") == Some(pass));
        of("colour").ok_or_else(|| format!("no colour tile of the {view} view"))?;
        for pass in ["clay", "normals", "silhouette"] {
            // (A silhouette is black on white.)
            let is_ground = |p: [u8; 3]| if pass == "silhouette" { p.iter().all(|c| *c > 200) } else { is_ground(p) };
            let tile = of(pass).ok_or_else(|| format!("no {pass} tile of the {view} view"))?;
            let (at_, in_) = (rect(tile, "tile"), rect(tile, "box"));
            let (Some([tx, ty, tw, th]), Some([bx, by, bw, bh])) = (at_, in_) else { return Err(format!("the {view} {pass} tile has no rectangle or box: {}", super::short(tile))) };
            // Framed as its colour tile is: the subject is where the view's box says, on its
            // ground, with nothing else drawn:
            // most of the box's middle column is the subject, and what lies clear of the box
            // (three pixels out) is ground. (The tile's bottom strip holds its label.)
            let (mut inside, mut outside, mut stray) = (0u32, 0u32, 0u32);
            for y in ty..ty + th.saturating_sub(14) {
                for x in tx..tx + tw {
                    let (lx, ly) = (x - tx, y - ty);
                    let within = lx + 3 >= bx && lx < bx + bw + 3 && ly + 3 >= by && ly < by + bh + 3;
                    if !within {
                        outside += 1;
                        stray += u32::from(!is_ground(px(x, y)));
                    } else if lx >= bx + bw / 2 - 2 && lx <= bx + bw / 2 + 2 && ly >= by + 2 && ly + 2 < by + bh {
                        inside += u32::from(!is_ground(px(x, y)));
                    }
                }
            }
            let column = 5 * bh.saturating_sub(4);
            if inside * 10 < column * 8 || stray * 200 > outside {
                return Err(format!("the {view} {pass} tile: {inside} of the {column} pixels down the middle of its box are the subject, and {stray} of the {outside} outside its box are not ground (tile {:?}, box {:?}; {path})", [tx, ty, tw, th], [bx, by, bw, bh]));
            }
            // What the view is: clay a lit warm grey on the shaft, normals a colour.
            let mid = px(tx + bx + bw / 2, ty + by + bh * 3 / 4);
            let right = match pass {
                "clay" => mid[0] >= mid[2] && mid[1] > 50 && mid[0].abs_diff(mid[1]) < 40,
                "silhouette" => mid.iter().all(|c| *c < 40),
                _ => mid[0].abs_diff(mid[1]) > 8 || mid[1].abs_diff(mid[2]) > 8,
            };
            if !right {
                return Err(format!("the {view} {pass} tile's shaft is {mid:?}: not what the {pass} view shows ({path})"));
            }
            seen += 1;
        }
    }

    // The line-up as shape alone.
    let lineup = cx.call(port, lead, "studio_lineup", r#"{"subjects": ["recipes/post.json"], "form": "clay", "width": 640, "response_format": "detailed"}"#)?;
    expect(text(&lineup, "form") == Some("clay") && at(&lineup, "rig").is_some_and(Json::is_null), "a line-up in clay does not say so (form: clay, and no rig)", &lineup)?;
    let lpath = text(&lineup, "image.path").ok_or("the line-up's answer has no picture")?.to_owned();
    let lbytes = std::fs::read(&lpath).map_err(|e| format!("{lpath}: {e}"))?;
    let limage = specula_core::png::decode(&lbytes).map_err(|e| format!("{lpath}: {e:?}"))?;
    let lpx = |x: u32, y: u32| -> [u8; 3] {
        let i = ((y * limage.width + x) * 4) as usize;
        [limage.rgba[i], limage.rgba[i + 1], limage.rgba[i + 2]]
    };
    let boxes = at(&lineup, "subjects.0.boxes").and_then(Json::as_array).ok_or_else(|| format!("the line-up's subject has no boxes: {}", super::short(&lineup)))?.to_vec();
    expect(boxes.len() == 2, "a line-up does not give its subject a box in each of its two views", &lineup)?;
    for b in &boxes {
        let view = text(b, "view").unwrap_or("?").to_owned();
        let Some([bx, by, bw, bh]) = rect(b, "box") else { return Err(format!("the line-up's {view} box is not four numbers: {}", super::short(b))) };
        // The subject fills the middle column of its box, in clay (a lit warm grey on the shaft).
        let column: u32 = (by + 2..by + bh.saturating_sub(2)).map(|y| u32::from(!is_ground(lpx(bx + bw / 2, y)))).sum();
        let mid = lpx(bx + bw / 2, by + bh * 3 / 4);
        let clay = mid[0] >= mid[2] && mid[1] > 50 && mid[0].abs_diff(mid[1]) < 40;
        // No floor: a lit line-up's floor runs to both edges of the picture all the way down the
        // view. Here the two edge columns are ground for every row the subject spans and a
        // little under it, and so are the corners above.
        let under = (by..(by + bh + 12).min(limage.height)).map(|y| u32::from(!is_ground(lpx(1, y))) + u32::from(!is_ground(lpx(limage.width - 2, y)))).sum::<u32>();
        let corner = is_ground(lpx(2, by.saturating_sub(by.min(4)) + 2)) && is_ground(lpx(limage.width - 3, by + 2));
        // The references are drawn: clear of the subject's box, in the rows it spans, something
        // stands that is not ground (the figure is taller than the post and to its left).
        let beside: u32 = (by..by + bh).map(|y| (0..limage.width).filter(|x| (*x + 6 < bx || *x > bx + bw + 6) && !is_ground(lpx(*x, y))).count() as u32).sum();
        if column * 10 < bh.saturating_sub(4) * 8 || !clay || under > 0 || !corner || beside < 200 {
            return Err(format!(
                "the line-up's {view} view in clay: {column} of {} pixels down the middle of the subject's box are the subject (its shaft {mid:?}); {under} pixels of the picture's two edge columns beside it are not ground (of {} rows: a floor would fill them); the corners are ground: {corner}; {beside} pixels beside its box are not ground (the references) (box {:?}; {lpath})",
                bh.saturating_sub(4),
                bh + 12,
                [bx, by, bw, bh]
            ));
        }
        seen += 1;
    }

    // No frame of it compiled a pipeline: the form views' set was built on a worker.
    let perf = cx.call(port, lead, "perf_frame", "{}")?;
    let hitched = at(&perf, "errors").and_then(Json::as_array).is_some_and(|e| e.iter().any(|x| super::short(x).contains("pipeline_created_after_warm")));
    expect(!hitched, "a frame compiled a pipeline after the engine was warm", &perf)?;
    cx.stop(port)?;
    Ok(format!("{seen} form tiles, silhouette tiles and line-up views in clay show what their view's box says, the line-up with its references and no floor; no pipeline compiled by a frame ({path}, {lpath})"))
}
