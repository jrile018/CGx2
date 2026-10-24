use cgx2_runtime::default_managed::{model::*, run, Execution};

struct Model(Program);

impl Model {
    fn new() -> Self {
        let mut m = Self(Program {
            root_maps: vec![],
            strings: vec![],
            layouts: vec![],
            expressions: vec![],
            functions: vec![],
            entry: 0,
            schema_version: 1,
            output_loc: Loc::default(),
        });
        m.layout(Shape::KInt);
        m.layout(Shape::KBool);
        m.layout(Shape::KUnit);
        m.layout(Shape::KError);
        m.layout(Shape::KList(0));
        m.layout(Shape::KVariant(vec![(0, vec![0]), (0, vec![3])]));
        m.layout(Shape::KVariant(vec![(0, vec![4]), (0, vec![3])]));
        m
    }
    fn layout(&mut self, shape: Shape) -> usize {
        let id = self.0.layouts.len();
        let name = self.0.strings.len();
        self.0.strings.push(format!("layout{id}"));
        self.0.layouts.push(Layout { name, shape });
        id
    }
    fn expr(&mut self, ty: usize, op: Op) -> usize {
        let id = self.0.expressions.len();
        self.0.expressions.push(Expression {
            ty,
            op,
            loc: Loc::default(),
        });
        id
    }
    fn function(result: usize, body: Vec<Statement>) -> Function {
        Function {
            name: 0,
            parameters: vec![],
            parameter_types: vec![],
            result,
            behavioral_effects: vec![],
            capabilities: vec![],
            requires: vec![],
            ensures: vec![],
            result_name: 999,
            body,
            effects: MemoryEffectsV1::default(),
            loc: Loc::default(),
        }
    }
    fn execute(mut self, body: Vec<Statement>) -> Execution {
        self.0.functions.insert(0, Self::function(1, body));
        let execution = run(&self.0, &[]);
        assert_eq!(
            execution.evidence.final_roots,
            execution.evidence.baseline_roots
        );
        assert_eq!(
            execution.evidence.roots_registered,
            execution.evidence.roots_released
        );
        assert_eq!(execution.evidence.cleanup_failures, 0);
        execution
    }
    fn compare(mut self, a: usize, b: usize, unequal: bool) -> Execution {
        let result = self.expr(1, Op::EBinary(a, if unequal { 6 } else { 5 }, b));
        self.execute(vec![Statement::SReturn(Some(result))])
    }
    fn failure(&mut self, shape_limit: bool) -> usize {
        let int = self.expr(0, Op::EInt(1));
        let inner = self.expr(
            4,
            Op::EConstruct(0, vec![int; if shape_limit { 65_537 } else { 1 }]),
        );
        let outcome = self.expr(6, Op::ETry(inner));
        // The second function returns an allocation-free Err after a real zero
        // guard budget failure; the oversized list is a deterministic ShapeLimit.
        let mut body = vec![Statement::SReturn(Some(outcome))];
        if !shape_limit {
            body = vec![Statement::SGuard(0, Loc::default(), body)];
        }
        self.0.functions.push(Self::function(6, body));
        self.expr(6, Op::ECall(self.0.functions.len(), vec![]))
    }
}

#[test]
fn logical_ok_comparison_preserves_allocation_and_root_accounting() {
    for reversed in [false, true] {
        for unequal in [false, true] {
            for payload in [1, 2] {
                let mut m = Model::new();
                let one = m.expr(0, Op::EInt(1));
                let other = m.expr(0, Op::EInt(payload));
                let a = m.expr(5, Op::ETry(one));
                let b = m.expr(5, Op::EConstruct(0, vec![other]));
                let (a, b) = if reversed { (b, a) } else { (a, b) };
                let e = m.compare(a, b, unequal);
                assert_eq!(
                    e.result.unwrap().to_string(),
                    ((payload == 1) != unequal).to_string()
                );
                assert_eq!(
                    e.evidence.allocations, 1,
                    "only the ordinary variant allocates"
                );
                assert_eq!(e.evidence.recovered, 0);
                assert_eq!(e.memory.totals.collection_count, 0);
            }
        }
    }
}

#[test]
fn logical_err_comparison_checks_case_and_copied_error_payload() {
    for reversed in [false, true] {
        for unequal in [false, true] {
            for different_error in [false, true] {
                let mut m = Model::new();
                let a = m.failure(false);
                let outcome = m.failure(different_error);
                // Rebuild the second intrinsic Err as an ordinary heap variant,
                // using the same copied error payload extracted by matching.
                let payload = m.expr(3, Op::ELocal(10));
                let heap = m.expr(6, Op::EConstruct(1, vec![payload]));
                let (a, b) = if reversed { (heap, a) } else { (a, heap) };
                let eq = m.expr(1, Op::EBinary(a, if unequal { 6 } else { 5 }, b));
                let no = m.expr(1, Op::EBool(false));
                let execution = m.execute(vec![Statement::SMatch(
                    outcome,
                    vec![
                        Arm {
                            case: Some(1),
                            bindings: vec![10],
                            body: vec![Statement::SReturn(Some(eq))],
                        },
                        Arm {
                            case: Some(0),
                            bindings: vec![11],
                            body: vec![Statement::SReturn(Some(no))],
                        },
                    ],
                )]);
                assert_eq!(
                    execution.result.unwrap().to_string(),
                    ((!different_error) != unequal).to_string()
                );
                assert_eq!(execution.evidence.recovered, 2);
                assert_eq!(execution.evidence.allocations, 1);
                assert_eq!(execution.memory.totals.collection_count, 0);
            }
        }
    }
}

#[test]
fn nested_carriers_compare_logical_layers_and_preserve_nominal_types() {
    for reversed in [false, true] {
        for unequal in [false, true] {
            for different_type in [false, true] {
                let mut m = Model::new();
                let nested = m.layout(Shape::KVariant(vec![(0, vec![5]), (0, vec![3])]));
                let other = if different_type {
                    m.layout(m.0.layouts[nested].shape.clone())
                } else {
                    nested
                };
                let one = m.expr(0, Op::EInt(1));
                let carrier = m.expr(5, Op::ETry(one));
                let heap = m.expr(5, Op::EConstruct(0, vec![one]));
                let a = m.expr(nested, Op::ETry(carrier));
                let b = m.expr(other, Op::EConstruct(0, vec![heap]));
                let (a, b) = if reversed { (b, a) } else { (a, b) };
                let e = m.compare(a, b, unequal);
                assert_eq!(
                    e.result.unwrap().to_string(),
                    ((!different_type) != unequal).to_string()
                );
                assert_eq!(e.evidence.allocations, 2);
            }
        }
    }
}

#[test]
fn equality_keeps_the_existing_depth_bound() {
    let mut m = Model::new();
    let mut ty = 0;
    let mut a = m.expr(0, Op::EInt(1));
    let mut b = m.expr(0, Op::EInt(1));
    let mut body = vec![];
    for i in 0..64 {
        ty = m.layout(Shape::KRecord(vec![(0, ty)]));
        a = m.expr(ty, Op::ERecord(vec![(0, a)]));
        b = m.expr(ty, Op::ERecord(vec![(0, b)]));
        body.push(Statement::SLet(2 * i, a));
        body.push(Statement::SLet(2 * i + 1, b));
        a = m.expr(ty, Op::ELocal(2 * i));
        b = m.expr(ty, Op::ELocal(2 * i + 1));
    }
    let comparison = m.expr(1, Op::EBinary(a, 5, b));
    body.push(Statement::SReturn(Some(comparison)));
    let e = m.execute(body);
    let failure = e.result.unwrap_err();
    assert_eq!(failure.code, "CGR010");
    assert_eq!(failure.message, "equality work bound exceeded");
}
