use cgx2_runtime::default_managed::{model::*, run, Snapshot};
use cgx2_runtime::ordinary_memory::{self, EntryPolicyV1, MemoryOpV1, MemoryProgramV1};

fn model() -> Program {
    Program {
        root_maps: vec![],
        strings: vec!["value".into()],
        layouts: [
            Shape::KInt,
            Shape::KUnit,
            Shape::KError,
            Shape::KList(0),
            Shape::KVariant(vec![(0, vec![3]), (0, vec![2])]),
            Shape::KBool,
        ]
        .into_iter()
        .map(|shape| Layout { name: 0, shape })
        .collect(),
        expressions: vec![],
        functions: vec![],
        entry: 0,
        schema_version: 1,
        output_loc: Loc::default(),
    }
}

fn expr(p: &mut Program, ty: usize, op: Op) -> usize {
    let index = p.expressions.len();
    p.expressions.push(Expression {
        ty,
        op,
        loc: Loc::default(),
    });
    index
}

fn function(body: Vec<Statement>, result: usize) -> Function {
    Function {
        name: 0,
        parameters: vec![],
        parameter_types: vec![],
        result,
        behavioral_effects: vec![],
        capabilities: vec![],
        requires: vec![],
        ensures: vec![],
        result_name: 99,
        body,
        effects: MemoryEffectsV1::default(),
        loc: Loc::default(),
    }
}

fn matched(e: usize, bindings: Vec<usize>, body: Vec<Statement>) -> Statement {
    Statement::SMatch(
        e,
        vec![Arm {
            case: Some(0),
            bindings,
            body,
        }],
    )
}

#[test]
fn only_inline_payloads_transfer_the_scrutinee_registration() {
    for inline in [false, true] {
        for bound in [false, true] {
            let mut p = model();
            let integer = expr(&mut p, 0, Op::EInt(42));
            let list = expr(&mut p, 3, Op::EConstruct(0, vec![integer]));
            let carrier = expr(
                &mut p,
                4,
                if inline {
                    Op::ETry(list)
                } else {
                    Op::EConstruct(0, vec![list])
                },
            );
            p.functions.push(function(
                vec![matched(
                    carrier,
                    if bound { vec![0] } else { vec![] },
                    vec![Statement::SExpr(integer)],
                )],
                0,
            ));
            let e = run(&p, &[]);
            assert_eq!(e.result, Ok(Snapshot::Integer(42)));
            assert_eq!(e.evidence.roots_registered, e.evidence.roots_released);
            assert_eq!(e.evidence.cleanup_failures, 0);
            assert_eq!(e.evidence.final_roots, e.evidence.baseline_roots);
            // Heap carrier extraction keeps both parent and child protected;
            // a binding-free arm still releases the inline scrutinee.
            assert_eq!(
                e.evidence.roots_registered,
                if inline {
                    1
                } else if bound {
                    3
                } else {
                    2
                }
            );
        }
    }
}

#[test]
fn returned_inline_payload_keeps_a_separate_obligation_during_nested_cleanup() {
    for nested in [false, true] {
        let mut p = model();
        let integer = expr(&mut p, 0, Op::EInt(42));
        let list = expr(&mut p, 3, Op::EConstruct(0, vec![integer]));
        let carrier = expr(&mut p, 4, Op::ETry(list));
        let alias = expr(&mut p, 3, Op::ELocal(0));
        let mut body = vec![Statement::SReturn(Some(alias))];
        if nested {
            let yes = expr(&mut p, 5, Op::EBool(true));
            body = vec![Statement::SIf(yes, body, vec![])];
        }
        p.functions
            .push(function(vec![matched(carrier, vec![0], body)], 3));
        let e = run(&p, &[]);
        assert_eq!(
            e.result,
            Ok(Snapshot::Sequence(false, vec![Snapshot::Integer(42)]))
        );
        assert_eq!(e.evidence.roots_registered, e.evidence.roots_released);
        assert_eq!(e.evidence.final_roots, e.evidence.baseline_roots);
        assert_eq!(e.evidence.cleanup_failures, 0);
    }
}

#[cfg(feature = "fault-injection")]
#[test]
fn copy_argument_allocation_and_publication_failures_recover_once() {
    let mut p = model();
    let region = expr(&mut p, 1, Op::EUnit);
    let integer = expr(&mut p, 0, Op::EInt(42));
    let list = expr(&mut p, 3, Op::EConstruct(0, vec![integer]));
    let copy = expr(&mut p, 4, Op::EUnit);
    let failed = expr(&mut p, 0, Op::EInt(-5));
    p.functions.push(function(
        vec![Statement::SMatch(
            copy,
            vec![
                Arm {
                    case: Some(0),
                    bindings: vec![0],
                    body: vec![Statement::SExpr(integer)],
                },
                Arm {
                    case: Some(1),
                    bindings: vec![1],
                    body: vec![Statement::SExpr(failed)],
                },
            ],
        )],
        0,
    ));
    let p = MemoryProgramV1 {
        program: p,
        operations: [
            (region, MemoryOpV1::DefaultRegion),
            (
                copy,
                MemoryOpV1::CopyTo {
                    region,
                    value: list,
                },
            ),
        ]
        .into(),
        entry_policy: EntryPolicyV1::Implicit,
    };
    let mut recovered = 0;
    for index in 1..=64 {
        let mut session = cgx2_runtime::Session::try_new().unwrap();
        session.inject_host_failure_at_reserve(index);
        let e = ordinary_memory::run_with_session(&p, &[], session);
        assert!(
            matches!(e.result, Ok(Snapshot::Integer(42 | -5))),
            "reserve {index}: {e:?}"
        );
        let evidence = e.evidence.unwrap();
        if e.result == Ok(Snapshot::Integer(-5)) {
            recovered += 1;
            assert_eq!(evidence.recovered, 1);
        } else {
            assert_eq!(evidence.recovered, 0);
        }
        assert_eq!(evidence.roots_registered, evidence.roots_released);
        assert_eq!(evidence.cleanup_failures, 0);
        assert_eq!(evidence.baseline_roots, evidence.final_roots);
    }
    assert!(
        recovered >= 10,
        "must reach argument, copy and publication reserves"
    );
}

#[test]
fn replacing_the_transferred_binding_preserves_an_independent_returned_alias() {
    let mut p = model();
    let integer = expr(&mut p, 0, Op::EInt(42));
    let list = expr(&mut p, 3, Op::EConstruct(0, vec![integer]));
    let carrier = expr(&mut p, 4, Op::ETry(list));
    let binding = expr(&mut p, 3, Op::ELocal(0));
    let alias = expr(&mut p, 3, Op::ELocal(1));
    let replacement = expr(&mut p, 3, Op::EConstruct(0, vec![integer, integer]));
    p.functions.push(function(
        vec![matched(
            carrier,
            vec![0],
            vec![
                Statement::SLet(1, binding),
                Statement::SAssign(0, 0, replacement),
                Statement::SReturn(Some(alias)),
            ],
        )],
        3,
    ));
    let e = run(&p, &[]);
    assert_eq!(
        e.result,
        Ok(Snapshot::Sequence(false, vec![Snapshot::Integer(42)]))
    );
    assert_eq!(e.evidence.roots_registered, e.evidence.roots_released);
    assert_eq!(e.evidence.baseline_roots, e.evidence.final_roots);
    assert_eq!(e.evidence.cleanup_failures, 0);
}
