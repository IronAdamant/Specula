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
//! - no frame of the sheet compiled a pipeline (the form views' set is built on a worker).

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
      "ops": [ { "op": "move", "by": [0, 1.1, 0] } ] }
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

    // No frame of it compiled a pipeline: the form views' set was built on a worker.
    let perf = cx.call(port, lead, "perf_frame", "{}")?;
    let hitched = at(&perf, "errors").and_then(Json::as_array).is_some_and(|e| e.iter().any(|x| super::short(x).contains("pipeline_created_after_warm")));
    expect(!hitched, "a frame compiled a pipeline after the engine was warm", &perf)?;
    cx.stop(port)?;
    Ok(format!("{seen} form and silhouette tiles show the subject alone where their view's box says; no pipeline compiled by a frame ({path})"))
}
