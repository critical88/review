use std::collections::{BTreeMap, BTreeSet};

use regex;

use petgraph::visit::Walker;

use twiggy_ir as ir;
use twiggy_opt as opt;
use twiggy_traits as traits;

mod emit;

struct DominatorTree {
    tree: BTreeMap<ir::Id, Vec<ir::Id>>,
    items: Vec<ir::Id>,
    opts: opt::Dominators,
    unreachable_items_summary: Option<UnreachableItemsSummary>,
}

struct UnreachableItemsSummary {
    count: usize,
    size: u32,
    size_percent: f64,
}

/// Compute the dominator tree for the given IR graph.
pub fn dominators(
    items: &mut ir::Items,
    opts: &opt::Dominators,
) -> anyhow::Result<Box<dyn traits::Emit>> {
    items.compute_dominator_tree();
    items.compute_dominators();
    items.compute_retained_sizes();
    items.compute_predecessors();

    let arguments = opts.items();
    let dominator_items = if arguments.is_empty() {
        vec![items.meta_root()]
    } else if opts.using_regexps() {
        // Match every item whose name matches any of the given regular
        // expressions, then order the survivors by descending retained size.
        let regexps = regex::RegexSet::new(arguments)?;
        let mut sorted_items: Vec<ir::Id> = Vec::new();
        for item in items.iter() {
            if regexps.is_match(&item.name()) {
                sorted_items.push(item.id());
            }
        }
        sorted_items.sort_by_key(|id| -i64::from(items.retained_size(*id)));
        sorted_items
    } else {
        // Match by exact name instead of by regular expression.
        let mut matched_items: Vec<ir::Id> = Vec::new();
        for name in arguments.iter() {
            if let Some(item) = items.get_item_by_name(name) {
                matched_items.push(item.id());
            }
        }
        matched_items
    };

    // Summarize the items that no export or public function reaches. A DFS from
    // the meta root finds everything retained transitively; everything left over
    // is unreachable. This is only surfaced when the caller did not name any
    // item explicitly and there is at least one unreachable byte to report.
    let mut reachable_set: BTreeSet<ir::Id> = BTreeSet::new();
    for id in petgraph::visit::Dfs::new(&*items, items.meta_root()).iter(&*items) {
        reachable_set.insert(id);
    }
    let mut size = 0;
    let mut count = 0;
    for item in items.iter() {
        if !reachable_set.contains(&item.id()) {
            size += item.size();
            count += 1;
        }
    }
    let unreachable_items_summary = if opts.items().is_empty() && size > 0 {
        Some(UnreachableItemsSummary {
            count,
            size,
            size_percent: (f64::from(size)) / (f64::from(items.size())) * 100.0,
        })
    } else {
        None
    };

    let tree = DominatorTree {
        tree: items.dominator_tree().clone(),
        items: dominator_items,
        opts: opts.clone(),
        unreachable_items_summary,
    };

    Ok(Box::new(tree) as Box<_>)
}
