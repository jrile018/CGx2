use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use cgx2_notebook_format::{
    load_asset, parse_notebook_bytes, save_asset, Output, ProvenanceResult,
};
use cgx2_notebook_ui::{serve_n, UiSession};
use serde_json::{json, Value};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    dir: PathBuf,
    input: PathBuf,
}
impl Fixture {
    fn new(source: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "cgx2-ui-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let input = dir.join("source.cgnb");
        fs::write(&input, source).unwrap();
        Self { dir, input }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.dir).unwrap();
    }
}

fn server(path: &Path, count: usize) -> (u16, thread::JoinHandle<()>) {
    let session = UiSession::open(path.to_path_buf()).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = thread::spawn(move || serve_n(listener, session, count).unwrap());
    (port, handle)
}

fn request(
    port: u16,
    method: &str,
    path: &str,
    body: Option<&str>,
    token: Option<&str>,
    origin: Option<&str>,
    host: Option<&str>,
) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let mut raw = format!(
        "{method} {path} HTTP/1.1\r\nHost: {}\r\n",
        host.unwrap_or(&format!("127.0.0.1:{port}"))
    );
    if let Some(origin) = origin {
        raw.push_str(&format!("Origin: {origin}\r\n"));
    }
    if let Some(token) = token {
        raw.push_str(&format!("X-CGX2-Token: {token}\r\n"));
    }
    if let Some(body) = body {
        raw.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    raw.push_str("\r\n");
    stream.write_all(raw.as_bytes()).unwrap();
    if let Some(body) = body {
        stream.write_all(body.as_bytes()).unwrap();
    }
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, body.into())
}

fn token(port: u16) -> String {
    let (status, html) = request(port, "GET", "/", None, None, None, None);
    assert_eq!(status, 200);
    html.split("name=\"cgx2-token\" content=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .into()
}

fn post(port: u16, path: &str, value: Value, token: &str) -> (u16, Value) {
    let (status, body) = request(
        port,
        "POST",
        path,
        Some(&value.to_string()),
        Some(token),
        Some(&format!("http://127.0.0.1:{port}")),
        None,
    );
    (status, serde_json::from_str(&body).unwrap())
}

#[test]
fn edit_run_restart_save_reopen_and_input_is_unchanged() {
    let f = Fixture::new("#!cgnb 1\n%% markdown id=notes\n# Notes\n%% code id=seed\nlet x = 3\n%% code id=result\nx + 4\n");
    let before = fs::read(&f.input).unwrap();
    let (port, handle) = server(&f.input, 8);
    let secret = token(port);
    let (_, state) = post(port, "/api/run-next", json!({}), &secret);
    assert_eq!(state["cells"][1]["status"], "current");
    let (_, state) = post(port, "/api/run-next", json!({}), &secret);
    assert_eq!(state["cells"][2]["outputs"][0]["body"], "7");
    let (_, state) = post(
        port,
        "/api/edit",
        json!({"id":"seed","source":"let x = 5"}),
        &secret,
    );
    assert_eq!(state["needsRestart"], true);
    assert_eq!(state["cells"][2]["status"], "stale");
    assert_eq!(post(port, "/api/run-next", json!({}), &secret).0, 409);
    let (_, state) = post(port, "/api/restart", json!({}), &secret);
    assert_eq!(state["cells"][2]["outputs"][0]["body"], "9");
    assert_eq!(
        post(
            port,
            "/api/save",
            json!({"name":"saved.cgnb","overwrite":false}),
            &secret
        )
        .0,
        200
    );
    assert_eq!(fs::read(&f.input).unwrap(), before);
    let saved = parse_notebook_bytes(&fs::read(f.dir.join("saved.cgnb")).unwrap()).unwrap();
    assert_eq!(saved.cells[1].source, "let x = 5");
    assert!(matches!(&saved.cells[2].outputs[0], Output::Text { body, .. } if body == "9"));
    assert!(saved.cells[2].provenance.is_some());
    assert_eq!(
        post(
            port,
            "/api/save",
            json!({"name":"saved.cgnb","overwrite":false}),
            &secret
        )
        .0,
        409
    );
    handle.join().unwrap();
}

#[test]
fn cached_and_current_provenance_are_exposed_after_save_and_reopen() {
    let f = Fixture::new("#!cgnb 1\n%% code id=value\n\"<b>safe</b>\"\n%% output id=value mime=text/plain ordinal=1\nold\n");
    let (port, handle) = server(&f.input, 4);
    let secret = token(port);
    let (_, body) = request(port, "GET", "/api/state", None, None, None, None);
    let state: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(state["cells"][0]["status"], "cached");
    assert!(state["cells"][0]["provenance"].is_null());
    let (_, state) = post(port, "/api/run-cell", json!({"id":"value"}), &secret);
    assert_eq!(state["cells"][0]["status"], "current");
    assert_eq!(state["cells"][0]["provenance"]["schemaVersion"], 2);
    assert_eq!(
        post(
            port,
            "/api/save",
            json!({"name":"saved.cgnb","overwrite":false}),
            &secret
        )
        .0,
        200
    );
    handle.join().unwrap();
    let (port, handle) = server(&f.dir.join("saved.cgnb"), 1);
    let (_, body) = request(port, "GET", "/api/state", None, None, None, None);
    let state: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(state["cells"][0]["status"], "cached");
    assert_eq!(state["cells"][0]["provenance"]["ordinal"], 2);
    handle.join().unwrap();
}

#[test]
fn failed_cell_is_reported_and_saved_text_is_not_current() {
    let f = Fixture::new("#!cgnb 1\n%% code id=bad\nmissing + 1\n%% output id=bad mime=text/plain ordinal=1\n<script>alert(1)</script>\n");
    let (port, handle) = server(&f.input, 5);
    let secret = token(port);
    let (status, state) = post(port, "/api/run-next", json!({}), &secret);
    assert_eq!(status, 409);
    assert!(state["error"].as_str().unwrap().contains("CGNK008"));
    let (status, body) = request(port, "GET", "/api/state", None, None, None, None);
    assert_eq!(status, 200);
    assert!(body.contains("\"failed\""));
    assert!(!body.contains("<script>"));
    let (_, html) = request(port, "GET", "/", None, None, None, None);
    assert!(!html.contains("<script>alert(1)</script>"));
    assert!(html.contains("script defer src=\"/app.js\""));
    assert_eq!(
        post(
            port,
            "/api/save",
            json!({"name":"failed.cgnb","overwrite":false}),
            &secret
        )
        .0,
        200
    );
    handle.join().unwrap();
    let saved = parse_notebook_bytes(&fs::read(f.dir.join("failed.cgnb")).unwrap()).unwrap();
    assert_eq!(
        saved.cells[0].provenance.as_ref().unwrap().result,
        ProvenanceResult::Failed
    );
    let (port, handle) = server(&f.dir.join("failed.cgnb"), 1);
    let (_, body) = request(port, "GET", "/api/state", None, None, None, None);
    let reopened: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(reopened["cells"][0]["status"], "failed");
    handle.join().unwrap();
}

#[test]
fn rejects_cross_origin_token_size_method_json_cell_and_paths_without_mutation() {
    let f = Fixture::new("#!cgnb 1\n%% code id=one\n1\n");
    let before = fs::read(&f.input).unwrap();
    let (port, handle) = server(&f.input, 12);
    let secret = token(port);
    let good_origin = format!("http://127.0.0.1:{port}");
    assert_eq!(
        request(
            port,
            "POST",
            "/api/edit",
            Some(r#"{"id":"one","source":"2"}"#),
            None,
            Some(&good_origin),
            None
        )
        .0,
        403
    );
    assert_eq!(
        request(
            port,
            "POST",
            "/api/edit",
            Some("{}"),
            Some(&secret),
            Some("http://evil.test"),
            None
        )
        .0,
        403
    );
    assert_eq!(
        request(
            port,
            "GET",
            "/api/state",
            None,
            None,
            None,
            Some("evil.test")
        )
        .0,
        403
    );
    assert_eq!(
        request(
            port,
            "POST",
            "/api/edit",
            Some("{"),
            Some(&secret),
            Some(&good_origin),
            None
        )
        .0,
        400
    );
    assert_eq!(
        request(port, "DELETE", "/api/state", None, None, None, None).0,
        405
    );
    assert_eq!(
        request(
            port,
            "POST",
            "/api/unknown",
            Some("{}"),
            Some(&secret),
            Some(&good_origin),
            None
        )
        .0,
        404
    );
    assert_eq!(
        post(
            port,
            "/api/edit",
            json!({"id":"none","source":"2"}),
            &secret
        )
        .0,
        409
    );
    assert_eq!(
        post(
            port,
            "/api/save",
            json!({"name":"../outside.cgnb","overwrite":true}),
            &secret
        )
        .0,
        409
    );
    assert_eq!(
        post(
            port,
            "/api/save",
            json!({"name":"source.cgnb","overwrite":false}),
            &secret
        )
        .0,
        409
    );
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.write_all(format!("POST /api/edit HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 131073\r\n\r\n").as_bytes()).unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    assert!(raw.starts_with("HTTP/1.1 413"));
    let (_, state) = request(port, "GET", "/api/state", None, None, None, None);
    assert_eq!(
        serde_json::from_str::<Value>(&state).unwrap()["cells"][0]["source"],
        "1\n"
    );
    assert_eq!(fs::read(&f.input).unwrap(), before);
    handle.join().unwrap();
}

#[test]
fn cached_asset_reference_is_valid_after_save_as() {
    let f = Fixture::new("#!cgnb 1\n%% code id=one\n1\n");
    let asset = b"image bytes";
    let digest = save_asset(&f.dir.join("source.cgnb.assets"), asset).unwrap();
    fs::write(&f.input, format!("#!cgnb 1\n%% code id=one\n1\n%% asset id=one mime=image/png ordinal=1 sha256={digest}\n")).unwrap();
    let (port, handle) = server(&f.input, 2);
    let secret = token(port);
    assert_eq!(
        post(
            port,
            "/api/save",
            json!({"name":"copy.cgnb","overwrite":false}),
            &secret
        )
        .0,
        200
    );
    let saved = parse_notebook_bytes(&fs::read(f.dir.join("copy.cgnb")).unwrap()).unwrap();
    assert!(
        matches!(&saved.cells[0].outputs[0], Output::Asset { sha256, .. } if sha256 == &digest)
    );
    assert_eq!(
        load_asset(&f.dir.join("copy.cgnb.assets"), &digest).unwrap(),
        asset
    );
    handle.join().unwrap();
}

#[test]
fn portable_open_serves_bounded_image_and_exports_without_sidecars() {
    let f = Fixture::new("#!cgnb 1\n%% markdown id=notes\n# Portable\n%% code id=seed\nlet x = 3\n%% code id=result\nx + 4\n");
    let digest = save_asset(&f.dir.join("source.cgnb.assets"), b"image bytes").unwrap();
    fs::write(
        &f.input,
        format!("#!cgnb 1\n%% markdown id=notes\n# Portable\n%% code id=seed\nlet x = 3\n%% asset id=seed mime=image/png ordinal=1 sha256={digest}\n%% code id=result\nx + 4\n%% output id=result mime=text/plain ordinal=2\n7\n"),
    )
    .unwrap();
    let original = fs::read(&f.input).unwrap();
    let notebook = parse_notebook_bytes(&original).unwrap();
    let portable = f.dir.join("portable.cgnb");
    fs::write(
        &portable,
        cgx2_notebook_format::export_self_contained(&notebook, &f.dir.join("source.cgnb.assets"))
            .unwrap(),
    )
    .unwrap();
    fs::remove_dir_all(f.dir.join("source.cgnb.assets")).unwrap();

    let (port, handle) = server(&portable, 6);
    let secret = token(port);
    let (status, state) = request(port, "GET", "/api/state", None, None, None, None);
    assert_eq!(status, 200);
    let state: Value = serde_json::from_str(&state).unwrap();
    assert_eq!(state["cells"][1]["status"], "cached");
    assert_eq!(state["cells"][1]["outputs"][0]["mime"], "image/png");
    assert_eq!(state["cells"][1]["outputs"][0]["size"], 11);
    let (status, image) = request(
        port,
        "POST",
        "/api/asset",
        Some(&json!({"sha256":digest}).to_string()),
        Some(&secret),
        Some(&format!("http://127.0.0.1:{port}")),
        None,
    );
    assert_eq!(status, 200);
    assert_eq!(image, "image bytes");
    let (status, _) = post(
        port,
        "/api/export",
        json!({"name":"copied.cgnb","overwrite":false}),
        &secret,
    );
    assert_eq!(status, 200);
    let (status, state) = post(port, "/api/restart", json!({}), &secret);
    assert_eq!(status, 200);
    assert_eq!(state["cells"][2]["outputs"][0]["body"], "7");
    let (status, _) = post(
        port,
        "/api/export",
        json!({"name":"fresh.cgnb","overwrite":false}),
        &secret,
    );
    assert_eq!(status, 200);
    handle.join().unwrap();
    let copy = fs::read(f.dir.join("copied.cgnb")).unwrap();
    let reopened =
        cgx2_notebook_format::open_notebook_bytes(&copy, &f.dir.join("missing.assets")).unwrap();
    assert_eq!(reopened.assets[&digest], b"image bytes");
    let fresh = fs::read(f.dir.join("fresh.cgnb")).unwrap();
    let fresh =
        cgx2_notebook_format::open_notebook_bytes(&fresh, &f.dir.join("missing.assets")).unwrap();
    assert!(fresh.assets.is_empty());
    assert_eq!(fs::read(&f.input).unwrap(), original);
    assert!(!f.dir.join("copied.cgnb.assets").exists());
}

#[test]
fn code_result_with_html_is_json_escaped_and_page_uses_local_scripts() {
    let f = Fixture::new("#!cgnb 1\n%% code id=one\n\"<script>alert(1)</script>\"\n");
    let (port, handle) = server(&f.input, 4);
    let secret = token(port);
    let (status, state) = post(port, "/api/run-next", json!({}), &secret);
    assert_eq!(status, 200);
    assert_eq!(
        state["cells"][0]["outputs"][0]["body"],
        "<script>alert(1)</script>"
    );
    let (_, raw) = request(port, "GET", "/api/state", None, None, None, None);
    assert!(!raw.contains("<script>"));
    assert!(raw.contains("\\u003cscript\\u003e"));
    let (_, js) = request(port, "GET", "/app.js", None, None, None, None);
    assert!(js.contains("textContent"));
    assert!(!js.contains("innerHTML"));
    handle.join().unwrap();
}

#[test]
fn selected_rerun_edit_save_reopen_and_clean_restart() {
    let f = Fixture::new("#!cgnb 1\n%% code id=seed\nlet x = 3\n%% code id=result\nx + 4\n");
    let (port, handle) = server(&f.input, 8);
    let secret = token(port);
    let (_, page) = request(port, "GET", "/app.js", None, None, None, None);
    assert!(page.contains("Run this cell"));
    post(port, "/api/restart", json!({}), &secret);
    let (_, state) = post(
        port,
        "/api/edit",
        json!({"id":"seed","source":"let x = 5"}),
        &secret,
    );
    assert_eq!(state["cells"][1]["status"], "stale");
    let (status, state) = post(port, "/api/run-cell", json!({"id":"seed"}), &secret);
    assert_eq!(status, 200);
    assert_eq!(state["cells"][0]["status"], "current");
    assert_eq!(state["cells"][1]["status"], "current");
    let (_, body) = request(port, "GET", "/api/state", None, None, None, None);
    let state: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(state["cells"][1]["outputs"][0]["body"], "9");
    assert_eq!(
        post(
            port,
            "/api/save",
            json!({"name":"saved.cgnb","overwrite":false}),
            &secret
        )
        .0,
        200
    );
    let saved = parse_notebook_bytes(&fs::read(f.dir.join("saved.cgnb")).unwrap()).unwrap();
    assert_eq!(saved.cells[0].source, "let x = 5");
    assert!(matches!(&saved.cells[1].outputs[0], Output::Text { body, .. } if body == "9"));
    let (_, state) = post(port, "/api/restart", json!({}), &secret);
    assert_eq!(state["cells"][1]["outputs"][0]["body"], "9");
    handle.join().unwrap();
    UiSession::open(f.dir.join("saved.cgnb")).unwrap();
}

#[test]
fn selected_rerun_route_rejects_bad_ids_and_missing_guards() {
    let f = Fixture::new("#!cgnb 1\n%% markdown id=notes\n# Hi\n%% code id=one\n\"<b>safe</b>\"\n");
    let (port, handle) = server(&f.input, 8);
    let secret = token(port);
    let origin = format!("http://127.0.0.1:{port}");
    assert_eq!(
        request(
            port,
            "POST",
            "/api/run-cell",
            Some("{\"id\":\"one\"}"),
            None,
            Some(&origin),
            None
        )
        .0,
        403
    );
    assert_eq!(
        request(
            port,
            "POST",
            "/api/run-cell",
            Some("{\"id\":\"one\"}"),
            Some(&secret),
            None,
            None
        )
        .0,
        403
    );
    assert_eq!(
        request(
            port,
            "POST",
            "/api/run-cell",
            Some("{\"id\":\"one\"}"),
            Some(&secret),
            Some("http://evil.test"),
            None
        )
        .0,
        403
    );
    assert_eq!(
        post(port, "/api/run-cell", json!({"id":"../../one"}), &secret).0,
        409
    );
    assert_eq!(
        post(port, "/api/run-cell", json!({"id":"notes"}), &secret).0,
        409
    );
    assert_eq!(
        post(
            port,
            "/api/run-cell",
            json!({"id":"one","source":"1"}),
            &secret
        )
        .0,
        409
    );
    let (status, state) = post(port, "/api/run-cell", json!({"id":"one"}), &secret);
    assert_eq!(status, 200);
    assert_eq!(state["cells"][1]["outputs"][0]["body"], "<b>safe</b>");
    handle.join().unwrap();
}
#[test]
fn selective_ui_rerun_preserves_independent_revision_and_exposes_output_digest() {
    let f = Fixture::new(
        "#!cgnb 1\n%% code id=seed\nlet x = 3\n%% code id=independent\nlet z = 9\n%% code id=result\nx + 4\n",
    );
    let (port, handle) = server(&f.input, 6);
    let secret = token(port);
    let (_, before) = post(port, "/api/restart", json!({}), &secret);
    let revision = before["cells"][1]["provenance"]["ordinal"].clone();
    let digest = before["cells"][1]["provenance"]["prefixSha256"].clone();
    let (_, edited) = post(
        port,
        "/api/edit",
        json!({"id":"seed","source":"let x = 5"}),
        &secret,
    );
    assert_eq!(edited["cells"][1]["status"], "current");
    assert_eq!(edited["cells"][2]["status"], "stale");
    let (status, after) = post(port, "/api/run-cell", json!({"id":"seed"}), &secret);
    assert_eq!(status, 200);
    assert_eq!(after["cells"][2]["outputs"][0]["body"], "9");
    assert_eq!(after["cells"][2]["outputs"][0]["mime"], "text/plain");
    assert_eq!(after["cells"][2]["outputs"][0]["cellId"], "result");
    assert_eq!(
        after["cells"][2]["outputs"][0]["executionRevision"],
        after["cells"][2]["provenance"]["ordinal"]
    );
    assert_eq!(
        after["cells"][2]["outputs"][0]["dependencySha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert_eq!(after["cells"][1]["provenance"]["ordinal"], revision);
    assert_eq!(after["cells"][1]["provenance"]["prefixSha256"], digest);
    let (_, body) = request(port, "GET", "/api/state", None, None, None, None);
    let state: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(state["cells"][1]["status"], "current");
    assert_eq!(
        post(
            port,
            "/api/save",
            json!({"name":"selective.cgnb","overwrite":false}),
            &secret
        )
        .0,
        200
    );
    handle.join().unwrap();
    let saved = parse_notebook_bytes(&fs::read(f.dir.join("selective.cgnb")).unwrap()).unwrap();
    assert_eq!(
        saved.cells[1].provenance.as_ref().unwrap().ordinal,
        revision.as_u64().unwrap()
    );
}

#[cfg(unix)]
#[test]
fn interrupt_stops_active_worker_and_saved_output_is_not_current() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};
    let f = Fixture::new("#!cgnb 1\n%% code id=slow\n1 + 2\n");
    let worker = f.dir.join("sleep-worker");
    fs::write(&worker, "#!/bin/sh\nexec sleep 2\n").unwrap();
    fs::set_permissions(&worker, fs::Permissions::from_mode(0o700)).unwrap();
    let mut session = UiSession::open(f.input.clone()).unwrap();
    session.configure_worker(worker);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = thread::spawn(move || serve_n(listener, session, 6).unwrap());
    let secret = token(port);
    let slow_client = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let secret_for_run = secret.clone();
    let run = thread::spawn(move || post(port, "/api/run-next", json!({}), &secret_for_run));
    thread::sleep(Duration::from_millis(80));
    let start = Instant::now();
    let (interrupt_status, interrupt_reply) = post(port, "/api/interrupt", json!({}), &secret);
    assert_eq!(interrupt_status, 200);
    assert_eq!(interrupt_reply["interruptRequested"], true);
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(run.join().unwrap().0, 409);
    let (_, body) = request(port, "GET", "/api/state", None, None, None, None);
    let state: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(state["cells"][0]["status"], "failed");
    let (status, restarted) = post(port, "/api/restart-only", json!({}), &secret);
    assert_eq!(status, 200);
    assert_eq!(restarted["cells"][0]["status"], "stale");
    drop(slow_client);
    handle.join().unwrap();
}

#[test]
fn slow_client_does_not_hold_up_interrupt_request() {
    use std::time::{Duration, Instant};
    let fixture = Fixture::new("#!cgnb 1\n%% code id=one\n1 + 2\n");
    let (port, handle) = server(&fixture.input, 3);
    let secret = token(port);
    let slow = TcpStream::connect(("127.0.0.1", port)).unwrap();
    thread::sleep(Duration::from_millis(40));
    let start = Instant::now();
    assert_eq!(post(port, "/api/interrupt", json!({}), &secret).0, 409);
    assert!(start.elapsed() < Duration::from_secs(1));
    drop(slow);
    handle.join().unwrap();
}
