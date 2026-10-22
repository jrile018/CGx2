use std::collections::{BTreeMap, BTreeSet, VecDeque};

use cgx2_notebook_format::{Cell, CellKind};

/// Static facts supplied by a compiler adapter for one code cell.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CellFacts {
    pub defines: BTreeSet<String>,
    pub reads: BTreeSet<String>,
    pub opaque_effects: bool,
}

/// Follow only forward notebook dependencies. A visited set bounds cycles in
/// supplied facts and lets each downstream cell be invalidated at most once.
pub(crate) fn affected_after(
    cells: &[Cell],
    facts: &BTreeMap<String, CellFacts>,
    index: usize,
    changed_defines: &BTreeSet<String>,
    opaque_effects: bool,
) -> Vec<usize> {
    let mut queue = VecDeque::from([(index, changed_defines.clone(), opaque_effects)]);
    let mut visited = BTreeSet::new();
    while let Some((origin, definitions, opaque)) = queue.pop_front() {
        for (candidate, cell) in cells.iter().enumerate().skip(origin + 1) {
            if cell.kind != CellKind::Code || visited.contains(&candidate) {
                continue;
            }
            let cell_facts = &facts[&cell.id];
            if opaque || !definitions.is_disjoint(&cell_facts.reads) {
                visited.insert(candidate);
                queue.push_back((
                    candidate,
                    cell_facts.defines.clone(),
                    cell_facts.opaque_effects,
                ));
            }
        }
    }
    visited.into_iter().collect()
}
