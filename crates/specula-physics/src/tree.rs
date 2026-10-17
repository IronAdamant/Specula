//! The broadphase: a dynamic bounding-volume tree of fattened boxes (the approach of Box2D),
//! incrementally updated and kept balanced with AVL-style rotations. The same tree answers ray
//! and box queries for agents (P1), so the broadphase P3 needs is the one P1 built.
//!
//! The tree's shape depends on the order of inserts and removals, so callers never let its
//! traversal order reach the simulation: every query result is sorted by stable keys before
//! use (DETERMINISM.md rule 3).

use crate::math::{inv_dir, Aabb, Vec3};

const NULL: u32 = u32::MAX;

/// A traversal stack on the stack (balanced trees stay shallow), spilling to the heap only if
/// it ever runs out.
pub(crate) struct Stack {
    fixed: [u32; 128],
    len: usize,
    spill: Vec<u32>,
}

impl Stack {
    pub(crate) fn new() -> Stack {
        Stack { fixed: [0; 128], len: 0, spill: Vec::new() }
    }

    #[inline]
    pub(crate) fn push(&mut self, v: u32) {
        if self.len < self.fixed.len() {
            self.fixed[self.len] = v;
            self.len += 1;
        } else {
            self.spill.push(v);
        }
    }

    #[inline]
    pub(crate) fn pop(&mut self) -> Option<u32> {
        if let Some(v) = self.spill.pop() {
            return Some(v);
        }
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        Some(self.fixed[self.len])
    }
}

#[derive(Clone, Copy, Debug)]
struct Node {
    aabb: Aabb,
    /// Parent, or the next free node while free.
    parent: u32,
    child1: u32,
    child2: u32,
    /// Leaf = 0; free = -1.
    height: i32,
    /// The caller's id for a leaf.
    user: u32,
}

impl Node {
    fn is_leaf(&self) -> bool {
        self.child1 == NULL
    }
}

/// A dynamic AABB tree. Leaves carry a `u32` user id.
#[derive(Clone, Debug)]
pub struct Tree {
    nodes: Vec<Node>,
    root: u32,
    free: u32,
    leaves: usize,
}

impl Default for Tree {
    fn default() -> Tree {
        Tree { nodes: Vec::new(), root: NULL, free: NULL, leaves: 0 }
    }
}

impl Tree {
    pub fn len(&self) -> usize {
        self.leaves
    }

    pub fn is_empty(&self) -> bool {
        self.leaves == 0
    }

    pub fn clear(&mut self) {
        *self = Tree::default();
    }

    fn alloc(&mut self) -> u32 {
        if self.free == NULL {
            self.nodes.push(Node { aabb: Aabb::EMPTY, parent: NULL, child1: NULL, child2: NULL, height: 0, user: 0 });
            return (self.nodes.len() - 1) as u32;
        }
        let i = self.free;
        self.free = self.nodes[i as usize].parent;
        self.nodes[i as usize] = Node { aabb: Aabb::EMPTY, parent: NULL, child1: NULL, child2: NULL, height: 0, user: 0 };
        i
    }

    fn release(&mut self, i: u32) {
        let n = &mut self.nodes[i as usize];
        n.parent = self.free;
        n.height = -1;
        self.free = i;
    }

    /// Adds a leaf with bounds `aabb` (already fattened by the caller); returns its proxy id.
    pub fn insert(&mut self, aabb: Aabb, user: u32) -> u32 {
        let leaf = self.alloc();
        let n = &mut self.nodes[leaf as usize];
        n.aabb = aabb;
        n.user = user;
        n.height = 0;
        self.insert_leaf(leaf);
        self.leaves += 1;
        leaf
    }

    pub fn remove(&mut self, proxy: u32) {
        self.remove_leaf(proxy);
        self.release(proxy);
        self.leaves -= 1;
    }

    /// Moves a leaf to new (fattened) bounds.
    pub fn update(&mut self, proxy: u32, aabb: Aabb) {
        self.remove_leaf(proxy);
        self.nodes[proxy as usize].aabb = aabb;
        self.insert_leaf(proxy);
    }

    pub fn fat_aabb(&self, proxy: u32) -> Aabb {
        self.nodes[proxy as usize].aabb
    }

    pub fn user(&self, proxy: u32) -> u32 {
        self.nodes[proxy as usize].user
    }

    fn insert_leaf(&mut self, leaf: u32) {
        if self.root == NULL {
            self.root = leaf;
            self.nodes[leaf as usize].parent = NULL;
            return;
        }
        let leaf_box = self.nodes[leaf as usize].aabb;
        // Find the best sibling (surface-area heuristic, greedy descent).
        let mut index = self.root;
        while !self.nodes[index as usize].is_leaf() {
            let n = self.nodes[index as usize];
            let area = n.aabb.half_area();
            let combined = n.aabb.union(&leaf_box).half_area();
            let cost = 2.0 * combined;
            let inheritance = 2.0 * (combined - area);
            let child_cost = |c: u32| {
                let cn = &self.nodes[c as usize];
                let u = leaf_box.union(&cn.aabb).half_area();
                if cn.is_leaf() {
                    u + inheritance
                } else {
                    u - cn.aabb.half_area() + inheritance
                }
            };
            let (c1, c2) = (child_cost(n.child1), child_cost(n.child2));
            if cost < c1 && cost < c2 {
                break;
            }
            index = if c1 <= c2 { n.child1 } else { n.child2 };
        }
        let sibling = index;
        let old_parent = self.nodes[sibling as usize].parent;
        let new_parent = self.alloc();
        {
            let s = self.nodes[sibling as usize];
            let p = &mut self.nodes[new_parent as usize];
            p.parent = old_parent;
            p.aabb = leaf_box.union(&s.aabb);
            p.height = s.height + 1;
            p.child1 = sibling;
            p.child2 = leaf;
        }
        if old_parent != NULL {
            let op = &mut self.nodes[old_parent as usize];
            if op.child1 == sibling {
                op.child1 = new_parent;
            } else {
                op.child2 = new_parent;
            }
        } else {
            self.root = new_parent;
        }
        self.nodes[sibling as usize].parent = new_parent;
        self.nodes[leaf as usize].parent = new_parent;
        self.refit(self.nodes[leaf as usize].parent);
    }

    fn remove_leaf(&mut self, leaf: u32) {
        if leaf == self.root {
            self.root = NULL;
            return;
        }
        let parent = self.nodes[leaf as usize].parent;
        let grand = self.nodes[parent as usize].parent;
        let p = self.nodes[parent as usize];
        let sibling = if p.child1 == leaf { p.child2 } else { p.child1 };
        if grand != NULL {
            let g = &mut self.nodes[grand as usize];
            if g.child1 == parent {
                g.child1 = sibling;
            } else {
                g.child2 = sibling;
            }
            self.nodes[sibling as usize].parent = grand;
            self.release(parent);
            self.refit(grand);
        } else {
            self.root = sibling;
            self.nodes[sibling as usize].parent = NULL;
            self.release(parent);
        }
    }

    /// Walks up from `index`, rebalancing and refitting bounds and heights.
    fn refit(&mut self, mut index: u32) {
        while index != NULL {
            index = self.balance(index);
            let n = self.nodes[index as usize];
            let (c1, c2) = (self.nodes[n.child1 as usize], self.nodes[n.child2 as usize]);
            let node = &mut self.nodes[index as usize];
            node.height = 1 + c1.height.max(c2.height);
            node.aabb = c1.aabb.union(&c2.aabb);
            index = node.parent;
        }
    }

    /// A left or right rotation if `ia` is unbalanced; returns the subtree's new root.
    fn balance(&mut self, ia: u32) -> u32 {
        let a = self.nodes[ia as usize];
        if a.is_leaf() || a.height < 2 {
            return ia;
        }
        let (ib, ic) = (a.child1, a.child2);
        let (b, c) = (self.nodes[ib as usize], self.nodes[ic as usize]);
        let balance = c.height - b.height;
        if balance > 1 {
            // Rotate C up.
            let (i_f, i_g) = (c.child1, c.child2);
            let (f, g) = (self.nodes[i_f as usize], self.nodes[i_g as usize]);
            self.nodes[ic as usize].child1 = ia;
            self.nodes[ic as usize].parent = a.parent;
            self.nodes[ia as usize].parent = ic;
            self.replace_child(a.parent, ia, ic);
            if f.height > g.height {
                self.nodes[ic as usize].child2 = i_f;
                self.nodes[ia as usize].child2 = i_g;
                self.nodes[i_g as usize].parent = ia;
                let a_box = b.aabb.union(&g.aabb);
                self.nodes[ia as usize].aabb = a_box;
                self.nodes[ic as usize].aabb = a_box.union(&f.aabb);
                let ah = 1 + b.height.max(g.height);
                self.nodes[ia as usize].height = ah;
                self.nodes[ic as usize].height = 1 + ah.max(f.height);
            } else {
                self.nodes[ic as usize].child2 = i_g;
                self.nodes[ia as usize].child2 = i_f;
                self.nodes[i_f as usize].parent = ia;
                let a_box = b.aabb.union(&f.aabb);
                self.nodes[ia as usize].aabb = a_box;
                self.nodes[ic as usize].aabb = a_box.union(&g.aabb);
                let ah = 1 + b.height.max(f.height);
                self.nodes[ia as usize].height = ah;
                self.nodes[ic as usize].height = 1 + ah.max(g.height);
            }
            return ic;
        }
        if balance < -1 {
            // Rotate B up.
            let (i_d, i_e) = (b.child1, b.child2);
            let (d, e) = (self.nodes[i_d as usize], self.nodes[i_e as usize]);
            self.nodes[ib as usize].child1 = ia;
            self.nodes[ib as usize].parent = a.parent;
            self.nodes[ia as usize].parent = ib;
            self.replace_child(a.parent, ia, ib);
            if d.height > e.height {
                self.nodes[ib as usize].child2 = i_d;
                self.nodes[ia as usize].child1 = i_e;
                self.nodes[i_e as usize].parent = ia;
                let a_box = c.aabb.union(&e.aabb);
                self.nodes[ia as usize].aabb = a_box;
                self.nodes[ib as usize].aabb = a_box.union(&d.aabb);
                let ah = 1 + c.height.max(e.height);
                self.nodes[ia as usize].height = ah;
                self.nodes[ib as usize].height = 1 + ah.max(d.height);
            } else {
                self.nodes[ib as usize].child2 = i_e;
                self.nodes[ia as usize].child1 = i_d;
                self.nodes[i_d as usize].parent = ia;
                let a_box = c.aabb.union(&d.aabb);
                self.nodes[ia as usize].aabb = a_box;
                self.nodes[ib as usize].aabb = a_box.union(&e.aabb);
                let ah = 1 + c.height.max(d.height);
                self.nodes[ia as usize].height = ah;
                self.nodes[ib as usize].height = 1 + ah.max(e.height);
            }
            return ib;
        }
        ia
    }

    fn replace_child(&mut self, parent: u32, old: u32, new: u32) {
        if parent == NULL {
            self.root = new;
            return;
        }
        let p = &mut self.nodes[parent as usize];
        if p.child1 == old {
            p.child1 = new;
        } else {
            p.child2 = new;
        }
    }

    /// Calls `f(user)` for every leaf whose fat box overlaps `b`, until `f` returns `false`.
    /// The order follows the tree's shape: sort the results before they reach the simulation.
    pub fn query(&self, b: &Aabb, mut f: impl FnMut(u32) -> bool) {
        if self.root == NULL {
            return;
        }
        let mut stack = Stack::new();
        stack.push(self.root);
        while let Some(i) = stack.pop() {
            let n = &self.nodes[i as usize];
            if !n.aabb.overlaps(b) {
                continue;
            }
            if n.is_leaf() {
                if !f(n.user) {
                    return;
                }
            } else {
                stack.push(n.child1);
                stack.push(n.child2);
            }
        }
    }

    /// Calls `f(user, max_t)` for leaves the ray `origin + t·dir`, `t ∈ [0, max_t]`, may hit;
    /// `f` returns the new `max_t` (lower it to prune farther leaves).
    pub fn ray(&self, origin: Vec3, dir: Vec3, mut max_t: f32, mut f: impl FnMut(u32, f32) -> f32) {
        if self.root == NULL {
            return;
        }
        let inv = inv_dir(dir);
        let mut stack = Stack::new();
        stack.push(self.root);
        while let Some(i) = stack.pop() {
            let n = &self.nodes[i as usize];
            if n.aabb.ray(origin, inv, max_t).is_none() {
                continue;
            }
            if n.is_leaf() {
                max_t = f(n.user, max_t);
            } else {
                stack.push(n.child1);
                stack.push(n.child2);
            }
        }
    }

    /// Height of the tree (0 for empty or a single leaf), for tests and stats.
    pub fn height(&self) -> i32 {
        if self.root == NULL {
            0
        } else {
            self.nodes[self.root as usize].height
        }
    }

    #[cfg(test)]
    fn validate(&self) {
        fn walk(t: &Tree, i: u32) -> (i32, usize) {
            let n = t.nodes[i as usize];
            if n.is_leaf() {
                assert_eq!(n.height, 0);
                return (0, 1);
            }
            assert_eq!(t.nodes[n.child1 as usize].parent, i);
            assert_eq!(t.nodes[n.child2 as usize].parent, i);
            let (h1, l1) = walk(t, n.child1);
            let (h2, l2) = walk(t, n.child2);
            assert_eq!(n.height, 1 + h1.max(h2));
            assert!(n.aabb.contains(&t.nodes[n.child1 as usize].aabb) && n.aabb.contains(&t.nodes[n.child2 as usize].aabb));
            assert!((h1 - h2).abs() <= 1 + 1, "unbalanced");
            (n.height, l1 + l2)
        }
        if self.root != NULL {
            assert_eq!(self.nodes[self.root as usize].parent, NULL);
            assert_eq!(walk(self, self.root).1, self.leaves);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::v3;

    #[test]
    fn inserts_removes_and_queries_stay_consistent() {
        let mut t = Tree::default();
        let mut proxies = Vec::new();
        for i in 0..200u32 {
            let c = v3((i % 20) as f32 * 2.0, (i / 20) as f32 * 2.0, 0.0);
            proxies.push(t.insert(Aabb::around(c, v3(0.6, 0.6, 0.6)), i));
            t.validate();
        }
        assert!(t.height() <= 12, "height {}", t.height());
        let mut hits = Vec::new();
        t.query(&Aabb::around(v3(0.0, 0.0, 0.0), v3(0.5, 0.5, 0.5)), |u| {
            hits.push(u);
            true
        });
        assert_eq!(hits, vec![0]);
        for (i, p) in proxies.iter().enumerate().step_by(3) {
            t.remove(*p);
            let _ = i;
            t.validate();
        }
        for (i, p) in proxies.iter().enumerate() {
            if i % 3 != 0 {
                t.update(*p, Aabb::around(v3(i as f32, 50.0, 0.0), v3(0.4, 0.4, 0.4)));
            }
        }
        t.validate();
        let mut ray_hits = Vec::new();
        t.ray(v3(-5.0, 50.0, 0.0), v3(1.0, 0.0, 0.0), 1000.0, |u, max| {
            ray_hits.push(u);
            max
        });
        ray_hits.sort_unstable();
        assert_eq!(ray_hits.len(), 200 - 67);
    }
}
