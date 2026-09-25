use crate::raw::build::BuilderNode;
use crate::raw::{CompiledAddr, Output, Transition, NONE_ADDRESS};

#[derive(Debug)]
pub struct Registry {
    table: Vec<RegistryCell>,
    table_size: usize, // number of rows
    mru_size: usize,   // number of columns
}

#[derive(Debug)]
struct RegistryCache<'a> {
    cells: &'a mut [RegistryCell],
}

#[derive(Clone, Debug)]
pub struct RegistryCell {
    addr: CompiledAddr,
    node: BuilderNode,
}

#[derive(Debug)]
pub enum RegistryEntry<'a> {
    Found(CompiledAddr),
    NotFound(&'a mut RegistryCell),
    Rejected,
}

impl Registry {
    pub fn new(table_size: usize, mru_size: usize) -> Registry {
        let empty_cell = RegistryCell::none();
        let ncells = table_size.checked_mul(mru_size).unwrap();
        Registry { table: vec![empty_cell; ncells], table_size, mru_size }
    }

    pub fn entry<'a>(
        &'a mut self,
        is_final: bool,
        final_output: Output,
        trans: &[Transition],
    ) -> RegistryEntry<'a> {
        if self.table.is_empty() {
            return RegistryEntry::Rejected;
        }
        let bucket = self.hash(is_final, final_output, trans);
        let start = self.mru_size * bucket;
        let end = start + self.mru_size;
        RegistryCache { cells: &mut self.table[start..end] }
            .entry(is_final, final_output, trans)
    }

    fn hash(
        &self,
        is_final: bool,
        final_output: Output,
        trans: &[Transition],
    ) -> usize {
        // Basic FNV-1a hash as described:
        // https://en.wikipedia.org/wiki/Fowler%E2%80%93Noll%E2%80%93Vo_hash_function
        //
        // In unscientific experiments, this provides the same compression
        // as `std::hash::SipHasher` but is much much faster.
        const FNV_PRIME: u64 = 1099511628211;
        let mut h = 14695981039346656037;
        h = (h ^ (is_final as u64)).wrapping_mul(FNV_PRIME);
        h = (h ^ final_output.value()).wrapping_mul(FNV_PRIME);
        for t in trans {
            h = (h ^ (t.inp as u64)).wrapping_mul(FNV_PRIME);
            h = (h ^ t.out.value()).wrapping_mul(FNV_PRIME);
            h = (h ^ (t.addr as u64)).wrapping_mul(FNV_PRIME);
        }
        (h as usize) % self.table_size
    }
}

impl<'a> RegistryCache<'a> {
    fn entry(
        mut self,
        is_final: bool,
        final_output: Output,
        trans: &[Transition],
    ) -> RegistryEntry<'a> {
        if self.cells.len() == 1 {
            let cell = &mut self.cells[0];
            if !cell.is_none() && reg_node_eq(&cell.node, is_final, final_output, trans) {
                RegistryEntry::Found(cell.addr)
            } else {
                reg_node_store(&mut cell.node, is_final, final_output, trans);
                RegistryEntry::NotFound(cell)
            }
        } else if self.cells.len() == 2 {
            let cell1 = &mut self.cells[0];
            if !cell1.is_none()
                && reg_node_eq(&cell1.node, is_final, final_output, trans)
            {
                return RegistryEntry::Found(cell1.addr);
            }

            let cell2 = &mut self.cells[1];
            if !cell2.is_none()
                && reg_node_eq(&cell2.node, is_final, final_output, trans)
            {
                let addr = cell2.addr;
                self.cells.swap(0, 1);
                return RegistryEntry::Found(addr);
            }

            reg_node_store(&mut self.cells[1].node, is_final, final_output, trans);
            self.cells.swap(0, 1);
            RegistryEntry::NotFound(&mut self.cells[0])
        } else {
            let find = |c: &RegistryCell| {
                !c.is_none() && reg_node_eq(&c.node, is_final, final_output, trans)
            };
            if let Some(i) = self.cells.iter().position(find) {
                let addr = self.cells[i].addr;
                self.promote(i); // most recently used
                RegistryEntry::Found(addr)
            } else {
                let last = self.cells.len() - 1;
                // discard LRU
                reg_node_store(&mut self.cells[last].node, is_final, final_output, trans);
                self.promote(last);
                RegistryEntry::NotFound(&mut self.cells[0])
            }
        }
    }

    fn promote(&mut self, mut i: usize) {
        assert!(i < self.cells.len());
        while i > 0 {
            self.cells.swap(i - 1, i);
            i -= 1;
        }
    }
}

impl RegistryCell {
    fn none() -> RegistryCell {
        RegistryCell { addr: NONE_ADDRESS, node: BuilderNode::default() }
    }

    fn is_none(&self) -> bool {
        self.addr == NONE_ADDRESS
    }

    pub fn insert(&mut self, addr: CompiledAddr) {
        self.addr = addr;
    }
}

/// Compares a cached node with the pieces of a builder node supplied by the
/// caller of the registry.
fn reg_node_eq(
    node: &BuilderNode,
    is_final: bool,
    final_output: Output,
    trans: &[Transition],
) -> bool {
    node.is_final == is_final
        && node.final_output == final_output
        && node.trans == trans
}

/// Overwrites a cached node with the pieces of a builder node supplied by the
/// caller of the registry.
fn reg_node_store(
    node: &mut BuilderNode,
    is_final: bool,
    final_output: Output,
    trans: &[Transition],
) {
    node.is_final = is_final;
    node.final_output = final_output;
    node.trans.clear();
    node.trans.extend(trans.iter());
}

#[cfg(test)]
mod tests {
    use super::{Registry, RegistryCache, RegistryCell, RegistryEntry};
    use crate::raw::build::BuilderNode;
    use crate::raw::{Output, Transition};

    fn assert_rejected(entry: RegistryEntry) {
        match entry {
            RegistryEntry::Rejected => {}
            entry => panic!("expected rejected entry, got: {:?}", entry),
        }
    }

    fn assert_not_found(entry: RegistryEntry) {
        match entry {
            RegistryEntry::NotFound(_) => {}
            entry => panic!("expected nout found entry, got: {:?}", entry),
        }
    }

    fn assert_insert_and_found(reg: &mut Registry, bnode: &BuilderNode) {
        match reg.entry(bnode.is_final, bnode.final_output, &bnode.trans) {
            RegistryEntry::NotFound(cell) => cell.insert(1234),
            entry => panic!("unexpected not found entry, got: {:?}", entry),
        }
        match reg.entry(bnode.is_final, bnode.final_output, &bnode.trans) {
            RegistryEntry::Found(addr) => assert_eq!(addr, 1234),
            entry => panic!("unexpected found entry, got: {:?}", entry),
        }
    }

    #[test]
    fn empty_is_ok() {
        let mut reg = Registry::new(0, 0);
        let bnode = BuilderNode {
            is_final: false,
            final_output: Output::zero(),
            trans: vec![],
        };
        assert_rejected(reg.entry(bnode.is_final, bnode.final_output, &bnode.trans));
    }

    #[test]
    fn one_final_is_ok() {
        let mut reg = Registry::new(1, 1);
        let bnode = BuilderNode {
            is_final: true,
            final_output: Output::zero(),
            trans: vec![],
        };
        assert_insert_and_found(&mut reg, &bnode);
    }

    #[test]
    fn one_with_trans_is_ok() {
        let mut reg = Registry::new(1, 1);
        let bnode = BuilderNode {
            is_final: false,
            final_output: Output::zero(),
            trans: vec![Transition {
                addr: 0,
                inp: b'a',
                out: Output::zero(),
            }],
        };
        assert_insert_and_found(&mut reg, &bnode);
        let is_final = BuilderNode { is_final: true, ..bnode.clone() };
        assert_not_found(reg.entry(is_final.is_final, is_final.final_output, &is_final.trans));
        let other_inp = BuilderNode {
            trans: vec![Transition {
                addr: 0,
                inp: b'b',
                out: Output::zero(),
            }],
            ..bnode.clone()
        };
        assert_not_found(reg.entry(other_inp.is_final, other_inp.final_output, &other_inp.trans));
        let other_out = BuilderNode {
            trans: vec![Transition {
                addr: 0,
                inp: b'a',
                out: Output::new(1),
            }],
            ..bnode.clone()
        };
        assert_not_found(reg.entry(other_out.is_final, other_out.final_output, &other_out.trans));
    }

    #[test]
    fn cache_works() {
        let mut reg = Registry::new(1, 1);

        let bnode1 = BuilderNode { is_final: true, ..BuilderNode::default() };
        assert_insert_and_found(&mut reg, &bnode1);

        let bnode2 =
            BuilderNode { final_output: Output::new(1), ..bnode1.clone() };
        assert_insert_and_found(&mut reg, &bnode2);
        assert_not_found(reg.entry(bnode1.is_final, bnode1.final_output, &bnode1.trans));
    }

    #[test]
    fn promote() {
        let bn = BuilderNode::default();
        let mut bnodes = vec![
            RegistryCell { addr: 1, node: bn.clone() },
            RegistryCell { addr: 2, node: bn.clone() },
            RegistryCell { addr: 3, node: bn.clone() },
            RegistryCell { addr: 4, node: bn.clone() },
        ];
        let mut cache = RegistryCache { cells: &mut bnodes };

        cache.promote(0);
        assert_eq!(cache.cells[0].addr, 1);
        assert_eq!(cache.cells[1].addr, 2);
        assert_eq!(cache.cells[2].addr, 3);
        assert_eq!(cache.cells[3].addr, 4);

        cache.promote(1);
        assert_eq!(cache.cells[0].addr, 2);
        assert_eq!(cache.cells[1].addr, 1);
        assert_eq!(cache.cells[2].addr, 3);
        assert_eq!(cache.cells[3].addr, 4);

        cache.promote(3);
        assert_eq!(cache.cells[0].addr, 4);
        assert_eq!(cache.cells[1].addr, 2);
        assert_eq!(cache.cells[2].addr, 1);
        assert_eq!(cache.cells[3].addr, 3);

        cache.promote(2);
        assert_eq!(cache.cells[0].addr, 1);
        assert_eq!(cache.cells[1].addr, 4);
        assert_eq!(cache.cells[2].addr, 2);
        assert_eq!(cache.cells[3].addr, 3);
    }
}
