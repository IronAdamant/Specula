//! The layout pass: flexbox-like rows / columns / grids, anchored stacks, and the radial ring.
//!
//! Five passes over the pre-order node array, no allocation:
//! 1. fit widths, bottom-up (content-based intrinsic sizes);
//! 2. resolve widths, top-down (fixed, percent, fill shares, stretch);
//! 3. text heights (wrapping at the resolved widths) and fit heights, bottom-up;
//! 4. resolve heights, top-down;
//! 5. positions, top-down (anchors, justify / align, grid cells, list scroll, radial ring).
//!
//! All sizes here are physical px: logical px × `scale`, em = the theme's body size × `scale`.

use crate::draw::Rect;
use crate::screen::{Align, Justify, Kind, Layout, Len, Node, Screen, Size};

/// Per-node layout results (kept between frames: scroll offsets live here).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Computed {
    pub visible: bool,
    /// Border box.
    pub rect: Rect,
    /// Inside the padding.
    pub content: Rect,
    /// Intrinsic (fit) size.
    pub fit: [f32; 2],
    /// Clip rect inherited from scrolling / clipping ancestors (`None`: unclipped).
    pub clip: Option<Rect>,
    /// Measured text box for labels.
    pub text: [f32; 2],
    /// Lists: scroll offset (px, ≥ 0) and the height of all children.
    pub scroll: f32,
    pub extent: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Ctx {
    /// Physical px per logical px.
    pub scale: f32,
    /// 1em in logical px.
    pub em: f32,
}

impl Ctx {
    fn len(&self, l: Len, parent: f32) -> f32 {
        l.resolve(parent, self.scale, self.em)
    }

    fn pad(&self, n: &Node, parent_w: f32) -> [f32; 4] {
        n.padding.map(|p| self.len(p, parent_w))
    }

    fn margin(&self, n: &Node, parent_w: f32) -> [f32; 4] {
        n.margin.map(|p| self.len(p, parent_w))
    }

    fn clamp(&self, n: &Node, axis: usize, v: f32, parent: f32) -> f32 {
        let mut v = v;
        if let Some(mx) = n.max[axis] {
            v = v.min(self.len(mx, parent));
        }
        if let Some(mn) = n.min[axis] {
            v = v.max(self.len(mn, parent));
        }
        v.max(0.0)
    }
}

/// Text measuring callback: `(node, wrap width)` → `[width, height]` in physical px.
pub trait Measure {
    fn text(&mut self, node: u32, wrap: Option<f32>) -> [f32; 2];
}

fn size_of(n: &Node, axis: usize) -> Size {
    if axis == 0 {
        n.width
    } else {
        n.height
    }
}

/// The size a child asks for along `axis` when its parent fits its content.
fn ask(ctx: &Ctx, n: &Node, c: &Computed, axis: usize) -> f32 {
    let v = match size_of(n, axis) {
        Size::Fixed(Len::Percent(_)) => c.fit[axis],
        Size::Fixed(l) => ctx.len(l, 0.0),
        Size::Auto | Size::Fill(_) => c.fit[axis],
    };
    ctx.clamp(n, axis, v, 0.0)
}

fn main_axis(l: Layout) -> Option<usize> {
    match l {
        Layout::Row => Some(0),
        Layout::Column => Some(1),
        _ => None,
    }
}

/// Lay out `screen` into `root` (the safe area, or the full target for full-bleed screens).
/// `comp[i].visible` must be set by the caller before.
pub fn layout(screen: &Screen, comp: &mut [Computed], root: Rect, ctx: &Ctx, m: &mut dyn Measure) {
    let nodes = &screen.nodes;
    let n = nodes.len();
    if n == 0 {
        return;
    }
    // ---- 1. fit widths, bottom-up ----
    for i in (0..n).rev() {
        fit(screen, comp, ctx, m, i, 0);
    }
    // ---- 2. resolve widths, top-down ----
    comp[0].rect.w = match nodes[0].width {
        Size::Fixed(l) => ctx.len(l, root.w),
        Size::Auto | Size::Fill(_) => root.w,
    };
    comp[0].rect.w = ctx.clamp(&nodes[0], 0, comp[0].rect.w, root.w);
    for i in 0..n {
        if comp[i].visible {
            resolve(screen, comp, ctx, i, 0);
        }
    }
    // ---- 3. text heights + fit heights, bottom-up ----
    for i in (0..n).rev() {
        if comp[i].visible {
            if let Kind::Label { wrap, .. } = nodes[i].kind {
                let pad = ctx.pad(&nodes[i], 0.0);
                let inner = (comp[i].rect.w - pad[1] - pad[3]).max(0.0);
                comp[i].text = m.text(i as u32, if wrap { Some(inner) } else { None });
            }
        }
        fit(screen, comp, ctx, m, i, 1);
    }
    // ---- 4. resolve heights, top-down ----
    comp[0].rect.h = match nodes[0].height {
        Size::Fixed(l) => ctx.len(l, root.h),
        Size::Auto | Size::Fill(_) => root.h,
    };
    comp[0].rect.h = ctx.clamp(&nodes[0], 1, comp[0].rect.h, root.h);
    for i in 0..n {
        if comp[i].visible {
            resolve(screen, comp, ctx, i, 1);
        }
    }
    // ---- 5. positions ----
    comp[0].rect.x = root.x;
    comp[0].rect.y = root.y;
    comp[0].clip = None;
    for i in 0..n {
        if comp[i].visible {
            place(screen, comp, ctx, i);
        }
    }
}

/// Intrinsic size of node `i` along `axis` (children already done).
fn fit(screen: &Screen, comp: &mut [Computed], ctx: &Ctx, m: &mut dyn Measure, i: usize, axis: usize) {
    let node = &screen.nodes[i];
    if !comp[i].visible {
        comp[i].fit[axis] = 0.0;
        return;
    }
    let pad = ctx.pad(node, 0.0);
    let pad_axis = if axis == 0 { pad[1] + pad[3] } else { pad[0] + pad[2] };
    let content = match node.kind {
        Kind::Label { .. } => {
            if axis == 0 {
                let t = m.text(i as u32, None);
                comp[i].text = t;
                t[0]
            } else {
                comp[i].text[1]
            }
        }
        _ => {
            let gap = ctx.len(node.gap, 0.0);
            let mut sum = 0.0f32;
            let mut max = 0.0f32;
            let mut count = 0usize;
            let mut cells: [f32; 2] = [0.0, 0.0];
            let columns = match node.layout {
                Layout::Grid { columns } => columns as usize,
                _ => 1,
            };
            // Grid rows need per-row maxima for heights: accumulate as we go.
            let mut row_max = 0.0f32;
            let mut rows_sum = 0.0f32;
            for c in screen.children(i as u32) {
                let c = c as usize;
                if !comp[c].visible {
                    continue;
                }
                let mg = ctx.margin(&screen.nodes[c], 0.0);
                let extra = if axis == 0 { mg[1] + mg[3] } else { mg[0] + mg[2] };
                let a = ask(ctx, &screen.nodes[c], &comp[c], axis) + extra;
                sum += a;
                max = max.max(a);
                cells[axis] = cells[axis].max(a);
                if let Layout::Grid { .. } = node.layout {
                    row_max = row_max.max(a);
                    if (count + 1) % columns == 0 {
                        rows_sum += row_max;
                        row_max = 0.0;
                    }
                }
                count += 1;
            }
            let gaps = gap * count.saturating_sub(1) as f32;
            match node.layout {
                Layout::Row if axis == 0 => sum + gaps,
                Layout::Column if axis == 1 => sum + gaps,
                Layout::Grid { .. } => {
                    let rows = count.div_ceil(columns);
                    if axis == 0 {
                        let cols = count.min(columns);
                        max * cols as f32 + gap * cols.saturating_sub(1) as f32
                    } else {
                        rows_sum + row_max + gap * rows.saturating_sub(1) as f32
                    }
                }
                Layout::Radial => 0.0,
                _ => max,
            }
        }
    };
    comp[i].fit[axis] = content + pad_axis;
}

/// Resolve the sizes of `i`'s children along `axis` (node `i`'s own size is known).
fn resolve(screen: &Screen, comp: &mut [Computed], ctx: &Ctx, i: usize, axis: usize) {
    let node = &screen.nodes[i];
    let pad = ctx.pad(node, comp[i].rect.w);
    let inner = if axis == 0 {
        (comp[i].rect.w - pad[1] - pad[3]).max(0.0)
    } else {
        (comp[i].rect.h - pad[0] - pad[2]).max(0.0)
    };
    let gap = ctx.len(node.gap, inner);
    let along_main = main_axis(node.layout) == Some(axis);
    if along_main {
        // Fixed / auto first, then share the rest among fills.
        let mut used = 0.0f32;
        let mut weights = 0.0f32;
        let mut count = 0usize;
        for c in screen.children(i as u32) {
            let c = c as usize;
            if !comp[c].visible {
                continue;
            }
            count += 1;
            let cn = &screen.nodes[c];
            let mg = ctx.margin(cn, inner);
            used += if axis == 0 { mg[1] + mg[3] } else { mg[0] + mg[2] };
            match size_of(cn, axis) {
                Size::Fill(w) => weights += w,
                Size::Fixed(l) => {
                    let v = ctx.clamp(cn, axis, ctx.len(l, inner), inner);
                    set_size(&mut comp[c], axis, v);
                    used += v;
                }
                Size::Auto => {
                    let v = ctx.clamp(cn, axis, comp[c].fit[axis], inner);
                    set_size(&mut comp[c], axis, v);
                    used += v;
                }
            }
        }
        used += gap * count.saturating_sub(1) as f32;
        let free = (inner - used).max(0.0);
        for c in screen.children(i as u32) {
            let c = c as usize;
            if !comp[c].visible {
                continue;
            }
            let cn = &screen.nodes[c];
            if let Size::Fill(w) = size_of(cn, axis) {
                let share = if weights > 0.0 { free * w / weights } else { 0.0 };
                let v = ctx.clamp(cn, axis, share.max(comp[c].fit[axis].min(share.max(0.0))), inner);
                set_size(&mut comp[c], axis, v);
            }
        }
        return;
    }
    // Cross axis (or stack / grid / radial): each child on its own.
    let cell = match node.layout {
        Layout::Grid { columns } if axis == 0 => {
            let cols = columns as f32;
            ((inner - gap * (cols - 1.0)) / cols).max(0.0)
        }
        Layout::Grid { columns } => {
            // Row height: the tallest child in that row (computed per row below).
            let _ = columns;
            -1.0
        }
        _ => inner,
    };
    let mut grid_rows: [f32; 0] = [];
    let _ = &mut grid_rows;
    let mut k = 0usize;
    let mut row_h = 0.0f32;
    let columns = match node.layout {
        Layout::Grid { columns } => columns as usize,
        _ => 1,
    };
    let children_start = i + 1;
    let _ = children_start;
    for c in screen.children(i as u32) {
        let c = c as usize;
        if !comp[c].visible {
            continue;
        }
        let cn = &screen.nodes[c];
        let mg = ctx.margin(cn, inner);
        let extra = if axis == 0 { mg[1] + mg[3] } else { mg[0] + mg[2] };
        let space = if cell >= 0.0 {
            cell
        } else {
            // Grid height: the row's tallest fit.
            if k % columns == 0 {
                row_h = 0.0;
                let mut j = 0;
                for c2 in screen.children(i as u32).filter(|&c2| comp[c2 as usize].visible).skip(k) {
                    if j == columns {
                        break;
                    }
                    let c2n = &screen.nodes[c2 as usize];
                    let mg2 = ctx.margin(c2n, inner);
                    row_h = row_h.max(ask(ctx, c2n, &comp[c2 as usize], 1) + mg2[0] + mg2[2]);
                    j += 1;
                }
            }
            row_h
        };
        let stretch = node.align == Align::Stretch && !matches!(node.layout, Layout::Stack | Layout::Radial);
        let v = match size_of(cn, axis) {
            Size::Fill(_) => space - extra,
            Size::Fixed(l) => ctx.len(l, space),
            Size::Auto if stretch || matches!(node.layout, Layout::Grid { .. }) && axis == 0 && cn.kind.interactive() => space - extra,
            Size::Auto => comp[c].fit[axis].min(if matches!(node.layout, Layout::Grid { .. }) { space - extra } else { f32::MAX }),
        };
        let v = ctx.clamp(cn, axis, v, space);
        set_size(&mut comp[c], axis, v);
        k += 1;
    }
}

fn set_size(c: &mut Computed, axis: usize, v: f32) {
    if axis == 0 {
        c.rect.w = v;
    } else {
        c.rect.h = v;
    }
}

/// Position `i`'s children (node `i`'s rect is final).
fn place(screen: &Screen, comp: &mut [Computed], ctx: &Ctx, i: usize) {
    let node = &screen.nodes[i];
    let r = comp[i].rect;
    let pad = ctx.pad(node, r.w);
    let content = r.inset(pad[0], pad[1], pad[2], pad[3]);
    comp[i].content = content;
    let child_clip = if node.clip {
        Some(match comp[i].clip {
            Some(c) => c.intersect(&content),
            None => content,
        })
    } else {
        comp[i].clip
    };
    let gap_x = ctx.len(node.gap, content.w);
    let gap_y = ctx.len(node.gap, content.h);
    match node.layout {
        Layout::Stack => {
            for c in screen.children(i as u32) {
                let c = c as usize;
                if !comp[c].visible {
                    continue;
                }
                let cn = &screen.nodes[c];
                let mg = ctx.margin(cn, content.w);
                let off = [ctx.len(cn.offset[0], content.w), ctx.len(cn.offset[1], content.h)];
                let a = cn.anchor.0;
                let (w, h) = (comp[c].rect.w, comp[c].rect.h);
                comp[c].rect.x = content.x + mg[3] + a[0] * (content.w - w - mg[1] - mg[3]) + off[0];
                comp[c].rect.y = content.y + mg[0] + a[1] * (content.h - h - mg[0] - mg[2]) + off[1];
                comp[c].clip = child_clip;
            }
        }
        Layout::Row | Layout::Column => {
            let axis = if node.layout == Layout::Row { 0 } else { 1 };
            let cross = 1 - axis;
            let (main_len, cross_len) = if axis == 0 { (content.w, content.h) } else { (content.h, content.w) };
            let gap = if axis == 0 { gap_x } else { gap_y };
            let mut total = 0.0f32;
            let mut count = 0usize;
            for c in screen.children(i as u32) {
                let c = c as usize;
                if !comp[c].visible {
                    continue;
                }
                let mg = ctx.margin(&screen.nodes[c], content.w);
                total += if axis == 0 { comp[c].rect.w + mg[1] + mg[3] } else { comp[c].rect.h + mg[0] + mg[2] };
                count += 1;
            }
            total += gap * count.saturating_sub(1) as f32;
            if let Kind::List { .. } = node.kind {
                comp[i].extent = total;
                let max_scroll = (total - main_len).max(0.0);
                comp[i].scroll = comp[i].scroll.clamp(0.0, max_scroll);
            }
            let free = main_len - total;
            let (mut pos, between) = match node.justify {
                Justify::Start => (0.0, gap),
                Justify::Center => (free * 0.5, gap),
                Justify::End => (free, gap),
                Justify::SpaceBetween if count > 1 => (0.0, gap + free.max(0.0) / (count - 1) as f32),
                Justify::SpaceBetween => (0.0, gap),
            };
            if let Kind::List { .. } = node.kind {
                pos = -comp[i].scroll;
            }
            for c in screen.children(i as u32) {
                let c = c as usize;
                if !comp[c].visible {
                    continue;
                }
                let cn = &screen.nodes[c];
                let mg = ctx.margin(cn, content.w);
                let (m0, m1, c0, c1) = if axis == 0 { (mg[3], mg[1], mg[0], mg[2]) } else { (mg[0], mg[2], mg[3], mg[1]) };
                let size = [comp[c].rect.w, comp[c].rect.h];
                let cross_free = cross_len - size[cross] - c0 - c1;
                let cross_pos = c0 + match node.align {
                    Align::Start | Align::Stretch => 0.0,
                    Align::Center => cross_free * 0.5,
                    Align::End => cross_free,
                };
                pos += m0;
                if axis == 0 {
                    comp[c].rect.x = content.x + pos;
                    comp[c].rect.y = content.y + cross_pos;
                } else {
                    comp[c].rect.y = content.y + pos;
                    comp[c].rect.x = content.x + cross_pos;
                }
                pos += size[axis] + m1 + between;
                comp[c].clip = child_clip;
            }
        }
        Layout::Grid { columns } => {
            let columns = columns as usize;
            let cell_w = ((content.w - gap_x * (columns as f32 - 1.0)) / columns as f32).max(0.0);
            let mut k = 0usize;
            let mut y = content.y;
            let mut row_h = 0.0f32;
            for c in screen.children(i as u32) {
                let c = c as usize;
                if !comp[c].visible {
                    continue;
                }
                let col = k % columns;
                if col == 0 && k > 0 {
                    y += row_h + gap_y;
                    row_h = 0.0;
                }
                let cn = &screen.nodes[c];
                let mg = ctx.margin(cn, content.w);
                let x0 = content.x + col as f32 * (cell_w + gap_x);
                let free = cell_w - comp[c].rect.w - mg[1] - mg[3];
                let ax = match node.align {
                    Align::Center => free * 0.5,
                    Align::End => free,
                    _ => 0.0,
                };
                comp[c].rect.x = x0 + mg[3] + ax;
                comp[c].rect.y = y + mg[0];
                row_h = row_h.max(comp[c].rect.h + mg[0] + mg[2]);
                comp[c].clip = child_clip;
                k += 1;
            }
        }
        Layout::Radial => {
            let (radius, inner) = match node.kind {
                Kind::Radial { radius, inner } => (radius * ctx.scale, inner * ctx.scale),
                _ => (r.w * 0.5, 0.0),
            };
            let centre = r.center();
            let items: usize = screen
                .children(i as u32)
                .filter(|&c| comp[c as usize].visible && matches!(screen.nodes[c as usize].kind, Kind::RadialItem { .. }))
                .count();
            let mut k = 0usize;
            for c in screen.children(i as u32) {
                let c = c as usize;
                if !comp[c].visible {
                    continue;
                }
                let (w, h) = (comp[c].rect.w, comp[c].rect.h);
                let p = if matches!(screen.nodes[c].kind, Kind::RadialItem { .. }) {
                    let a = radial_angle(k, items);
                    k += 1;
                    let rm = (radius + inner) * 0.5;
                    [centre[0] + rm * a.cos(), centre[1] + rm * a.sin()]
                } else {
                    centre
                };
                comp[c].rect.x = p[0] - w * 0.5;
                comp[c].rect.y = p[1] - h * 0.5;
                comp[c].clip = child_clip;
            }
        }
    }
    // Children's content boxes for leaves (no further place() call needs them otherwise).
    let _ = Rect::default();
}

/// Angle (radians, y down: −π/2 is up) of item `k` of `n` in a radial menu: the first item at the
/// top, then clockwise.
pub fn radial_angle(k: usize, n: usize) -> f32 {
    -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::TAU / n.max(1) as f32
}
