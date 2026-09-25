//! Associative Range Query Tree with dynamic allocation, supporting sparse
//! initialization and persistence
use super::ArqSpec;

pub struct DynamicArqNode<T: ArqSpec> {
    val: T::S,
    app: Option<T::F>,
    down: (usize, usize),
}

// TODO: in a future Rust version, this might be replaced by a #[derive(Clone)]
impl<T: ArqSpec> Clone for DynamicArqNode<T> {
    fn clone(&self) -> Self {
        Self {
            val: self.val.clone(),
            app: self.app.clone(),
            down: self.down,
        }
    }
}

impl<T: ArqSpec> Default for DynamicArqNode<T> {
    fn default() -> Self {
        Self {
            val: T::identity(),
            app: None,
            down: (usize::MAX, usize::MAX),
        }
    }
}

impl<T: ArqSpec> DynamicArqNode<T> {
    fn apply(&mut self, f: &T::F, size: i64) {
        self.val = T::apply(f, &self.val, size);
        if size > 1 {
            let h = match self.app {
                Some(ref g) => T::compose(f, g),
                None => f.clone(),
            };
            self.app = Some(h);
        }
    }
}

pub type ArqView = (usize, i64);

/// A dynamic, and optionally persistent, associative range query data structure.
pub struct DynamicArq<T: ArqSpec> {
    nodes: Vec<DynamicArqNode<T>>,
    is_persistent: bool,
}

impl<T: ArqSpec> DynamicArq<T> {
    /// Initializes the data structure without creating any nodes.
    pub fn new(is_persistent: bool) -> Self {
        Self {
            nodes: vec![],
            is_persistent,
        }
    }

    /// Lazily builds a tree initialized to the identity.
    pub fn build_from_identity(&mut self, size: i64) -> ArqView {
        self.nodes.push(DynamicArqNode::default());
        (self.nodes.len() - 1, size)
    }

    /// Builds a tree whose leaves are set to a given non-empty slice.
    pub fn build_from_slice(&mut self, init_val: &[T::S]) -> ArqView {
        if init_val.len() == 1 {
            let root = DynamicArqNode {
                val: init_val[0].clone(),
                ..Default::default()
            };
            self.nodes.push(root);
            (self.nodes.len() - 1, 1)
        } else {
            let ls = init_val.len() / 2;
            let (l_init, r_init) = init_val.split_at(ls);
            let l_view = self.build_from_slice(l_init);
            let r_view = self.build_from_slice(r_init);
            self.merge_equal_sized(l_view, r_view)
        }
    }

    /// Merges two balanced subtrees into a single tree with a 0-indexed view.
    pub fn merge_equal_sized(&mut self, (lp, ls): ArqView, (rp, rs): ArqView) -> ArqView {
        assert!(ls == rs || ls + 1 == rs);
        let p = self.nodes.len();
        let root = DynamicArqNode {
            down: (lp, rp),
            ..Default::default()
        };
        self.nodes.push(root);
        self.pull(p);
        (p, ls + rs)
    }

    pub fn push(&mut self, (p, s): ArqView) -> (ArqView, ArqView) {
        if self.nodes[p].down.0 == usize::MAX {
            self.nodes.push(DynamicArqNode::default());
            self.nodes.push(DynamicArqNode::default());
            self.nodes[p].down = (self.nodes.len() - 2, self.nodes.len() - 1)
        };
        let (lp, rp) = self.nodes[p].down;
        let ls = s / 2;
        if let Some(ref f) = self.nodes[p].app.take() {
            self.nodes[lp].apply(f, ls);
            self.nodes[rp].apply(f, s - ls);
        }
        ((lp, ls), (rp, s - ls))
    }

    pub fn pull(&mut self, p: usize) {
        let (lp, rp) = self.nodes[p].down;
        let left_val = &self.nodes[lp].val;
        let right_val = &self.nodes[rp].val;
        self.nodes[p].val = T::op(left_val, right_val);
    }

    /// Applies the endomorphism f to all entries from l to r, inclusive.
    /// If l == r, the updates are eager. Otherwise, they are lazy.
    ///
    /// On the boundary of a partial overlap, the required lazy-propagation
    /// bookkeeping is performed locally: node allocation, persistence cloning
    /// of the visited path, downward flow of the pending tags, and recomposed
    /// aggregation of the two children.
    pub fn update(&mut self, view: ArqView, l: i64, r: i64, f: &T::F) -> ArqView {
        let (p_orig, s) = view;
        // The two boundary predicates of the entry point are named up front so
        // the three update regimes read at a glance: a dead region never
        // touches the tree, a saturated region swallows the update whole,
        // and anything in between walks into the children.
        let dead_region = r < 0 || s - 1 < l;
        let saturated = l <= 0 && s - 1 <= r;
        if dead_region {
            view
        } else if saturated {
            // Total overlap: f covers this whole subtree.
            // Persistence protocol: clone the path we are about to rewrite.
            let p_clone = if self.is_persistent {
                let node = self.nodes[p_orig].clone();
                self.nodes.push(node);
                let clone_slot = self.nodes.len() - 1;
                clone_slot
            } else {
                p_orig
            };
            // This subtree eagerly takes the update, composing it onto any
            // pending tag when it denies the children an individual identity.
            self.nodes[p_clone].val = T::apply(f, &self.nodes[p_clone].val, s);
            if s > 1 {
                let composed = match self.nodes[p_clone].app {
                    Some(ref g) => T::compose(f, g),
                    None => f.clone(),
                };
                self.nodes[p_clone].app = Some(composed);
            }
            (p_clone, s)
        } else {
            // Partial overlap: the children of p_orig live in separate halves.
            // Endow p_orig with children if it has none so far.
            if self.nodes[p_orig].down.0 == usize::MAX {
                self.nodes.push(DynamicArqNode::default());
                self.nodes.push(DynamicArqNode::default());
                self.nodes[p_orig].down = (self.nodes.len() - 2, self.nodes.len() - 1);
            }
            let (lp, rp) = self.nodes[p_orig].down;
            let ls = s / 2;
            let rs = s - ls;
            // Flush p_orig's pending tag into the children before descending,
            // so the recursion below sees fully-updated subtrees.
            if let Some(ref pending_tag) = self.nodes[p_orig].app.take() {
                // Left child, covering ls leaves:
                self.nodes[lp].val = T::apply(pending_tag, &self.nodes[lp].val, ls);
                if ls > 1 {
                    let composed = match self.nodes[lp].app {
                        Some(ref g) => T::compose(pending_tag, g),
                        None => pending_tag.clone(),
                    };
                    self.nodes[lp].app = Some(composed);
                }
                // Right child, covering rs leaves:
                self.nodes[rp].val = T::apply(pending_tag, &self.nodes[rp].val, rs);
                if rs > 1 {
                    let composed = match self.nodes[rp].app {
                        Some(ref g) => T::compose(pending_tag, g),
                        None => pending_tag.clone(),
                    };
                    self.nodes[rp].app = Some(composed);
                }
            }
            let l_view = (lp, ls);
            let r_view = (rp, rs);
            // Persistence protocol: clone the path we are about to rewrite.
            let p_clone = if self.is_persistent {
                let node = self.nodes[p_orig].clone();
                self.nodes.push(node);
                let clone_slot = self.nodes.len() - 1;
                clone_slot
            } else {
                p_orig
            };
            let lp_clone = self.update(l_view, l, r, f).0;
            let rp_clone = self.update(r_view, l - ls, r - ls, f).0;
            self.nodes[p_clone].down = (lp_clone, rp_clone);
            // Restore the parent aggregate from the (possibly cloned) children.
            let (lp_agg, rp_agg) = self.nodes[p_clone].down;
            let left_val = &self.nodes[lp_agg].val;
            let right_val = &self.nodes[rp_agg].val;
            self.nodes[p_clone].val = T::op(left_val, right_val);
            (p_clone, s)
        }
    }

    /// Returns the aggregate range query on all entries from l to r, inclusive.
    pub fn query(&mut self, view: ArqView, l: i64, r: i64) -> T::S {
        let (p, s) = view;
        if r < 0 || s - 1 < l {
            T::identity()
        } else if l <= 0 && s - 1 <= r {
            self.nodes[p].val.clone()
        } else {
            let (l_view, r_view) = self.push(view);
            let ls = l_view.1;
            let l_agg = self.query(l_view, l, r);
            let r_agg = self.query(r_view, l - ls, r - ls);
            T::op(&l_agg, &r_agg)
        }
    }
}

/// An example of binary search to find the first position whose element is negative.
/// The DynamicArq version works on trees of any size, not necessarily a power of two.
pub fn first_negative(arq: &mut DynamicArq<super::specs::AssignMin>, view: ArqView) -> Option<i64> {
    let (p, s) = view;
    if s == 1 {
        Some(0).filter(|_| arq.nodes[p].val < 0)
    } else {
        let (l_view, r_view) = arq.push(view);
        let (lp, ls) = l_view;
        if arq.nodes[lp].val < 0 {
            first_negative(arq, l_view)
        } else {
            first_negative(arq, r_view).map(|x| ls + x)
        }
    }
}
