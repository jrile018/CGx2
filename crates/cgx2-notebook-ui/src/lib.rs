//! Bounded loopback editor for the Wave 5 pure-scalar notebook kernel.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cgx2_notebook_format::{
    export_document, image_mime, markdown_image_digests, open_notebook_bytes, parse_notebook_bytes,
    referenced_asset_digests, save_asset, write_notebook, CellKind, Output, MAX_NOTEBOOK_BYTES,
};
use cgx2_notebook_kernel::{ExecutionControl, NotebookKernel, DEFAULT_CELL_TIMEOUT};
use cgx2_notebook_session::CellStatus;
use serde_json::{json, Value};

const MAX_HEADERS: usize = 16 * 1024;
const MAX_BODY: usize = 128 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_LIFETIME: Duration = Duration::from_secs(8 * 60 * 60);
const MAX_REQUESTS: usize = 10_000;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub struct UiSession {
    input: PathBuf,
    kernel: NotebookKernel,
    assets: BTreeMap<String, Vec<u8>>,
    token: String,
    last_error: Option<String>,
    needs_restart: bool,
    control: Option<ExecutionControl>,
}

impl UiSession {
    fn referenced_assets(&self) -> Result<BTreeMap<String, Vec<u8>>, String> {
        let mut referenced = BTreeMap::new();
        for digest in
            referenced_asset_digests(self.kernel.notebook()).map_err(|error| error.to_string())?
        {
            let bytes = self
                .assets
                .get(&digest)
                .ok_or_else(|| format!("missing verified asset {digest}"))?;
            referenced.insert(digest, bytes.clone());
        }
        Ok(referenced)
    }

    pub fn open(input: PathBuf) -> Result<Self, String> {
        Self::open_with_project(input, None)
    }

    pub fn open_with_project(input: PathBuf, project: Option<&Path>) -> Result<Self, String> {
        if input.extension().is_none_or(|ext| ext != "cgnb") {
            return Err("expected a .cgnb input file".into());
        }
        let mut bytes = Vec::new();
        File::open(&input)
            .and_then(|file| {
                file.take((MAX_NOTEBOOK_BYTES + 1) as u64)
                    .read_to_end(&mut bytes)
            })
            .map_err(|error| format!("cannot read {}: {error}", input.display()))?;
        let document = open_notebook_bytes(&bytes, &sidecar(&input))
            .map_err(|error| format!("invalid {}: {error}", input.display()))?;
        let kernel = NotebookKernel::open_with_project(document.notebook, project)
            .map_err(|error| format!("cannot open notebook kernel: {error}"))?;
        let mut secret = [0u8; 32];
        getrandom::getrandom(&mut secret)
            .map_err(|error| format!("cannot generate request token: {error}"))?;
        let token = secret.iter().map(|byte| format!("{byte:02x}")).collect();
        Ok(Self {
            input,
            kernel,
            assets: document.assets,
            token,
            last_error: None,
            needs_restart: false,
            control: None,
        })
    }

    pub fn configure_worker(&mut self, executable: PathBuf) {
        let control = ExecutionControl::new(executable, DEFAULT_CELL_TIMEOUT);
        self.kernel.set_execution_control(control.clone());
        self.control = Some(control);
    }

    fn clear_interrupt(&self) {
        if let Some(control) = &self.control {
            control.clear_interrupt();
        }
    }

    fn state(&self) -> Value {
        let cells: Vec<Value> = self
            .kernel
            .notebook()
            .cells
            .iter()
            .map(|cell| {
                let status = if cell.kind == CellKind::Markdown {
                    "markdown"
                } else {
                    match self.kernel.status(&cell.id).expect("validated cell ID") {
                        CellStatus::NeverRun => "never-run",
                        CellStatus::Cached => "cached",
                        CellStatus::Current => "current",
                        CellStatus::Stale => "stale",
                        CellStatus::Failed => "failed",
                    }
                };
            let outputs: Vec<Value> = cell.outputs.iter().map(|output| match output {
                Output::Text { mime, ordinal, body } => json!({"kind":"text", "body":body,
                    "mime":mime, "ordinal":ordinal, "cellId":cell.id,
                    "executionRevision":cell.provenance.as_ref().map(|p| p.ordinal),
                    "dependencySha256":cell.provenance.as_ref().map(|p| &p.prefix_sha256)}),
                    Output::Asset { mime, sha256, .. } => json!({"kind":"asset",
                        "body":sha256, "mime":mime,
                        "size":self.assets.get(sha256).map(Vec::len).unwrap_or(0)}),
                }).collect();
                let provenance = cell.provenance.as_ref().map(|p| json!({
                    "schemaVersion":p.schema_version, "sourceSha256":p.source_sha256,
                    "prefixSha256":p.prefix_sha256, "evaluator":p.evaluator,
                    "ordinal":p.ordinal, "outputOrdinal":p.output_ordinal,
                    "result":match p.result {
                        cgx2_notebook_format::ProvenanceResult::Success => "success",
                        cgx2_notebook_format::ProvenanceResult::Failed => "failed",
            }, "stale": p.stale,
            "package": p.package.as_ref().map(|package| json!({
                "manifestSha256":package.manifest_sha256,
                "rootSha256":package.root_sha256,
                "modules":package.modules
            }))}));
                json!({"id":cell.id, "kind":if cell.kind == CellKind::Code {"code"} else {"markdown"},
                    "source":cell.source, "status":status, "outputs":outputs, "provenance":provenance})
            })
            .collect();
        json!({"cells":cells, "needsRestart":self.needs_restart,
            "lastError":self.last_error, "inputName":self.input.file_name().unwrap_or_default().to_string_lossy()})
    }

    fn run_next(&mut self) -> Result<(), String> {
        self.clear_interrupt();
        if self.needs_restart {
            return Err("an edit requires Run this cell or Restart and run all".into());
        }
        let next = self.kernel.next_cell_id().map(str::to_owned);
        let Some(id) = next else {
            return Err("no more unrun code cells".into());
        };
        self.kernel.run_cell(&id).map_err(|error| error.to_string())
    }

    fn run_cell(&mut self, id: &str) -> Result<(), String> {
        self.clear_interrupt();
        let result = self
            .kernel
            .rerun_affected(id)
            .map_err(|error| error.to_string());
        if result.is_ok() {
            self.needs_restart = false;
        } else if self.kernel.status(id).ok() == Some(CellStatus::Failed) {
            self.needs_restart = true;
        }
        result
    }

    fn restart(&mut self) -> Result<(), String> {
        self.clear_interrupt();
        self.needs_restart = false;
        self.kernel.restart().map_err(|error| error.to_string())
    }

    fn restart_and_run_all(&mut self) -> Result<(), String> {
        self.clear_interrupt();
        self.needs_restart = false;
        self.kernel
            .restart_and_run_all()
            .map_err(|error| error.to_string())
    }

    fn edit(&mut self, id: &str, source: &str) -> Result<(), String> {
        self.kernel
            .replace_cell(id, source.to_owned())
            .map_err(|error| error.to_string())?;
        self.needs_restart = true;
        Ok(())
    }

    fn delete(&mut self, id: &str) -> Result<(), String> {
        let was_code = self
            .kernel
            .notebook()
            .cells
            .iter()
            .find(|cell| cell.id == id)
            .is_some_and(|cell| cell.kind == CellKind::Code);
        self.kernel
            .remove_cell(id)
            .map_err(|error| error.to_string())?;
        self.needs_restart |= was_code;
        Ok(())
    }

    fn save(&self, name: &str, overwrite: bool) -> Result<(), String> {
        if !valid_save_name(name) {
            return Err("save name must be a simple .cgnb filename".into());
        }
        let parent = self.input.parent().unwrap_or(Path::new("."));
        let destination = parent.join(name);
        let existing = fs::symlink_metadata(&destination);
        match existing {
            Ok(meta) if meta.file_type().is_symlink() || !meta.is_file() => {
                return Err("save destination must be a regular file".into())
            }
            Ok(_) if !overwrite => {
                return Err("destination exists; check Overwrite to replace it".into())
            }
            Err(error) if error.kind() != io::ErrorKind::NotFound => {
                return Err(format!("cannot inspect save destination: {error}"))
            }
            _ => {}
        }
        let notebook = self.kernel.notebook().clone();
        let text = write_notebook(&notebook)
            .map_err(|error| format!("cannot serialize notebook: {error}"))?;
        parse_notebook_bytes(text.as_bytes())
            .map_err(|error| format!("serialized notebook invalid: {error}"))?;
        atomic_write_with_assets(
            &destination,
            text.as_bytes(),
            &self.referenced_assets()?,
            overwrite,
        )
    }

    fn export(&self, name: &str, overwrite: bool) -> Result<(), String> {
        if !valid_save_name(name) {
            return Err("export name must be a simple .cgnb filename".into());
        }
        let destination = self.input.parent().unwrap_or(Path::new(".")).join(name);
        match fs::symlink_metadata(&destination) {
            Ok(meta) if meta.file_type().is_symlink() || !meta.is_file() => {
                return Err("export destination must be a regular file".into());
            }
            Ok(_) if !overwrite => {
                return Err("destination exists; check Overwrite to replace it".into());
            }
            Err(error) if error.kind() != io::ErrorKind::NotFound => {
                return Err(format!("cannot inspect export destination: {error}"));
            }
            _ => {}
        }
        let text = export_document(self.kernel.notebook(), &self.referenced_assets()?)
            .map_err(|error| format!("cannot export notebook: {error}"))?;
        atomic_write(&destination, text.as_bytes(), overwrite)
    }
}

fn sidecar(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".assets");
    PathBuf::from(name)
}

fn valid_save_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".cgnb") else {
        return false;
    };
    !stem.is_empty()
        && stem != "."
        && stem != ".."
        && stem.len() <= 128
        && stem
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

struct TempPath(PathBuf);
impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

struct TempDirectory(PathBuf);
impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn atomic_write_with_assets(
    path: &Path,
    bytes: &[u8],
    assets: &BTreeMap<String, Vec<u8>>,
    overwrite: bool,
) -> Result<(), String> {
    let target = sidecar(path);
    let existing = match fs::symlink_metadata(&target) {
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
            return Err("save asset destination must be a regular directory".into());
        }
        Ok(_) if !overwrite => {
            return Err("asset directory exists; check Overwrite to replace it".into());
        }
        Ok(_) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(format!("cannot inspect asset destination: {error}")),
    };
    if assets.is_empty() && !existing {
        return atomic_write(path, bytes, overwrite);
    }
    let parent = path.parent().unwrap_or(Path::new("."));
    let transaction = (0..16)
        .find_map(|_| {
            let seq = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let temp = parent.join(format!(".cgx2-ui-assets-{}-{seq}.tmp", std::process::id()));
            match fs::create_dir(&temp) {
                Ok(()) => Some(Ok(TempDirectory(temp))),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(format!("cannot stage asset directory: {error}"))),
            }
        })
        .ok_or_else(|| "could not reserve temporary asset directory".to_owned())??;
    let staged = transaction.0.join("new");
    let backup = transaction.0.join("old");
    if !assets.is_empty() {
        fs::create_dir(&staged).map_err(|error| format!("cannot stage assets: {error}"))?;
        for (digest, data) in assets {
            let actual =
                save_asset(&staged, data).map_err(|error| format!("cannot save asset: {error}"))?;
            if &actual != digest {
                return Err(format!("asset digest changed while saving: {digest}"));
            }
        }
    }
    open_notebook_bytes(bytes, &staged)
        .map_err(|error| format!("cannot validate saved assets: {error}"))?;
    if existing {
        fs::rename(&target, &backup)
            .map_err(|error| format!("cannot stage prior assets: {error}"))?;
    }
    if !assets.is_empty() {
        if let Err(error) = fs::rename(&staged, &target) {
            if existing {
                fs::rename(&backup, &target).map_err(|rollback| {
                    format!("cannot install assets: {error}; rollback failed: {rollback}")
                })?;
            }
            return Err(format!("cannot install assets: {error}"));
        }
    }
    if let Err(error) = atomic_write(path, bytes, overwrite) {
        if !assets.is_empty() {
            fs::rename(&target, &staged)
                .map_err(|rollback| format!("{error}; cannot roll back new assets: {rollback}"))?;
        }
        if existing {
            fs::rename(&backup, &target)
                .map_err(|rollback| format!("{error}; cannot restore prior assets: {rollback}"))?;
        }
        return Err(error);
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8], overwrite: bool) -> Result<(), String> {
    let parent = path.parent().unwrap_or(Path::new("."));
    for _ in 0..16 {
        let seq = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = TempPath(parent.join(format!(".cgx2-ui-{}-{seq}.tmp", std::process::id())));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary.0);
        let mut file = match file {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("cannot create temporary save: {error}")),
        };
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("cannot write notebook: {error}"))?;
        drop(file);
        if overwrite {
            fs::rename(&temporary.0, path)
                .map_err(|error| format!("cannot replace notebook: {error}"))?;
        } else {
            fs::hard_link(&temporary.0, path)
                .map_err(|error| format!("cannot create notebook (possible collision): {error}"))?;
        }
        return Ok(());
    }
    Err("could not reserve temporary save name".into())
}

struct Request {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> Result<Request, (u16, String)> {
    let mut raw = Vec::new();
    let deadline = Instant::now() + IO_TIMEOUT;
    let end = loop {
        if raw.len() >= MAX_HEADERS {
            return Err((431, "request headers too large".into()));
        }
        let mut byte = [0u8];
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err((408, "request timed out".into()));
        }
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|error| (400, error.to_string()))?;
        stream
            .read_exact(&mut byte)
            .map_err(|error| (400, format!("cannot read request: {error}")))?;
        raw.push(byte[0]);
        if raw.ends_with(b"\r\n\r\n") {
            break raw.len();
        }
    };
    let head = std::str::from_utf8(&raw[..end - 4]).map_err(|_| (400, "invalid headers".into()))?;
    let mut lines = head.split("\r\n");
    let request_line = lines.next().ok_or((400, "missing request line".into()))?;
    let words: Vec<_> = request_line.split(' ').collect();
    if words.len() != 3
        || words[2] != "HTTP/1.1"
        || words[1].contains('?')
        || words[1].contains('%')
    {
        return Err((400, "invalid request line".into()));
    }
    let mut headers = BTreeMap::new();
    for line in lines {
        let (name, value) = line.split_once(':').ok_or((400, "invalid header".into()))?;
        if !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err((400, "invalid header name".into()));
        }
        let name = name.to_ascii_lowercase();
        if headers.insert(name, value.trim().to_owned()).is_some() {
            return Err((400, "duplicate header".into()));
        }
    }
    if headers.contains_key("transfer-encoding") {
        return Err((400, "transfer encoding unsupported".into()));
    }
    let length = headers
        .get("content-length")
        .map(|s| s.parse::<usize>())
        .transpose()
        .map_err(|_| (400, "invalid content length".into()))?
        .unwrap_or(0);
    if length > MAX_BODY {
        return Err((413, "request body too large".into()));
    }
    let mut body = vec![0; length];
    let mut offset = 0;
    while offset < length {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err((408, "request timed out".into()));
        }
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|error| (400, error.to_string()))?;
        let read = stream
            .read(&mut body[offset..])
            .map_err(|error| (400, format!("incomplete body: {error}")))?;
        if read == 0 {
            return Err((400, "incomplete body".into()));
        }
        offset += read;
    }
    Ok(Request {
        method: words[0].into(),
        path: words[1].into(),
        headers,
        body,
    })
}

fn json_bytes(value: &Value) -> Vec<u8> {
    serde_json::to_string(value)
        .expect("JSON serialization")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .into_bytes()
}

fn response(status: u16, mime: &str, body: Vec<u8>) -> Vec<u8> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        413 => "Content Too Large",
        431 => "Request Header Fields Too Large",
        _ => "Error",
    };
    let head = format!("HTTP/1.1 {status} {reason}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; img-src blob:; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'\r\nConnection: close\r\n\r\n", body.len());
    [head.as_bytes(), &body].concat()
}

fn error_response(status: u16, message: &str) -> Vec<u8> {
    response(
        status,
        "application/json; charset=utf-8",
        json_bytes(&json!({"error":message})),
    )
}

fn dispatch(session: &mut UiSession, request: Request, port: u16) -> Vec<u8> {
    let expected = format!("127.0.0.1:{port}");
    if request.headers.get("host") != Some(&expected)
        || request
            .headers
            .get("origin")
            .is_some_and(|origin| origin != &format!("http://{expected}"))
    {
        return error_response(403, "invalid Host or Origin");
    }
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => {
            let html = include_str!("page.html").replace("__TOKEN__", &session.token);
            response(200, "text/html; charset=utf-8", html.into_bytes())
        }
        ("GET", "/app.js") => response(
            200,
            "text/javascript; charset=utf-8",
            include_bytes!("app.js").to_vec(),
        ),
        ("GET", "/style.css") => response(
            200,
            "text/css; charset=utf-8",
            include_bytes!("style.css").to_vec(),
        ),
        ("GET", "/api/state") => response(
            200,
            "application/json; charset=utf-8",
            json_bytes(&session.state()),
        ),
        ("POST", "/api/asset") => {
            if request.headers.get("x-cgx2-token") != Some(&session.token)
                || request.headers.get("origin") != Some(&format!("http://{expected}"))
            {
                return error_response(403, "missing or invalid token or Origin");
            }
            if request
                .headers
                .get("content-type")
                .is_none_or(|value| value != "application/json")
            {
                return error_response(400, "expected application/json");
            }
            let value: Value = match serde_json::from_slice(&request.body) {
                Ok(value) => value,
                Err(_) => return error_response(400, "invalid JSON"),
            };
            let Some(digest) = value.get("sha256").and_then(Value::as_str) else {
                return error_response(400, "asset requires sha256");
            };
            if value.as_object().is_none_or(|object| object.len() != 1) {
                return error_response(400, "asset requires only sha256");
            }
            let mime = session
                .kernel
                .notebook()
                .cells
                .iter()
                .flat_map(|cell| &cell.outputs)
                .find_map(|output| match output {
                    Output::Asset { mime, sha256, .. } if sha256 == digest => Some(mime.as_str()),
                    _ => None,
                });
            let Some(bytes) = session.assets.get(digest) else {
                return error_response(404, "unknown asset");
            };
            let markdown_reference = session
                .kernel
                .notebook()
                .cells
                .iter()
                .filter(|cell| cell.kind == CellKind::Markdown)
                .any(|cell| {
                    markdown_image_digests(&cell.source)
                        .is_ok_and(|digests| digests.contains(digest))
                });
            let mime = if markdown_reference {
                image_mime(bytes)
            } else {
                mime
            };
            let Some(mime) = mime else {
                return error_response(404, "unknown asset");
            };
            response(200, mime, bytes.clone())
        }
        (
            "POST",
            "/api/run-next" | "/api/run-cell" | "/api/restart" | "/api/restart-only" | "/api/edit"
            | "/api/delete" | "/api/save" | "/api/export",
        ) => {
            if request.headers.get("x-cgx2-token") != Some(&session.token) {
                return error_response(403, "missing or invalid token");
            }
            if request.headers.get("origin") != Some(&format!("http://{expected}")) {
                return error_response(403, "missing or invalid Origin");
            }
            if request
                .headers
                .get("content-type")
                .is_none_or(|value| value != "application/json")
            {
                return error_response(400, "expected application/json");
            }
            let value: Value = match serde_json::from_slice(&request.body) {
                Ok(value) => value,
                Err(_) => return error_response(400, "invalid JSON"),
            };
            let result = match request.path.as_str() {
                "/api/run-next" if value == json!({}) => session.run_next(),
                "/api/run-cell" => match value.get("id").and_then(Value::as_str) {
                    Some(id) if value.as_object().is_some_and(|obj| obj.len() == 1) => {
                        session.run_cell(id)
                    }
                    _ => Err("run-cell requires one notebook cell ID".into()),
                },
                "/api/restart" if value == json!({}) => session.restart_and_run_all(),
                "/api/restart-only" if value == json!({}) => session.restart(),
                "/api/delete" => match value.get("id").and_then(Value::as_str) {
                    Some(id) if value.as_object().is_some_and(|obj| obj.len() == 1) => {
                        session.delete(id)
                    }
                    _ => Err("delete requires one notebook cell ID".into()),
                },
                "/api/edit" => match (
                    value.get("id").and_then(Value::as_str),
                    value.get("source").and_then(Value::as_str),
                ) {
                    (Some(id), Some(source))
                        if value.as_object().is_some_and(|obj| obj.len() == 2) =>
                    {
                        session.edit(id, source)
                    }
                    _ => Err("edit requires id and source strings".into()),
                },
                "/api/save" => match (
                    value.get("name").and_then(Value::as_str),
                    value.get("overwrite").and_then(Value::as_bool),
                ) {
                    (Some(name), Some(overwrite))
                        if value.as_object().is_some_and(|obj| obj.len() == 2) =>
                    {
                        session.save(name, overwrite)
                    }
                    _ => Err("save requires name and overwrite".into()),
                },
                "/api/export" => match (
                    value.get("name").and_then(Value::as_str),
                    value.get("overwrite").and_then(Value::as_bool),
                ) {
                    (Some(name), Some(overwrite))
                        if value.as_object().is_some_and(|obj| obj.len() == 2) =>
                    {
                        session.export(name, overwrite)
                    }
                    _ => Err("export requires name and overwrite".into()),
                },
                _ => Err("expected an empty JSON object".into()),
            };
            match result {
                Ok(()) => {
                    session.last_error = None;
                    response(
                        200,
                        "application/json; charset=utf-8",
                        json_bytes(&session.state()),
                    )
                }
                Err(message) => {
                    session.last_error = Some(message.clone());
                    error_response(409, &message)
                }
            }
        }
        ("POST", _) => error_response(404, "unknown route"),
        ("GET", _) => error_response(404, "unknown route"),
        _ => error_response(405, "method not allowed"),
    }
}

fn interrupt_response(
    request: &Request,
    port: u16,
    token: &str,
    control: Option<&ExecutionControl>,
) -> Vec<u8> {
    let host = format!("127.0.0.1:{port}");
    if request.headers.get("host") != Some(&host)
        || request.headers.get("origin") != Some(&format!("http://{host}"))
        || request
            .headers
            .get("x-cgx2-token")
            .is_none_or(|value| value != token)
    {
        return error_response(403, "invalid Host, Origin or token");
    }
    if request
        .headers
        .get("content-type")
        .is_none_or(|value| value != "application/json")
        || serde_json::from_slice::<Value>(&request.body).ok() != Some(json!({}))
    {
        return error_response(400, "interrupt requires an empty JSON object");
    }
    let Some(control) = control else {
        return error_response(409, "scalar worker unavailable");
    };
    control.interrupt();
    response(
        200,
        "application/json; charset=utf-8",
        json_bytes(&json!({"interruptRequested":true})),
    )
}

pub fn serve(listener: TcpListener, session: UiSession) -> io::Result<()> {
    let port = listener.local_addr()?.port();
    listener.set_nonblocking(true)?;
    let token = session.token.clone();
    let control = session.control.clone();
    let shared = Arc::new(Mutex::new(session));
    let start = Instant::now();
    let mut requests = 0;
    while start.elapsed() < MAX_LIFETIME && requests < MAX_REQUESTS {
        match listener.accept() {
            Ok((mut stream, _)) => {
                requests += 1;
                stream.set_read_timeout(Some(IO_TIMEOUT))?;
                stream.set_write_timeout(Some(IO_TIMEOUT))?;
                let shared = Arc::clone(&shared);
                let token = token.clone();
                let control = control.clone();
                std::thread::spawn(move || {
                    let request = read_request(&mut stream);
                    let reply = match request {
                        Ok(request)
                            if request.method == "POST" && request.path == "/api/interrupt" =>
                        {
                            interrupt_response(&request, port, &token, control.as_ref())
                        }
                        Ok(request) => match shared.lock() {
                            Ok(mut session) => dispatch(&mut session, request, port),
                            Err(_) => error_response(500, "notebook session unavailable"),
                        },
                        Err((status, message)) => error_response(status, &message),
                    };
                    let _ = stream.write_all(&reply);
                });
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20))
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

pub fn serve_n(listener: TcpListener, session: UiSession, count: usize) -> io::Result<()> {
    let port = listener.local_addr()?.port();
    let token = session.token.clone();
    let control = session.control.clone();
    let shared = Arc::new(Mutex::new(session));
    let mut handlers = Vec::new();
    for _ in 0..count {
        let (mut stream, _) = listener.accept()?;
        stream.set_read_timeout(Some(IO_TIMEOUT))?;
        stream.set_write_timeout(Some(IO_TIMEOUT))?;
        let shared = Arc::clone(&shared);
        let token = token.clone();
        let control = control.clone();
        handlers.push(std::thread::spawn(move || {
            let request = read_request(&mut stream);
            let reply = match request {
                Ok(request) if request.method == "POST" && request.path == "/api/interrupt" => {
                    interrupt_response(&request, port, &token, control.as_ref())
                }
                Ok(request) => match shared.lock() {
                    Ok(mut session) => dispatch(&mut session, request, port),
                    Err(_) => error_response(500, "notebook session unavailable"),
                },
                Err((status, message)) => error_response(status, &message),
            };
            stream.write_all(&reply)
        }));
    }
    for handler in handlers {
        handler
            .join()
            .map_err(|_| io::Error::other("notebook handler panicked"))??;
    }
    Ok(())
}

#[cfg(test)]
mod project_tests {
    use super::*;

    #[test]
    fn project_startup_runs_and_reopens_two_module_notebook() {
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../cgx2-notebook-cli/examples/pure-imports");
        let folder =
            std::env::temp_dir().join(format!("cgx2-ui-pure-imports-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let input = folder.join("input.cgnb");
        fs::copy(project.join("pure-imports.cgnb"), &input).unwrap();
        let mut session = UiSession::open_with_project(input, Some(&project)).unwrap();
        session.restart_and_run_all().unwrap();
        let state = session.state();
        assert_eq!(state["cells"][3]["outputs"][0]["body"], "43");
        assert_eq!(state["cells"][4]["outputs"][0]["body"], "21");
        assert!(state["cells"][4]["provenance"]["package"]["modules"]["demo.beta"].is_string());
        session.save("saved.cgnb", false).unwrap();
        let reopened =
            UiSession::open_with_project(folder.join("saved.cgnb"), Some(&project)).unwrap();
        assert_eq!(reopened.state()["cells"][4]["status"], "cached");
        drop(reopened);
        drop(session);
        fs::remove_dir_all(folder).unwrap();
    }
}
