//! `sight_compare`: before/after captures. An agent marks a "before" (the capture, its subjects
//! and the camera that took it) and later diffs the world as it is now, through the very same
//! camera, against it.
//!
//! The diff is numbers first (changed pixel %, mean and max difference, the boxes of changed
//! regions, and which subjects were added, removed, moved or changed visibility) and one image:
//! before | after | heat, side by side (scaled to stay within the capture size limit). In the
//! heat panel unchanged pixels show the "after" image dimmed to grey and changed ones glow from
//! red (small) to yellow and white (large).
//!
//! Marks are kept per agent identity: at most [`MAX_MARKS_PER_AGENT`] each, for at most
//! [`MAX_MARK_OWNERS`] identities, and [`MAX_MARK_BYTES`] in all (pixels, subjects and ids
//! counted); the least recently used goes first. A mark keeps only its on-screen subjects, plus
//! the ids of every entity at the time (8 bytes each), to tell new entities from old ones.

use crate::sight::Camera;
use specula_core::Json;
use std::collections::BTreeMap;

pub const MAX_MARKS_PER_AGENT: usize = 8;
/// Identities whose marks are kept (the least recently used identity's go first).
pub const MAX_MARK_OWNERS: usize = 32;
/// Every agent's marks together: pixels, subjects and entity ids (bytes, estimated).
pub const MAX_MARK_BYTES: usize = 256 * 1024 * 1024;
/// Heap bytes per stored subject besides its path: the record (about 100 bytes), the map node
/// around it and the allocators' rounding (measured: about 1.4x the record, rounded up).
const SUBJECT_BYTES: usize = 176;
/// Changed regions reported.
pub const MAX_REGIONS: usize = 16;
/// Subject changes listed per kind.
pub const MAX_SUBJECT_CHANGES: usize = 25;
/// Side of the cells changed pixels are grouped into for regions.
const CELL: u32 = 8;

/// How a diff renders "after": the mark's own settings.
#[derive(Clone, Debug)]
pub struct MarkView {
    pub camera: Camera,
    pub width: u32,
    pub height: u32,
    pub include_ui: bool,
    pub lod: Option<usize>,
    pub time_s: Option<f32>,
    pub overlays: crate::sight_overlay::Spec,
    pub ids: bool,
}

/// What a mark remembers besides its view.
pub struct MarkData {
    pub rgba: Vec<u8>,
    /// The subjects on screen at the mark.
    pub subjects: BTreeMap<u64, SubjectState>,
    /// Every entity id at the mark, sorted.
    pub ids: Vec<u64>,
    pub frame: u64,
    pub tick: u64,
    pub png_path: Option<String>,
}

/// A remembered "before".
pub struct Mark {
    pub view: MarkView,
    pub data: MarkData,
    used: u64,
}

impl Mark {
    /// Memory it holds, estimated.
    fn bytes(&self) -> usize {
        let d = &self.data;
        d.rgba.len() + d.ids.len() * 8 + d.subjects.values().map(|s| s.path.len() + SUBJECT_BYTES).sum::<usize>()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubjectState {
    pub path: String,
    /// The projected box (`screen`).
    pub projected: Option<[u32; 4]>,
    /// The ID buffer's exact box (`screen_box_exact`).
    pub exact: Option<[u32; 4]>,
    pub visible_px: Option<u64>,
    pub position: [f32; 3],
}

impl SubjectState {
    /// From a subject's JSON (as in the packet).
    pub fn from_json(j: &Json) -> Option<(u64, SubjectState)> {
        let id = j.get("id")?.as_u64()?;
        let rect = |key: &str| -> Option<[u32; 4]> {
            let a = j.get(key)?.as_array()?;
            if a.len() != 4 {
                return None;
            }
            Some([0, 1, 2, 3].map(|i| a[i].as_u64().unwrap_or(0) as u32))
        };
        let pos = j.get("position").and_then(Json::as_array).map(|a| {
            let c = |i: usize| a.get(i).and_then(Json::as_f64).unwrap_or(0.0) as f32;
            [c(0), c(1), c(2)]
        });
        Some((
            id,
            SubjectState {
                path: j.get("path").and_then(Json::as_str).unwrap_or("").to_owned(),
                projected: rect("screen"),
                exact: rect("screen_box_exact"),
                visible_px: j.get("visible_px").and_then(Json::as_u64),
                position: pos.unwrap_or([0.0; 3]),
            },
        ))
    }

    fn seen(&self) -> bool {
        match self.visible_px {
            Some(px) => px > 0,
            None => self.projected.is_some(),
        }
    }
}

/// Every agent's marks.
#[derive(Default)]
pub struct Marks {
    by_owner: BTreeMap<String, BTreeMap<String, Mark>>,
    clock: u64,
}

impl Marks {
    /// Stores a mark (replacing one of the same name); returns the names evicted to make room.
    pub fn put(&mut self, owner: &str, name: &str, view: MarkView, data: MarkData) -> Vec<String> {
        self.clock += 1;
        let mark = Mark { view, data, used: self.clock };
        if !self.by_owner.contains_key(owner) && self.by_owner.len() >= MAX_MARK_OWNERS {
            // Forget the identity whose marks were used longest ago.
            let stale = self.by_owner.iter().map(|(o, ms)| (ms.values().map(|m| m.used).max().unwrap_or(0), o.clone())).min();
            if let Some((_, o)) = stale {
                self.by_owner.remove(&o);
            }
        }
        let mine = self.by_owner.entry(owner.to_owned()).or_default();
        mine.insert(name.to_owned(), mark);
        let mut evicted = Vec::new();
        while mine.len() > MAX_MARKS_PER_AGENT {
            let oldest = mine.iter().min_by_key(|(_, m)| m.used).map(|(k, _)| k.clone()).expect("non-empty");
            mine.remove(&oldest);
            evicted.push(oldest);
        }
        // The global budget: least recently used anywhere (never the one just stored).
        while self.bytes() > MAX_MARK_BYTES {
            let victim = self
                .by_owner
                .iter()
                .flat_map(|(o, ms)| ms.iter().map(move |(n, m)| (m.used, o.clone(), n.clone())))
                .filter(|(_, o, n)| !(o == owner && n == name))
                .min();
            let Some((_, o, n)) = victim else { break };
            if let Some(ms) = self.by_owner.get_mut(&o) {
                ms.remove(&n);
            }
            if o == owner {
                evicted.push(n);
            }
        }
        self.by_owner.retain(|_, ms| !ms.is_empty());
        evicted
    }

    pub fn get(&mut self, owner: &str, name: &str) -> Option<&Mark> {
        self.clock += 1;
        let clock = self.clock;
        let m = self.by_owner.get_mut(owner)?.get_mut(name)?;
        m.used = clock;
        Some(m)
    }

    pub fn names(&self, owner: &str) -> Vec<(String, u64, [u32; 2])> {
        self.by_owner
            .get(owner)
            .map(|ms| ms.iter().map(|(n, m)| (n.clone(), m.data.frame, [m.view.width, m.view.height])).collect())
            .unwrap_or_default()
    }

    pub fn remove(&mut self, owner: &str, name: Option<&str>) -> usize {
        let Some(ms) = self.by_owner.get_mut(owner) else { return 0 };
        let n = match name {
            Some(n) => usize::from(ms.remove(n).is_some()),
            None => std::mem::take(ms).len(),
        };
        self.by_owner.retain(|_, ms| !ms.is_empty());
        n
    }

    /// Memory every mark holds, estimated (what [`MAX_MARK_BYTES`] bounds).
    pub fn bytes(&self) -> usize {
        self.by_owner.values().flat_map(|ms| ms.values()).map(Mark::bytes).sum()
    }

    pub fn owners(&self) -> usize {
        self.by_owner.len()
    }
}

/// Pixel difference between two same-sized RGBA8 images.
#[derive(Clone, Debug, PartialEq)]
pub struct PixelDiff {
    pub changed: u64,
    pub total: u64,
    /// Mean over all pixels of the largest channel difference, 0..255.
    pub mean: f64,
    pub max: u8,
    /// `[x, y, w, h]` and changed pixels per region, largest first.
    pub regions: Vec<([u32; 4], u64)>,
    pub regions_total: usize,
    /// Per pixel: the largest channel difference.
    pub magnitude: Vec<u8>,
}

impl PixelDiff {
    pub fn to_json(&self) -> Json {
        let pct = if self.total == 0 { 0.0 } else { self.changed as f64 * 100.0 / self.total as f64 };
        Json::obj([
            ("changed_px", Json::from(self.changed)),
            ("changed_pct", Json::from((pct * 1000.0).round() / 1000.0)),
            ("mean_diff", Json::from((self.mean * 1000.0).round() / 1000.0)),
            ("max_diff", Json::from(u32::from(self.max))),
            (
                "regions",
                Json::arr(self.regions.iter().map(|(r, n)| {
                    Json::obj([("box", Json::arr(r.map(Json::from))), ("changed_px", Json::from(*n))])
                })),
            ),
            ("regions_total", Json::from(self.regions_total)),
        ])
    }
}

/// Compares `before` and `after` (RGBA8, `w` × `h`): a pixel changed when a colour channel
/// differs by more than `threshold`. Changed pixels are grouped into 8 px cells, and touching
/// cells (8-connected) into regions.
pub fn diff(before: &[u8], after: &[u8], w: u32, h: u32, threshold: u8) -> PixelDiff {
    let n = (w * h) as usize;
    assert!(before.len() >= n * 4 && after.len() >= n * 4, "images smaller than {w}x{h}");
    let mut magnitude = vec![0u8; n];
    let (mut changed, mut sum, mut max) = (0u64, 0u64, 0u8);
    let (cw, ch) = (w.div_ceil(CELL), h.div_ceil(CELL));
    let mut cells = vec![0u64; (cw * ch) as usize];
    for i in 0..n {
        let (b, a) = (&before[i * 4..i * 4 + 3], &after[i * 4..i * 4 + 3]);
        let m = (0..3).map(|c| b[c].abs_diff(a[c])).max().unwrap_or(0);
        magnitude[i] = m;
        sum += u64::from(m);
        max = max.max(m);
        if m > threshold {
            changed += 1;
            let (x, y) = (i as u32 % w, i as u32 / w);
            cells[((y / CELL) * cw + x / CELL) as usize] += 1;
        }
    }
    // Connected cells → regions (exact pixel boxes from the changed pixels inside them).
    let mut label = vec![u32::MAX; cells.len()];
    let mut regions: Vec<([u32; 4], u64)> = Vec::new();
    let mut stack = Vec::new();
    for start in 0..cells.len() {
        if cells[start] == 0 || label[start] != u32::MAX {
            continue;
        }
        let id = regions.len() as u32;
        let (mut x0, mut y0, mut x1, mut y1, mut count) = (u32::MAX, u32::MAX, 0, 0, 0u64);
        label[start] = id;
        stack.push(start);
        while let Some(c) = stack.pop() {
            let (cx, cy) = (c as u32 % cw, c as u32 / cw);
            count += cells[c];
            // The exact extent of changed pixels in this cell.
            for y in cy * CELL..((cy + 1) * CELL).min(h) {
                for x in cx * CELL..((cx + 1) * CELL).min(w) {
                    if magnitude[(y * w + x) as usize] > threshold {
                        x0 = x0.min(x);
                        y0 = y0.min(y);
                        x1 = x1.max(x);
                        y1 = y1.max(y);
                    }
                }
            }
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let (nx, ny) = (i64::from(cx) + dx, i64::from(cy) + dy);
                    if nx < 0 || ny < 0 || nx >= i64::from(cw) || ny >= i64::from(ch) {
                        continue;
                    }
                    let nc = (ny as u32 * cw + nx as u32) as usize;
                    if cells[nc] > 0 && label[nc] == u32::MAX {
                        label[nc] = id;
                        stack.push(nc);
                    }
                }
            }
        }
        regions.push(([x0, y0, x1 - x0 + 1, y1 - y0 + 1], count));
    }
    regions.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let regions_total = regions.len();
    regions.truncate(MAX_REGIONS);
    PixelDiff { changed, total: n as u64, mean: if n == 0 { 0.0 } else { sum as f64 / n as f64 }, max, regions, regions_total, magnitude }
}

/// The heat image: unchanged pixels are the "after" image in dim grey, changed ones a ramp from
/// red through yellow to white by how much they changed.
pub fn heat(after: &[u8], d: &PixelDiff, threshold: u8) -> Vec<u8> {
    let mut out = vec![0u8; d.magnitude.len() * 4];
    for (i, &m) in d.magnitude.iter().enumerate() {
        let px = &mut out[i * 4..i * 4 + 4];
        if m > threshold {
            let t = f32::from(m) / 255.0;
            // 0..1/3 red rises, ..2/3 green, ..1 blue: red → yellow → white.
            let ramp = |lo: f32| (((t - lo) * 3.0).clamp(0.0, 1.0) * 255.0) as u8;
            px.copy_from_slice(&[ramp(-0.25).max(140), ramp(0.2), ramp(0.6), 255]);
        } else {
            let a = &after[i * 4..i * 4 + 3];
            let grey = ((u32::from(a[0]) * 3 + u32::from(a[1]) * 6 + u32::from(a[2])) / 10 / 3) as u8;
            px.copy_from_slice(&[grey, grey, grey, 255]);
        }
    }
    out
}

/// Subject changes between a mark and now, through the same camera. `before` and `after` hold
/// the on-screen subjects only; `before_ids` is every entity that existed at the mark (sorted)
/// and `exists` says whether an entity exists now.
/// - added: on screen now, and created since the mark;
/// - removed: on screen at the mark, and deleted since;
/// - moved: on screen both times, with its projected box shifted by more than `move_px` or its
///   world position by more than 1 mm (projected boxes are compared, never an exact box with a
///   projected one: a change of visibility alone is not a move);
/// - became_visible / became_hidden: pixels showing (or, without the ID buffer, on screen) at
///   one capture and not the other, for entities that exist both times.
///
/// At most `max` items are listed per kind (all are counted).
pub fn subject_diff(
    before: &BTreeMap<u64, SubjectState>,
    before_ids: &[u64],
    after: &BTreeMap<u64, SubjectState>,
    exists: &dyn Fn(u64) -> bool,
    move_px: u32,
    max: usize,
) -> Json {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut moved = Vec::new();
    let mut appeared = Vec::new();
    let mut disappeared = Vec::new();
    let path = |s: &SubjectState| Json::from(s.path.as_str());
    let existed = |id: &u64| before_ids.binary_search(id).is_ok();
    for (id, a) in after {
        if !existed(id) {
            added.push(path(a));
            continue;
        }
        if let Some(b) = before.get(id) {
            let screen_moved = match (b.projected, a.projected) {
                (Some(r0), Some(r1)) => (0..4).any(|i| r0[i].abs_diff(r1[i]) > move_px),
                _ => false,
            };
            let d = (0..3).map(|i| (a.position[i] - b.position[i]).powi(2)).sum::<f32>().sqrt();
            if screen_moved || d > 1e-3 {
                moved.push(Json::obj([
                    ("path", path(a)),
                    ("from", b.projected.map_or(Json::Null, |r| Json::arr(r.map(Json::from)))),
                    ("to", a.projected.map_or(Json::Null, |r| Json::arr(r.map(Json::from)))),
                    ("world_moved_m", Json::from((d * 1000.0).round() / 1000.0)),
                ]));
            }
        }
        if a.seen() && !before.get(id).is_some_and(SubjectState::seen) {
            appeared.push(path(a));
        }
    }
    for (id, b) in before {
        if !exists(*id) {
            removed.push(path(b));
        } else if b.seen() && !after.get(id).is_some_and(SubjectState::seen) {
            disappeared.push(path(b));
        }
    }
    let list = |mut v: Vec<Json>| {
        let total = v.len();
        v.truncate(max);
        Json::obj([("count", Json::from(total)), ("items", Json::Arr(v))])
    };
    Json::obj([
        ("added", list(added)),
        ("removed", list(removed)),
        ("moved", list(moved)),
        ("became_visible", list(appeared)),
        ("became_hidden", list(disappeared)),
    ])
}

/// `images` side by side (all `w` × `h`, RGBA8) with a `gap` px dark gutter, scaled down by a
/// whole factor (box filter) so the sheet is at most `max_w` wide. Returns `(rgba, w, h, factor)`.
pub fn side_by_side(images: &[&[u8]], w: u32, h: u32, gap: u32, max_w: u32) -> (Vec<u8>, u32, u32, u32) {
    let n = images.len() as u32;
    let mut factor = 1;
    while (w / factor) * n + gap * (n - 1) > max_w && factor < 16 {
        factor += 1;
    }
    let (tw, th) = (w / factor, h / factor);
    let (sw, sh) = (tw * n + gap * (n - 1), th);
    let mut out = vec![0u8; (sw * sh * 4) as usize];
    for px in out.chunks_exact_mut(4) {
        px.copy_from_slice(&[24, 26, 30, 255]);
    }
    for (k, img) in images.iter().enumerate() {
        let small = downscale(img, w, h, factor);
        blit(&mut out, sw, &small, tw, th, k as u32 * (tw + gap), 0);
    }
    (out, sw, sh, factor)
}

/// Box-filters an RGBA8 image down by a whole `factor`.
pub fn downscale(img: &[u8], w: u32, h: u32, factor: u32) -> Vec<u8> {
    if factor <= 1 {
        return img[..(w * h * 4) as usize].to_vec();
    }
    let (tw, th) = (w / factor, h / factor);
    let mut out = vec![0u8; (tw * th * 4) as usize];
    let area = factor * factor;
    for y in 0..th {
        for x in 0..tw {
            let mut acc = [0u32; 4];
            for sy in 0..factor {
                let row = ((y * factor + sy) * w + x * factor) as usize * 4;
                for sx in 0..factor as usize {
                    for c in 0..4 {
                        acc[c] += u32::from(img[row + sx * 4 + c]);
                    }
                }
            }
            let o = ((y * tw + x) * 4) as usize;
            for c in 0..4 {
                out[o + c] = ((acc[c] + area / 2) / area) as u8;
            }
        }
    }
    out
}

/// Copies a `w` × `h` RGBA8 image into `dst` (`dst_w` wide) at (`x`, `y`).
pub fn blit(dst: &mut [u8], dst_w: u32, src: &[u8], w: u32, h: u32, x: u32, y: u32) {
    for row in 0..h {
        let d = (((y + row) * dst_w + x) * 4) as usize;
        let s = (row * w * 4) as usize;
        dst[d..d + (w * 4) as usize].copy_from_slice(&src[s..s + (w * 4) as usize]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        (0..w * h).flat_map(|i| f(i % w, i / w)).collect()
    }

    #[test]
    fn diff_counts_pixels_and_boxes_regions() {
        let (w, h) = (64, 48);
        let before = image(w, h, |_, _| [10, 20, 30, 255]);
        // Two separate changed patches: 10x5 at (4, 4), and 3x3 at (50, 40); plus a pixel
        // that changes by less than the threshold.
        let after = image(w, h, |x, y| {
            if (4..14).contains(&x) && (4..9).contains(&y) {
                [200, 20, 30, 255]
            } else if (50..53).contains(&x) && (40..43).contains(&y) {
                [10, 20, 90, 255]
            } else if (x, y) == (30, 30) {
                [15, 20, 30, 255]
            } else {
                [10, 20, 30, 255]
            }
        });
        let d = diff(&before, &after, w, h, 8);
        assert_eq!(d.changed, 50 + 9);
        assert_eq!(d.max, 190);
        let expected_mean = (50.0 * 190.0 + 9.0 * 60.0 + 5.0) / (64.0 * 48.0);
        assert!((d.mean - expected_mean).abs() < 1e-9, "{} vs {expected_mean}", d.mean);
        assert_eq!(d.regions, vec![([4, 4, 10, 5], 50), ([50, 40, 3, 3], 9)]);
        let j = d.to_json();
        assert_eq!(j.get("changed_pct").and_then(Json::as_f64), Some(1.921));
        // Identical images: nothing changed.
        let same = diff(&before, &before, w, h, 0);
        assert_eq!((same.changed, same.max, same.regions.len()), (0, 0, 0));
        // The heat image marks exactly the changed pixels in colour.
        let hm = heat(&after, &d, 8);
        let coloured = hm.chunks_exact(4).filter(|p| p[0] != p[1] || p[1] != p[2]).count();
        assert_eq!(coloured, 59);
    }

    #[test]
    fn touching_cells_join_one_region() {
        let (w, h) = (40, 40);
        let before = image(w, h, |_, _| [0, 0, 0, 255]);
        // A diagonal stroke crossing several cells.
        let after = image(w, h, |x, y| if x == y { [255, 255, 255, 255] } else { [0, 0, 0, 255] });
        let d = diff(&before, &after, w, h, 8);
        assert_eq!(d.regions, vec![([0, 0, 40, 40], 40)]);
    }


    fn st(path: &str, projected: Option<[u32; 4]>, exact: Option<[u32; 4]>, px: Option<u64>, x: f32) -> SubjectState {
        SubjectState { path: path.into(), projected, exact, visible_px: px, position: [x, 0.0, 0.0] }
    }

    #[test]
    fn subject_changes() {
        let r = |x: u32, y: u32| Some([x, y, 20, 20]);
        let before = BTreeMap::from([
            (1, st("/stay", r(10, 10), r(10, 10), Some(400), 0.0)),
            (2, st("/move", r(50, 10), r(50, 10), Some(400), 3.0)),
            (3, st("/gone", r(90, 10), r(90, 10), Some(400), 6.0)),
            (4, st("/hide", r(90, 50), r(90, 50), Some(300), 9.0)),
            // A wall comes in front of it: its exact box shrinks to nothing, its projected box
            // stays: hidden, not moved.
            (6, st("/walled", r(130, 50), r(130, 50), Some(400), 12.0)),
        ]);
        let after = BTreeMap::from([
            (1, st("/stay", r(10, 11), r(10, 11), Some(400), 0.0)),
            (2, st("/move", r(60, 10), r(60, 10), Some(400), 3.5)),
            (4, st("/hide", r(90, 50), None, Some(0), 9.0)),
            (5, st("/new", Some([0, 0, 5, 5]), Some([0, 0, 5, 5]), Some(25), 1.0)),
            (6, st("/walled", r(130, 50), Some([130, 50, 4, 20]), Some(80), 12.0)),
            // It existed at the mark (off screen) and came into view: not new.
            (7, st("/came_in", r(0, 100), r(0, 100), Some(400), 0.0)),
        ]);
        let ids = [1, 2, 3, 4, 6, 7, 8];
        // 3 was deleted; 8 still exists but is off screen in both.
        let exists = |id: u64| id != 3;
        let j = subject_diff(&before, &ids, &after, &exists, 2, MAX_SUBJECT_CHANGES);
        let items = |k: &str| -> Vec<String> {
            j.get(k).unwrap().get("items").unwrap().as_array().unwrap().iter().map(|v| v.get("path").unwrap_or(v).as_str().unwrap().to_owned()).collect()
        };
        assert_eq!(items("added"), ["/new"]);
        assert_eq!(items("removed"), ["/gone"]);
        assert_eq!(items("moved"), ["/move"], "visibility alone (/walled, /hide) is no move");
        assert_eq!(items("became_visible"), ["/came_in"]);
        assert_eq!(items("became_hidden"), ["/hide"]);
        let moved = &j.get("moved").unwrap().get("items").unwrap().as_array().unwrap()[0];
        assert_eq!(moved.get("world_moved_m").and_then(Json::as_f64), Some(0.5));
        // Leaving the view is becoming hidden, not being removed.
        let gone_from_view = subject_diff(&before, &ids, &BTreeMap::new(), &|_| true, 2, 10);
        assert_eq!(gone_from_view.get("removed").unwrap().get("count").and_then(Json::as_u64), Some(0));
        assert_eq!(gone_from_view.get("became_hidden").unwrap().get("count").and_then(Json::as_u64), Some(5));
    }

    #[test]
    fn subject_changes_stay_bounded() {
        let s = |i: u64, x: f32| st(&format!("/district/house_{i:05}"), Some([0, 0, 4, 4]), Some([0, 0, 4, 4]), Some(16), x);
        let before: BTreeMap<u64, SubjectState> = (0..2000).map(|i| (i, s(i, 0.0))).collect();
        let before_ids: Vec<u64> = (0..2000).collect();
        let after: BTreeMap<u64, SubjectState> = (1000..3000).map(|i| (i, s(i, 1.0))).collect();
        let j = subject_diff(&before, &before_ids, &after, &|id| id >= 1000, 2, 10);
        for kind in ["added", "removed", "moved"] {
            let k = j.get(kind).unwrap();
            assert_eq!(k.get("count").and_then(Json::as_u64), Some(1000), "{kind}");
            assert_eq!(k.get("items").and_then(Json::as_array).map(|a| a.len()), Some(10), "{kind}");
        }
        assert!(j.to_string().len() < 4096, "{} bytes", j.to_string().len());
        // Changed regions are capped too.
        let (w, h) = (256u32, 256u32);
        let before = vec![0u8; (w * h * 4) as usize];
        let after: Vec<u8> = (0..w * h).flat_map(|i| if (i % w) % 16 == 0 && (i / w) % 16 == 0 { [255, 255, 255, 255] } else { [0, 0, 0, 255] }).collect();
        let d = diff(&before, &after, w, h, 8);
        assert_eq!((d.regions.len(), d.regions_total), (MAX_REGIONS, 256));
    }

    fn view() -> MarkView {
        MarkView {
            camera: Camera { eye: specula_core::math::Vec3::ONE, target: specula_core::math::Vec3::ZERO, fov_y_deg: 50.0 },
            width: 4,
            height: 4,
            include_ui: false,
            lod: None,
            time_s: None,
            overlays: Default::default(),
            ids: true,
        }
    }

    fn data(pixels: usize, subjects: BTreeMap<u64, SubjectState>, ids: Vec<u64>) -> MarkData {
        MarkData { rgba: vec![0; pixels * 4], subjects, ids, frame: 1, tick: 0, png_path: None }
    }

    #[test]
    fn marks_are_bounded_per_agent() {
        let mut m = Marks::default();
        for i in 0..MAX_MARKS_PER_AGENT {
            assert!(m.put("ann", &format!("m{i}"), view(), data(16, BTreeMap::new(), Vec::new())).is_empty());
        }
        // Using m0 makes m1 the least recently used.
        assert!(m.get("ann", "m0").is_some());
        let evicted = m.put("ann", "extra", view(), data(16, BTreeMap::new(), Vec::new()));
        assert_eq!(evicted, ["m1"]);
        assert_eq!(m.names("ann").len(), MAX_MARKS_PER_AGENT);
        assert!(m.get("bob", "m0").is_none(), "marks are per agent");
        assert_eq!(m.remove("ann", Some("m0")), 1);
        assert_eq!(m.remove("ann", None), MAX_MARKS_PER_AGENT - 1);
    }

    /// Reviewer finding: the budget counted pixels only, while each mark kept the whole world's
    /// subject list, and owners were unbounded (20k entities x 48 marks grew the process 184 MB).
    #[test]
    fn marks_count_subjects_and_ids_and_cap_owners() {
        let mut m = Marks::default();
        // 20k entities (ids only) and a hundred on-screen subjects per mark, 64x64 pixels.
        let ids: Vec<u64> = (1..=20_000).collect();
        let subjects: BTreeMap<u64, SubjectState> =
            (1..=100).map(|i| (i, st(&format!("/district_{i}/building_with_a_long_name_{i:05}"), None, None, None, 0.0))).collect();
        let one = Mark { view: view(), data: data(64 * 64, subjects.clone(), ids.clone()), used: 0 }.bytes();
        assert!(one >= 64 * 64 * 4 + 20_000 * 8 + 100 * (40 + SUBJECT_BYTES), "{one}");
        for owner in 0..(MAX_MARK_OWNERS + 10) {
            for k in 0..MAX_MARKS_PER_AGENT {
                m.put(&format!("agent_{owner}"), &format!("m{k}"), view(), data(64 * 64, subjects.clone(), ids.clone()));
            }
        }
        assert_eq!(m.owners(), MAX_MARK_OWNERS, "the oldest identities are forgotten");
        assert_eq!(m.bytes(), MAX_MARK_OWNERS * MAX_MARKS_PER_AGENT * one);
        assert!(m.get("agent_0", "m0").is_none() && m.get(&format!("agent_{}", MAX_MARK_OWNERS + 9), "m0").is_some());
        // The byte budget evicts by everything a mark holds, not its pixels alone.
        let mut big = Marks::default();
        let huge_ids: Vec<u64> = (0..(MAX_MARK_BYTES / 8 / 3) as u64).collect();
        big.put("a", "x", view(), data(16, BTreeMap::new(), huge_ids.clone()));
        big.put("a", "y", view(), data(16, BTreeMap::new(), huge_ids.clone()));
        let evicted = big.put("a", "z", view(), data(16, BTreeMap::new(), huge_ids));
        assert_eq!(evicted, ["x"]);
        assert!(big.bytes() <= MAX_MARK_BYTES);
    }
    #[test]
    fn sheets_fit_their_width() {
        let a = image(100, 50, |_, _| [255, 0, 0, 255]);
        let b = image(100, 50, |_, _| [0, 0, 255, 255]);
        let (sheet, w, h, f) = side_by_side(&[&a, &b], 100, 50, 4, 150);
        assert_eq!((w, h, f), (2 * 50 + 4, 25, 2));
        assert_eq!(sheet.len(), (w * h * 4) as usize);
        assert_eq!(&sheet[0..4], &[255, 0, 0, 255]);
        assert_eq!(&sheet[(54 * 4) as usize..(54 * 4 + 4) as usize], &[0, 0, 255, 255]);
        assert_eq!(&sheet[(51 * 4) as usize..(51 * 4 + 4) as usize], &[24, 26, 30, 255], "gutter");
        let checker = image(4, 4, |x, y| if (x + y) % 2 == 0 { [0, 0, 0, 255] } else { [254, 254, 254, 255] });
        assert_eq!(downscale(&checker, 4, 4, 2), [127, 127, 127, 255].repeat(4));
    }
}
