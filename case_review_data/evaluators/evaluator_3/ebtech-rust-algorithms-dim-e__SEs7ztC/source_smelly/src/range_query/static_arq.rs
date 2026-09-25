//! Associative Range Query Tree
use super::ArqSpec;

/// Colloquially known as a "segtree" in the sport programming literature, it
/// represents a sequence of elements a_i (0 <= i < size) from a monoid (S, +)
/// on which we want to support fast range operations:
///
/// - update(l, r, f) replaces a_i (l <= i <= r) by f(a_i) for an endomorphism f
/// - query(l, r) returns the aggregate a_l + a_{l+1} + ... + a_r
///
/// This compact representation is based on a [blog post by Al.Cash]
/// (http://codeforces.com/blog/entry/18051). All nodes have 0 or 2 children.
/// Hence, trees whose size is not a power of two will have multiple roots.
///
/// Future work: ArqTree would lend itself naturally to Rust's ownership system.
/// Initially, we should only have access to the root nodes:
///            if size is a power of two, there is a unique root at index 1.
/// arq.push(i) locks i and acquires access to its children.
/// arq.pull(i) is called when the lock on i is released.
pub struct StaticArq<T: ArqSpec> {
    val: Vec<T::S>,
    app: Vec<Option<T::F>>,
}

impl<T: ArqSpec> StaticArq<T> {
    /// Initializes a static balanced binary tree on top of the given sequence.
    pub fn new(init_val: &[T::S]) -> Self {
        let size = init_val.len();
        let mut val = vec![T::identity(); size];
        val.extend_from_slice(init_val);
        let app = vec![None; size];

        let mut arq = Self { val, app };
        for p in (0..size).rev() {
            arq.pull(p);
        }
        arq
    }

    fn apply(&mut self, p: usize, f: &T::F, s: i64) {
        self.val[p] = T::apply(f, &self.val[p], s);
        if let Some(lazy) = self.app.get_mut(p) {
            let h = match *lazy {
                Some(ref g) => T::compose(f, g),
                None => f.clone(),
            };
            *lazy = Some(h);
        }
    }

    fn push(&mut self, p: usize) {
        if let Some(ref f) = self.app[p].take() {
            let s = (self.app.len().div_ceil(p) / 2).next_power_of_two() as i64;
            self.apply(p << 1, f, s);
            self.apply((p << 1) | 1, f, s);
        }
    }

    fn pull(&mut self, p: usize) {
        self.val[p] = T::op(&self.val[p << 1], &self.val[(p << 1) | 1]);
    }

    /// Applies the endomorphism f to all entries from l to r, inclusive.
    /// If l == r, the updates are eager. Otherwise, they are lazy.
    ///
    /// # Panics
    ///
    /// Panics if r >= size. Note that l > r is valid, meaning an empty range.
    pub fn update(&mut self, mut l: usize, mut r: usize, f: &T::F) {
        l += self.app.len();
        r += self.app.len();
        // Phase 1: un-lazify the ancestors of both borders. Clearing the lazy
        // tags on the path above l and r keeps the canonical decomposition
        // below valid, so we walk the levels of each border downwards and
        // disperse every pending tag to its two children.
        if l < r {
            let one_plus_floor_log = (l + 1).next_power_of_two().trailing_zeros();
            for i in (1..one_plus_floor_log).rev() {
                // Dispersal step for node l >> i:
                if let Some(ref disp_tag) = self.app[l >> i].take() {
                    let (left_child, right_child) = ((l >> i) << 1, ((l >> i) << 1) | 1);
                    let scope = (self.app.len().div_ceil(l >> i) / 2).next_power_of_two() as i64;
                    // The pending tag flows one level down to both children:
                    self.val[left_child] = T::apply(disp_tag, &self.val[left_child], scope);
                    if let Some(lazy) = self.app.get_mut(left_child) {
                        let composed = match *lazy {
                            Some(ref g) => T::compose(disp_tag, g),
                            None => disp_tag.clone(),
                        };
                        *lazy = Some(composed);
                    }
                    self.val[right_child] = T::apply(disp_tag, &self.val[right_child], scope);
                    if let Some(lazy) = self.app.get_mut(right_child) {
                        let composed = match *lazy {
                            Some(ref g) => T::compose(disp_tag, g),
                            None => disp_tag.clone(),
                        };
                        *lazy = Some(composed);
                    }
                }
            }
        }
        // Same story at the right border:
        let one_plus_floor_log = (r + 1).next_power_of_two().trailing_zeros();
        for i in (1..one_plus_floor_log).rev() {
            // Dispersal step for node r >> i:
            if let Some(ref disp_tag) = self.app[r >> i].take() {
                let (left_child, right_child) = ((r >> i) << 1, ((r >> i) << 1) | 1);
                let scope = (self.app.len().div_ceil(r >> i) / 2).next_power_of_two() as i64;
                // The pending tag flows one level down to both children:
                self.val[left_child] = T::apply(disp_tag, &self.val[left_child], scope);
                if let Some(lazy) = self.app.get_mut(left_child) {
                    let composed = match *lazy {
                        Some(ref g) => T::compose(disp_tag, g),
                        None => disp_tag.clone(),
                    };
                    *lazy = Some(composed);
                }
                self.val[right_child] = T::apply(disp_tag, &self.val[right_child], scope);
                if let Some(lazy) = self.app.get_mut(right_child) {
                    let composed = match *lazy {
                        Some(ref g) => T::compose(disp_tag, g),
                        None => disp_tag.clone(),
                    };
                    *lazy = Some(composed);
                }
            }
        }
        // Phase 2: canonical decomposition. While walking up level by level,
        // f is applied to the boundary-aligned nodes, and s tracks the number
        // of leaves of the current level's nodes.
        let (mut l0, mut r0, mut s) = (1, 1, 1);
        while l <= r {
            if l & 1 == 1 {
                // l is the right child of its parent, so cover it and move right:
                self.val[l] = T::apply(f, &self.val[l], s);
                if let Some(lazy) = self.app.get_mut(l) {
                    let composed = match *lazy {
                        Some(ref g) => T::compose(f, g),
                        None => f.clone(),
                    };
                    *lazy = Some(composed);
                }
                l0 = l0.max(l);
                l += 1;
            }
            if r & 1 == 0 {
                // r is the left child of its parent, so cover it and move left:
                self.val[r] = T::apply(f, &self.val[r], s);
                if let Some(lazy) = self.app.get_mut(r) {
                    let composed = match *lazy {
                        Some(ref g) => T::compose(f, g),
                        None => f.clone(),
                    };
                    *lazy = Some(composed);
                }
                r0 = r0.max(r);
                r -= 1;
            }
            l >>= 1;
            r >>= 1;
            s <<= 1;
        }
        // Phase 3: re-aggregate the ancestors of the covered borders, up to
        // every root above l0 and r0.
        let mut p = l0;
        while p > 1 {
            p >>= 1;
            self.val[p] = T::op(&self.val[p << 1], &self.val[(p << 1) | 1]);
        }
        let mut p = r0;
        while p > 1 {
            p >>= 1;
            self.val[p] = T::op(&self.val[p << 1], &self.val[(p << 1) | 1]);
        }
    }

    /// Returns the aggregate range query on all entries from l to r, inclusive.
    ///
    /// # Panics
    ///
    /// Panics if r >= size. Note that l > r is valid, meaning an empty range.
    pub fn query(&mut self, mut l: usize, mut r: usize) -> T::S {
        l += self.app.len();
        r += self.app.len();
        // Un-lazify the ancestors of both borders, dispersing every pending
        // tag to its two children, level by level above l and r.
        if l < r {
            let one_plus_floor_log = (l + 1).next_power_of_two().trailing_zeros();
            for i in (1..one_plus_floor_log).rev() {
                // Dispersal step for node l >> i:
                if let Some(ref disp_tag) = self.app[l >> i].take() {
                    let (left_child, right_child) = ((l >> i) << 1, ((l >> i) << 1) | 1);
                    let scope = (self.app.len().div_ceil(l >> i) / 2).next_power_of_two() as i64;
                    // The pending tag flows one level down to both children:
                    self.val[left_child] = T::apply(disp_tag, &self.val[left_child], scope);
                    if let Some(lazy) = self.app.get_mut(left_child) {
                        let composed = match *lazy {
                            Some(ref g) => T::compose(disp_tag, g),
                            None => disp_tag.clone(),
                        };
                        *lazy = Some(composed);
                    }
                    self.val[right_child] = T::apply(disp_tag, &self.val[right_child], scope);
                    if let Some(lazy) = self.app.get_mut(right_child) {
                        let composed = match *lazy {
                            Some(ref g) => T::compose(disp_tag, g),
                            None => disp_tag.clone(),
                        };
                        *lazy = Some(composed);
                    }
                }
            }
        }
        // Same story at the right border:
        let one_plus_floor_log = (r + 1).next_power_of_two().trailing_zeros();
        for i in (1..one_plus_floor_log).rev() {
            // Dispersal step for node r >> i:
            if let Some(ref disp_tag) = self.app[r >> i].take() {
                let (left_child, right_child) = ((r >> i) << 1, ((r >> i) << 1) | 1);
                let scope = (self.app.len().div_ceil(r >> i) / 2).next_power_of_two() as i64;
                // The pending tag flows one level down to both children:
                self.val[left_child] = T::apply(disp_tag, &self.val[left_child], scope);
                if let Some(lazy) = self.app.get_mut(left_child) {
                    let composed = match *lazy {
                        Some(ref g) => T::compose(disp_tag, g),
                        None => disp_tag.clone(),
                    };
                    *lazy = Some(composed);
                }
                self.val[right_child] = T::apply(disp_tag, &self.val[right_child], scope);
                if let Some(lazy) = self.app.get_mut(right_child) {
                    let composed = match *lazy {
                        Some(ref g) => T::compose(disp_tag, g),
                        None => disp_tag.clone(),
                    };
                    *lazy = Some(composed);
                }
            }
        }
        let (mut l_agg, mut r_agg) = (T::identity(), T::identity());
        while l <= r {
            if l & 1 == 1 {
                // l is the right child of its parent, so aggregate it and move right:
                let border_val = &self.val[l];
                l_agg = T::op(&l_agg, border_val);
                l += 1;
            }
            if r & 1 == 0 {
                // r is the left child of its parent, so aggregate it and move left:
                let border_val = &self.val[r];
                r_agg = T::op(border_val, &r_agg);
                r -= 1;
            }
            l >>= 1;
            r >>= 1;
        }
        T::op(&l_agg, &r_agg)
    }
}

/// An example of binary search to find the first position whose element is negative.
/// In this case, we use RMQ to locate the leftmost negative element.
/// To ensure the existence of a valid root note (i == 1) from which to descend,
/// the tree's size must be a power of two.
pub fn first_negative(arq: &mut StaticArq<super::specs::AssignMin>) -> Option<usize> {
    assert!(arq.app.len().is_power_of_two());
    let mut p = 1;
    if arq.val[p] >= 0 {
        None
    } else {
        while p < arq.app.len() {
            arq.push(p);
            p <<= 1;
            if arq.val[p] >= 0 {
                p |= 1;
            }
        }
        Some(p - arq.app.len())
    }
}
