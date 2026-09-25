use std::collections::BTreeSet;

use regex;

use twiggy_ir as ir;
use twiggy_opt as opt;
use twiggy_traits as traits;

mod paths_emit;
mod paths_entry;

use self::paths_entry::PathsEntry;

#[derive(Debug)]
struct Paths {
    opts: opt::Paths,
    entries: Vec<PathsEntry>,
}

/// Find all retaining paths for the given items.
pub fn paths(items: &mut ir::Items, opts: &opt::Paths) -> anyhow::Result<Box<dyn traits::Emit>> {
    // The predecessor tree only needs to be computed if we are ascending
    // through the retaining paths.
    if !opts.descending() {
        items.compute_predecessors();
    }

    // Initialize the collection of Id values whose retaining paths we will emit.
    let opts = opts.clone();

    let args_given = !opts.functions().is_empty();
    let using_regexps = opts.using_regexps();
    let descending = opts.descending();

    // Resolve the top-most path entries inline, choosing the strategy that
    // matches the requested options rather than delegating to a helper that
    // no other caller needed.
    let starting_positions: Vec<ir::Id> = if args_given {
        if using_regexps {
            // Treat the arguments as regular expressions and collect any item
            // whose name matches any of them.
            let regexps = regex::RegexSet::new(opts.functions())?;
            let mut matches = Vec::new();
            for item in items.iter() {
                if regexps.is_match(item.name()) {
                    matches.push(item.id());
                }
            }
            matches
        } else {
            // Treat the arguments as exact names and look each one up.
            let mut matches = Vec::new();
            for s in opts.functions().iter() {
                if let Some(item) = items.get_item_by_name(s) {
                    matches.push(item.id());
                }
            }
            matches
        }
    } else if descending {
        // No arguments were given and we are descending: start from the meta
        // root's neighbors, sorted by descending size.
        let mut roots = items
            .neighbors(items.meta_root())
            .map(|id| &items[id])
            .collect::<Vec<_>>();
        roots.sort_by(|a, b| b.size().cmp(&a.size()));
        roots.into_iter().map(|item| item.id()).collect()
    } else {
        // No arguments were given and we are ascending: start from every item
        // but the meta root, sorted by descending size.
        let mut sorted_items = items
            .iter()
            .filter(|item| item.id() != items.meta_root())
            .collect::<Vec<_>>();
        sorted_items.sort_by(|a, b| b.size().cmp(&a.size()));
        sorted_items.iter().map(|item| item.id()).collect()
    };

    let entries = starting_positions
        .iter()
        .map(|id| create_entry(*id, items, &opts, &mut BTreeSet::new()))
        .collect();

    let paths = Paths { opts, entries };

    Ok(Box::new(paths) as Box<_>)
}

/// Create a `PathsEntry` object for the given item.
fn create_entry(
    id: ir::Id,
    items: &ir::Items,
    opts: &opt::Paths,
    seen: &mut BTreeSet<ir::Id>,
) -> PathsEntry {
    // Determine the item's name and size.
    let item = &items[id];
    let name = item.name().to_string();
    let size = item.size();

    // Collect the `ir::Id` values of this entry's children, depending on
    // whether we are ascending or descending the IR-tree.
    let children_ids: Vec<ir::Id> = if opts.descending() {
        items
            .neighbors(id)
            .map(|id| id as ir::Id)
            .filter(|id| !seen.contains(id))
            .filter(|&id| id != items.meta_root())
            .collect()
    } else {
        items
            .predecessors(id)
            .map(|id| id as ir::Id)
            .filter(|id| !seen.contains(id))
            .filter(|&id| id != items.meta_root())
            .collect()
    };

    // Temporarily add the current item to the set of discovered nodes, and
    // create an entry for each child. Collect these into a `children` vector.
    seen.insert(id);
    let children = children_ids
        .into_iter()
        .map(|id| create_entry(id, items, opts, seen))
        .collect();
    seen.remove(&id);

    PathsEntry {
        name,
        size,
        children,
    }
}
