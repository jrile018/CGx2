use std::io::Write;
use std::process::{Command, Stdio};

use cgx2_api_index::{build_index, ApiFunction, ApiIndex, ApiParameter, IndexError};
use cgx2_sema::check_module;
use cgx2_syntax::{parse_module, Span};

#[test]
fn native_api_index_exposes_unsafe_binding_and_source_span() {
    let source = "unsafe foreign native pub fn double(x: i64) -> i64 effects(Foreign) caps(native) = fixture.fixture_double runtime \"native_fixture@1.0.0\" ownership copied\n";
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    let index = build_index(&module, &typed).unwrap();
    assert_eq!(index.schema_version, 4);
    let function = &index.functions[0];
    assert_eq!(&source[function.span.start..function.span.end], source);
    assert_eq!(function.capabilities, ["native"]);
    assert_eq!(
        function.foreign.as_ref().unwrap().binding_id.as_deref(),
        Some("fixture")
    );
    let json = index.render_json();
    assert!(json.contains("\"unsafe\":true"));
    assert!(json.contains("\"binding_id\":\"fixture\""));
}

fn python_json_assert(json: &str, python_assertions: &str, args: &[&str]) {
    let program = format!(
        "import json, sys\nsys.argv = [sys.argv[0]] + [bytes.fromhex(x).decode('utf-8') for x in sys.argv[1:]]\ndata = json.load(sys.stdin)\n{python_assertions}\n"
    );
    let hex_args = args
        .iter()
        .map(|value| {
            value
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    let mut child = Command::new("python3")
        .arg("-c")
        .arg(program)
        .args(hex_args)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("python3 is required for JSON conformance tests");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(json.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "JSON validation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn extracts_only_public_source_signatures() {
    let source = "module app.math\n\nfn hidden(x: i64):\n    x\n\npub fn zebra(a: String, b: List<i64>) -> Bool:\n    true\n\npub fn alpha(x: i64) -> i64:\n    x\n";
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    assert!(typed.functions.contains_key("arg"));
    assert!(typed.functions.contains_key("arg_count"));
    assert_eq!(typed.functions["hidden"].return_type.to_string(), "i64");

    let index = build_index(&module, &typed).unwrap();
    assert_eq!(index.schema_version, 1);
    assert_eq!(index.module.as_deref(), Some("app.math"));
    assert_eq!(index.functions.len(), 2);
    assert_eq!(index.functions[0].name, "alpha");
    assert_eq!(index.functions[0].effects_status, "inferred");
    assert_eq!(index.functions[0].parameters[0].name, "x");
    assert_eq!(index.functions[0].parameters[0].ty, "i64");
    assert_eq!(index.functions[0].return_type, "i64");
    assert_eq!(index.functions[1].name, "zebra");
    assert_eq!(
        index.functions[1]
            .parameters
            .iter()
            .map(|p| (p.name.as_str(), p.ty.as_str()))
            .collect::<Vec<_>>(),
        vec![("a", "String"), ("b", "List<i64>")]
    );
    assert_eq!(index.functions[1].return_type, "Bool");
    for function in &index.functions {
        assert_eq!(
            function.span,
            module
                .functions
                .iter()
                .find(|f| f.name == function.name)
                .unwrap()
                .span
        );
    }
    assert!(!index.render_text().contains("hidden"));
    assert!(!index.render_text().contains("arg_count"));
    python_json_assert(
        &index.render_json(),
        "assert [f['name'] for f in data['functions']] == ['alpha', 'zebra']",
        &[],
    );
}

#[test]
fn rejects_stale_or_mutated_semantic_summaries() {
    let old = parse_module("pub fn answer(x: i64) -> i64:\n    x\n").unwrap();
    let changed = parse_module("pub fn answer(x: Bool) -> Bool:\n    x\n").unwrap();
    let old_typed = check_module(&old).unwrap();
    assert_eq!(
        build_index(&changed, &old_typed),
        Err(IndexError::MismatchedTypedModule)
    );

    let mut forged = check_module(&changed).unwrap();
    forged.functions.get_mut("answer").unwrap().public = false;
    assert_eq!(
        build_index(&changed, &forged),
        Err(IndexError::MismatchedTypedModule)
    );
    forged = check_module(&changed).unwrap();
    forged.functions.remove("answer");
    assert_eq!(
        build_index(&changed, &forged),
        Err(IndexError::MismatchedTypedModule)
    );

    let invalid = parse_module("pub fn answer(x: i64):\n    x\n").unwrap();
    assert!(matches!(
        build_index(&invalid, &old_typed),
        Err(IndexError::InvalidModule(diagnostics)) if diagnostics.iter().any(|d| d.code == "CGT006")
    ));
}

#[test]
fn lookup_is_exact_and_spans_count_utf8_bytes() {
    let source = "module café.tools\n\npub fn naïve(x: i64) -> i64:\n    x\n";
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    let index = build_index(&module, &typed).unwrap();
    let function = index.find("naïve").unwrap();
    assert_eq!(function.span.start, source.find("pub fn naïve").unwrap());
    assert_eq!(
        &source[function.span.start..function.span.end],
        "pub fn naïve(x: i64) -> i64:\n    x\n"
    );
    assert_eq!(index.module.as_deref(), Some("café.tools"));
    assert!(index.find("Naïve").is_none());
    assert!(index.find("naï").is_none());
    assert!(index.find("arg_count").is_none());
}

#[test]
fn private_only_module_has_empty_public_api() {
    let module = parse_module("fn helper():\n    4\n").unwrap();
    let typed = check_module(&module).unwrap();
    let index = build_index(&module, &typed).unwrap();
    assert!(index.functions.is_empty());
    assert!(index.find("helper").is_none());
}

#[test]
fn indexes_explicit_declarations_and_runtime_checked_contracts() {
    let source = "pub fn f(x: i64) -> i64 effects() caps():\n    requires x >= 0\n    ensures result >= x\n    x\n";
    let module = parse_module(source).unwrap();
    let index = build_index(&module, &check_module(&module).unwrap()).unwrap();
    let function = index.find("f").unwrap();
    assert_eq!(function.effects_status, "explicit");
    assert_eq!(function.capabilities_status, "explicit");
    assert!(function.effects.is_empty());
    assert_eq!(function.requires[0].expression, "x >= 0");
    assert_eq!(function.ensures[0].status, "runtime-checked");
    python_json_assert(&index.render_json(), "assert data['schema_version'] == 2\nf = data['functions'][0]\nassert f['effects_status'] == 'explicit' and f['capabilities_status'] == 'explicit'\nassert f['requires'][0]['expression'] == 'x >= 0'\nassert f['ensures'][0]['status'] == 'runtime-checked'", &[]);
}

#[test]
fn stable_text_and_json_schema() {
    let source = "module app.math\n\npub fn zebra(a: String, b: List<i64>) -> Bool:\n    true\n\npub fn alpha(x: i64) -> i64:\n    x\n";
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    let index = build_index(&module, &typed).unwrap();
    let alpha = index.find("alpha").unwrap().span;
    let zebra = index.find("zebra").unwrap().span;
    assert_eq!(
        index.render_text(),
        format!(
            "module app.math\npub fn alpha(x: i64) -> i64 [{}, {})\npub fn zebra(a: String, b: List<i64>) -> Bool [{}, {})\n",
            alpha.start, alpha.end, zebra.start, zebra.end
        )
    );
    python_json_assert(
        &index.render_json(),
        "assert data['schema_version'] == 1\nassert data['module'] == 'app.math'\nassert [f['name'] for f in data['functions']] == ['alpha', 'zebra']\nassert data['functions'][0]['span'] == {'start': int(sys.argv[1]), 'end': int(sys.argv[2])}\nassert data['functions'][1]['span'] == {'start': int(sys.argv[3]), 'end': int(sys.argv[4])}",
        &[
            &alpha.start.to_string(),
            &alpha.end.to_string(),
            &zebra.start.to_string(),
            &zebra.end.to_string(),
        ],
    );
}

#[test]
fn json_round_trips_escapes_controls_and_unicode() {
    let module_name = "quo\"te\\path\n雪";
    let function_name = "f\"\\\n\t\r\u{0001}é";
    let parameter_name = "p\u{0000}λ";
    let index = ApiIndex {
        variants: vec![],
        semantics_status: "checked".into(),
        schema_version: 2,
        module: Some(module_name.into()),
        functions: vec![ApiFunction {
            loop_contracts: vec![],
            parameter_types: vec![],
            result_type: None,
            type_parameters: vec![],
            name: function_name.into(),
            parameters: vec![ApiParameter {
                name: parameter_name.into(),
                ty: "T\"\\\u{001f}".into(),
            }],
            return_type: "Réponse".into(),
            span: Span { start: 2, end: 9 },
            effects: vec![],
            capabilities: vec![],
            effects_status: "inferred".into(),
            capabilities_status: "inferred".into(),
            requires: vec![],
            ensures: vec![],
            foreign: None,
        }],
        records: vec![],
    };
    python_json_assert(
        &index.render_json(),
        "assert data['schema_version'] == 2\nassert data['module'] == sys.argv[1]\nassert data['functions'][0]['name'] == sys.argv[2]\nassert data['functions'][0]['parameters'][0] == {'name': sys.argv[3], 'ty': sys.argv[4]}\nassert data['functions'][0]['return_type'] == 'Réponse'",
        &[module_name, function_name, parameter_name, "T\"\\\u{001f}"],
    );
}

#[test]
fn public_python_binding_exposes_route_target_and_runtime() {
    let source = "foreign python pub fn isqrt(x: i64) -> i64 effects(Foreign) caps(ForeignCall) = math.isqrt runtime \"local:cpython\" ownership copied\n";
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    let index = build_index(&module, &typed).unwrap();
    assert_eq!(index.schema_version, 3);
    let foreign = index.find("isqrt").unwrap().foreign.as_ref().unwrap();
    assert_eq!(foreign.route, "python");
    assert_eq!(foreign.target, "math.isqrt");
    assert_eq!(foreign.ownership, "copied");
    assert_eq!(foreign.runtime, "local:cpython");
    let json = index.render_json();
    assert!(json.contains("\"foreign\":{\"route\":\"python\",\"target\":\"math.isqrt\",\"ownership\":\"copied\",\"runtime\":\"local:cpython\"}"));
    assert!(index
        .render_text()
        .contains("foreign route=python target=math.isqrt ownership=copied runtime=local:cpython"));
}

#[test]
fn empty_public_api_renders_deterministically() {
    let module = parse_module("fn helper():\n    4\n").unwrap();
    let index = build_index(&module, &check_module(&module).unwrap()).unwrap();
    assert_eq!(index.render_text(), "module <unnamed>\n");
    assert_eq!(
        index.render_json(),
        "{\"schema_version\":1,\"module\":null,\"functions\":[]}"
    );
    python_json_assert(
        &index.render_json(),
        "assert data == {'schema_version': 1, 'module': None, 'functions': []}",
        &[],
    );
}
