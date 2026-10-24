//! Shared interpreter/native bridge implementation. No second Session is created.
use super::owned_bridge::{run_with_bridge, EvidenceV2, Failure, ManagedBridge, Model, Scalar};
use cgx2_runtime::default_managed::{model::Program, Evidence, Snapshot};
use cgx2_runtime::{MemoryReport, Session};
#[derive(Clone, Debug)]
pub struct HybridExecution {
    pub result: Result<Scalar, Failure>,
    pub owned: EvidenceV2,
    pub managed: Evidence,
    pub memory: MemoryReport,
}
struct Bridge<'a> {
    p: &'a Program,
    entries: &'a [(String, usize)],
    evidence: Evidence,
}
impl ManagedBridge for Bridge<'_> {
    fn invoke(
        &mut self,
        name: &str,
        args: &[Scalar],
        session: &mut Session,
        remaining: usize,
        depth: usize,
    ) -> Option<(Result<Scalar, Failure>, usize)> {
        let id = self.entries.iter().find(|(n, _)| n == name)?.1;
        let args = args
            .iter()
            .map(|v| match v {
                Scalar::I64(n) => Snapshot::Integer(*n),
                Scalar::Bool(b) => Snapshot::Boolean(*b),
                Scalar::Unit => Snapshot::Unit,
                Scalar::Record { .. } => unreachable!("authenticated scalar bridge"),
            })
            .collect::<Vec<_>>();
        let e = cgx2_runtime::default_managed::run_function_borrowed(
            self.p, id, &args, session, remaining, depth,
        );
        self.evidence.allocations += e.evidence.allocations;
        self.evidence.roots_registered += e.evidence.roots_registered;
        self.evidence.roots_released += e.evidence.roots_released;
        self.evidence.cleanup_failures += e.evidence.cleanup_failures;
        self.evidence.recovered += e.evidence.recovered;
        self.evidence.guard_entries += e.evidence.guard_entries;
        self.evidence.guard_exits += e.evidence.guard_exits;
        self.evidence.guard_entry_failures += e.evidence.guard_entry_failures;
        self.evidence.graph_reads += e.evidence.graph_reads;
        self.evidence.graph_writes += e.evidence.graph_writes;
        self.evidence.final_roots = e.evidence.final_roots;
        self.evidence.steps += e.evidence.steps;
        self.evidence.memory_effects_v1 = self
            .evidence
            .memory_effects_v1
            .union(self.p.functions[id].effects);
        self.evidence.behavioral_effects.extend(
            self.p.functions[id]
                .behavioral_effects
                .iter()
                .map(|i| self.p.strings[*i].clone()),
        );
        self.evidence.behavioral_effects.sort();
        self.evidence.behavioral_effects.dedup();
        self.evidence.capabilities.extend(
            self.p.functions[id]
                .capabilities
                .iter()
                .map(|i| self.p.strings[*i].clone()),
        );
        self.evidence.capabilities.sort();
        self.evidence.capabilities.dedup();
        Some((
            e.result
                .map_err(|f| Failure {
                    code: f.code,
                    message: f.message,
                    span: super::owned_bridge::source_span(f.loc),
                })
                .and_then(|v| match v {
                    Snapshot::Integer(n) => Ok(Scalar::I64(n)),
                    Snapshot::Boolean(b) => Ok(Scalar::Bool(b)),
                    Snapshot::Unit => Ok(Scalar::Unit),
                    _ => Err(Failure {
                        code: "CGR091",
                        message: "non-scalar mixed bridge result".into(),
                        span: super::owned_bridge::source_span(self.p.functions[id].loc),
                    }),
                }),
            e.evidence.steps,
        ))
    }
}
pub fn run(model: &Model, managed: &Program, entries: &[(String, usize)]) -> HybridExecution {
    match Session::try_new() {
        Ok(session) => run_with_session(model, managed, entries, session),
        Err(e) => HybridExecution {
            result: Err(Failure {
                code: "CGR080",
                message: format!("session allocation failed: {e:?}"),
                span: model.functions[model.entry].span,
            }),
            owned: EvidenceV2::default(),
            managed: Evidence::default(),
            memory: MemoryReport {
                schema_version: 1,
                regions: vec![],
                totals: Default::default(),
            },
        },
    }
}
pub fn run_with_session(
    model: &Model,
    managed: &Program,
    entries: &[(String, usize)],
    mut session: Session,
) -> HybridExecution {
    let baseline = session.root_census().map_or(0, |r| r.root_registrations);
    let mut bridge = Bridge {
        p: managed,
        entries,
        evidence: Evidence {
            baseline_roots: baseline,
            ..Evidence::default()
        },
    };
    let result = run_with_bridge(model, &mut session, &mut bridge);
    bridge.evidence.charged_bytes = session.memory_observation_v2().charged_bytes;
    bridge.evidence.final_roots = session
        .root_census()
        .map_or(usize::MAX, |r| r.root_registrations);
    HybridExecution {
        result: result.result,
        owned: result.evidence,
        managed: bridge.evidence,
        memory: session.memory_report(),
    }
}
