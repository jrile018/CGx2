use std::collections::{BTreeMap, BTreeSet};

use cgx2_notebook_format::{source_hash, Cell, CellKind};

/// Static facts supplied by a compiler adapter for one code cell.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CellFacts {
    pub defines: BTreeSet<String>,
    pub reads: BTreeSet<String>,
    pub opaque_effects: bool,
}

/// Follow actual file-order producers. A later redefinition shadows an
/// earlier producer, so edits do not invalidate readers of the new value.
pub(crate) fn affected_after(
    cells: &[Cell],
    facts: &BTreeMap<String, CellFacts>,
    index: usize,
    changed_defines: &BTreeSet<String>,
    opaque_effects: bool,
) -> Vec<usize> {
    let mut affected = BTreeSet::new();
    let mut force_later = opaque_effects;
    for candidate in index + 1..cells.len() {
        let cell = &cells[candidate];
        if cell.kind != CellKind::Code {
            continue;
        }
        let cell_facts = &facts[&cell.id];
        let reads_changed_producer = cell_facts.reads.iter().any(|read| {
            (0..candidate)
                .rev()
                .find(|previous| {
                    cells[*previous].kind == CellKind::Code
                        && facts[&cells[*previous].id].defines.contains(read)
                })
                .is_some_and(|producer| {
                    affected.contains(&producer)
                        || (producer == index && changed_defines.contains(read))
                })
        });
        if force_later || reads_changed_producer {
            affected.insert(candidate);
            force_later |= cell_facts.opaque_effects;
        }
    }
    affected.into_iter().collect()
}

/// Hash the direct producers of a cell and their transitive source dependencies.
/// Only earlier definitions can supply a value in the file-order notebook kernel.
pub(crate) fn dependency_digest(
    cells: &[Cell],
    facts: &BTreeMap<String, CellFacts>,
    index: usize,
) -> String {
    fn digest_at(
        cells: &[Cell],
        facts: &BTreeMap<String, CellFacts>,
        index: usize,
        cache: &mut [Option<String>],
    ) -> String {
        if let Some(digest) = &cache[index] {
            return digest.clone();
        }
        let cell_facts = &facts[&cells[index].id];
        let mut producers = BTreeSet::new();
        if cell_facts.opaque_effects {
            producers.extend((0..index).filter(|previous| cells[*previous].kind == CellKind::Code));
        }
        for read in &cell_facts.reads {
            if let Some(previous) = (0..index).rev().find(|previous| {
                cells[*previous].kind == CellKind::Code
                    && facts[&cells[*previous].id].defines.contains(read)
            }) {
                producers.insert(previous);
            }
        }
        let mut encoded = String::from("CGX2-DEPENDENCIES-v2\0");
        for previous in producers {
            let producer = &cells[previous];
            let transitive = digest_at(cells, facts, previous, cache);
            for part in [&producer.id, &producer.source, &transitive] {
                encoded.push_str(&part.len().to_string());
                encoded.push(':');
                encoded.push_str(part);
            }
        }
        let digest = source_hash(&encoded);
        cache[index] = Some(digest.clone());
        digest
    }
    digest_at(cells, facts, index, &mut vec![None; cells.len()])
}
