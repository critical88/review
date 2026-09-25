use std::cmp;
use std::collections::{BTreeMap, BTreeSet};

use regex;
use twiggy_ir as ir;
use twiggy_opt as opt;
use twiggy_traits as traits;

mod emit;
mod entry;

use self::entry::MonosEntry;

#[derive(Debug)]
struct Monos {
    monos: Vec<MonosEntry>,
}

/// Find bloaty monomorphizations of generic functions.
pub fn monos(items: &mut ir::Items, opts: &opt::Monos) -> anyhow::Result<Box<dyn traits::Emit>> {
    let args_given = !opts.functions().is_empty();
    let using_regexps = opts.using_regexps();
    let regexps = regex::RegexSet::new(opts.functions())?;

    // Gather each generic function and its monomorphizations. Duplicated
    // instantiations of the same generic are collapsed, then the instantiations
    // of every generic are sorted by descending size and, on ties, by name.
    let mut per_generic: BTreeMap<&str, BTreeSet<(String, u32)>> = BTreeMap::new();
    for item in items.iter() {
        if let Some(generic) = item.monomorphization_of() {
            let keep = if args_given {
                if using_regexps {
                    regexps.is_match(generic)
                } else {
                    opts.functions().iter().any(|name| name == generic)
                }
            } else {
                true
            };
            if keep {
                per_generic
                    .entry(generic)
                    .or_insert_with(BTreeSet::new)
                    .insert((item.name().to_string(), item.size()));
            }
        }
    }

    let mut monos_map: BTreeMap<&str, Vec<(String, u32)>> = BTreeMap::new();
    for (generic, inst_set) in per_generic {
        let mut insts = inst_set.into_iter().collect::<Vec<_>>();
        insts.sort_by(|(a_name, a_size), (b_name, b_size)| {
            b_size.cmp(a_size).then(a_name.cmp(b_name))
        });
        monos_map.insert(generic, insts);
    }

    // Walk each generic's instantiations, summarizing the bloat and truncating
    // the list according to the requested options.
    let only_generics = opts.only_generics();
    let max_monos = opts.max_monos() as usize;
    let mut monos: Vec<MonosEntry> = Vec::new();
    for (generic, mut insts) in monos_map {
        let max = insts.iter().map(|(_, size)| *size).max();
        if let Some(max) = max {
            let total_size = insts.iter().map(|(_, size)| *size).sum::<u32>();
            let inst_cnt = insts.len() as u32;
            let size_per_inst = total_size / inst_cnt;
            let avg_savings = size_per_inst * (inst_cnt - 1);
            let removing_largest_savings = total_size - max;
            let approx_potential_savings = cmp::min(avg_savings, removing_largest_savings);

            if only_generics {
                insts.truncate(0);
            } else {
                let mut rem_cnt: u32 = 0;
                let mut rem_size: u32 = 0;
                for (_, size) in insts.iter().skip(max_monos) {
                    rem_cnt += 1;
                    rem_size += *size;
                }
                insts.truncate(max_monos);
                if rem_cnt > 0 {
                    insts.push((format!("... and {} more.", rem_cnt), rem_size));
                }
            }

            monos.push(MonosEntry {
                name: generic.to_string(),
                insts,
                size: total_size,
                bloat: approx_potential_savings,
            });
        }
    }
    monos.sort();

    // Append a row that summarizes the generics we are truncating, plus a row
    // spanning the totals for the whole set, before truncating the display.
    let max_generics = opts.max_generics() as usize;
    let remaining: Option<MonosEntry> = if monos.len() > max_generics {
        let mut rem_cnt: usize = 0;
        let mut rem_size: u32 = 0;
        let mut rem_savings: u32 = 0;
        for entry in monos.iter().skip(max_generics) {
            rem_cnt += 1 + entry.insts.len();
            rem_size += entry.size;
            rem_savings += entry.bloat;
        }
        Some(MonosEntry {
            name: format!("... and {} more.", rem_cnt),
            size: rem_size,
            insts: vec![],
            bloat: rem_savings,
        })
    } else {
        None
    };

    let total = {
        let mut total_cnt: usize = 0;
        let mut total_size: u32 = 0;
        let mut total_savings: u32 = 0;
        for entry in monos.iter() {
            total_cnt += 1 + entry.insts.len();
            total_size += entry.size;
            total_savings += entry.bloat;
        }
        MonosEntry {
            name: format!("Σ [{} Total Rows]", total_cnt),
            size: total_size,
            insts: vec![],
            bloat: total_savings,
        }
    };

    monos.truncate(max_generics);
    if let Some(remaining) = remaining {
        monos.push(remaining);
    }
    monos.push(total);

    Ok(Box::new(Monos { monos }) as Box<_>)
}
