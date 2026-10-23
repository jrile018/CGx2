use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub const DEFAULT_BYTES: usize = 1_048_576;
pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(2000);
pub const MAX_TIMEOUT: Duration = Duration::from_secs(60);
pub const MAX_DEPTH: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Unavailable(String),
    Identity(String),
    Limit(String),
    Protocol(String),
    DuplicateRequest,
    Cancelled,
    Closed,
    Io(String),
    /// The request may have performed an effect. Never retry it automatically.
    OutcomeUnknown(String),
}
impl Error {
    pub fn category(&self) -> &'static str {
        match self {
            Self::Unavailable(_) => "unavailable",
            Self::Identity(_) => "identity",
            Self::Limit(_) => "limit",
            Self::Protocol(_) => "protocol",
            Self::DuplicateRequest => "duplicate_request",
            Self::Cancelled => "cancelled",
            Self::Closed => "closed",
            Self::Io(_) => "io",
            Self::OutcomeUnknown(_) => "outcome_unknown",
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub frame_bytes: usize,
    pub state_bytes: usize,
    pub timeout_ms: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            frame_bytes: DEFAULT_BYTES,
            state_bytes: DEFAULT_BYTES,
            timeout_ms: 2000,
        }
    }
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        if self.frame_bytes < 256
            || self.frame_bytes > 16 * DEFAULT_BYTES
            || self.state_bytes == 0
            || self.state_bytes > 16 * DEFAULT_BYTES
            || self.timeout_ms == 0
            || self.timeout_ms > 60_000
        {
            return Err(Error::Limit(
                "frame/state 1..16 MiB (frame >=256); timeout 1..60000 ms".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Confinement {
    Unrestricted,
    Restricted,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CapabilityPolicy {
    pub operations: Vec<String>,
    pub resources: Vec<String>,
    pub required: Confinement,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Enforcement {
    /// No filesystem/network confinement. The mandatory process-group guard still applies.
    Unrestricted,
    LinuxReadOnlyNoNetwork {
        landlock_abi: i32,
    },
}
/// A reviewed, externally pinned file (runtime modules/configuration included).
#[derive(Debug, Clone)]
pub struct Artifact {
    pub path: PathBuf,
    pub sha256: String,
}
#[derive(Debug, Clone)]
pub struct Config {
    pub executable: Artifact,
    pub artifacts: Vec<Artifact>,
    /// Exact bootstrap arguments; script/module files must also be pinned in artifacts.
    pub arguments: Vec<String>,
    /// Exact environment; inherited environment and PATH discovery are disabled.
    pub environment: BTreeMap<String, String>,
    pub environment_sha256: String,
    pub runtime_identity: String,
    pub policy: CapabilityPolicy,
    pub limits: Limits,
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn environment_digest(environment: &BTreeMap<String, String>) -> String {
    digest(&serde_json::to_vec(environment).expect("string map serialization"))
}

/// Copied wire values; adapters must map their own reviewed ForeignValue representation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum WireValue {
    Unit,
    Bool(bool),
    I64(i64),
    U64(u64),
    F32Bits(u32),
    F64Bits(u64),
    String(String),
    /// Canonical RFC4648 base64 with padding. Raw bytes never appear in JSON.
    Bytes(String),
    List(Vec<WireValue>),
    Handle(Handle),
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Handle {
    pub session_id: String,
    pub id: u64,
    pub declared_type: String,
    pub owner: String,
    pub release: String,
}
impl WireValue {
    /// Adapters validate copied/session-owned values on both sides of their boundary.
    pub fn validate_for_session(&self, session: &str, limits: &Limits) -> Result<()> {
        limits.validate()?;
        self.validate(session, 0)?;
        serialize_bounded(self, limits.frame_bytes)?;
        Ok(())
    }
    fn validate(&self, session: &str, depth: usize) -> Result<()> {
        if depth > MAX_DEPTH {
            return Err(Error::Limit("value nesting".into()));
        }
        match self {
            Self::Bytes(s) if !valid_base64(s) => {
                return Err(Error::Protocol("noncanonical base64".into()));
            }
            Self::List(xs) => {
                for x in xs {
                    x.validate(session, depth + 1)?;
                }
                shape(self)?;
            }
            Self::Handle(h)
                if h.session_id != session
                    || h.id == 0
                    || h.declared_type.is_empty()
                    || h.owner.is_empty()
                    || !matches!(h.release.as_str(), "explicit" | "session") =>
            {
                return Err(Error::Identity("invalid session handle".into()));
            }
            _ => {}
        }
        Ok(())
    }
}
#[derive(Clone, PartialEq, Eq)]
enum Shape {
    Any,
    Leaf(std::mem::Discriminant<WireValue>),
    Handle(String, String, String),
    List(Box<Shape>),
}
fn unify(a: &mut Shape, b: Shape) -> Result<()> {
    match (a, b) {
        (a @ Shape::Any, b) => {
            *a = b;
            Ok(())
        }
        (_, Shape::Any) => Ok(()),
        (Shape::List(a), Shape::List(b)) => unify(a, *b),
        (a, b) if *a == b => Ok(()),
        _ => Err(Error::Protocol("heterogeneous list".into())),
    }
}
fn shape(value: &WireValue) -> Result<Shape> {
    match value {
        WireValue::List(values) => {
            let mut element = Shape::Any;
            for value in values {
                unify(&mut element, shape(value)?)?;
            }
            Ok(Shape::List(Box::new(element)))
        }
        WireValue::Handle(h) => Ok(Shape::Handle(
            h.declared_type.clone(),
            h.owner.clone(),
            h.release.clone(),
        )),
        _ => Ok(Shape::Leaf(std::mem::discriminant(value))),
    }
}
fn valid_base64(s: &str) -> bool {
    if !s.len().is_multiple_of(4) {
        return false;
    }
    let bytes = s.as_bytes();
    let padding = bytes.iter().rev().take_while(|&&b| b == b'=').count();
    if padding > 2 {
        return false;
    }
    let data = &bytes[..bytes.len() - padding];
    let val = |b| match b {
        b'A'..=b'Z' => Some(b - b'A'),
        b'a'..=b'z' => Some(b - b'a' + 26),
        b'0'..=b'9' => Some(b - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    if data.iter().any(|&b| val(b).is_none()) {
        return false;
    }
    match padding {
        1 => val(*data.last().unwrap()).unwrap() & 3 == 0,
        2 => val(*data.last().unwrap()).unwrap() & 15 == 0,
        _ => true,
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Hello,
    Invoke,
    Result,
    Error,
    Cancel,
    Cancelled,
    Shutdown,
    Stdout,
    Diagnostic,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub schema_version: u32,
    pub session_id: String,
    pub request_id: u64,
    pub kind: Kind,
    pub payload: serde_json::Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub nonce: String,
    pub runtime_identity: String,
    pub limits: Limits,
    pub policy: CapabilityPolicy,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Invocation {
    pub operation: String,
    pub arguments: Vec<WireValue>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Returned {
    pub value: WireValue,
    pub state_bytes: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ForeignFailure {
    pub code: String,
    pub message: String,
    pub state_bytes: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub text: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub result: std::result::Result<WireValue, ForeignFailure>,
    pub events: Vec<(Kind, String)>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Ready,
    InFlight(u64),
    Poisoned,
    Closed,
}
#[derive(Debug, Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

pub fn envelope<T: Serialize>(session: &str, id: u64, kind: Kind, payload: &T) -> Result<Envelope> {
    Ok(Envelope {
        schema_version: 1,
        session_id: session.into(),
        request_id: id,
        kind,
        payload: serde_json::to_value(payload).map_err(|e| Error::Protocol(e.to_string()))?,
    })
}
pub fn payload<T: serde::de::DeserializeOwned>(e: &Envelope) -> Result<T> {
    serde_json::from_value(e.payload.clone()).map_err(|e| Error::Protocol(e.to_string()))
}
/// Validate depth before serde recursion/allocation. JSON strings cannot affect the depth counter.
fn depth_check(bytes: &[u8]) -> Result<()> {
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    for &b in bytes {
        if quoted {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                quoted = false;
            }
        } else {
            match b {
                b'"' => quoted = true,
                b'[' | b'{' => {
                    depth += 1;
                    if depth > MAX_DEPTH {
                        return Err(Error::Limit("JSON nesting".into()));
                    }
                }
                b']' | b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    Ok(())
}
fn serialize_bounded<T: Serialize>(value: &T, max: usize) -> Result<Vec<u8>> {
    struct Bounded {
        bytes: Vec<u8>,
        max: usize,
    }
    impl std::io::Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.max.saturating_sub(self.bytes.len()) {
                return Err(std::io::Error::other("frame bytes"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut out = Bounded {
        bytes: Vec::new(),
        max,
    };
    serde_json::to_writer(&mut out, value)
        .map_err(|_| Error::Limit("frame serialization".into()))?;
    Ok(out.bytes)
}
pub fn encode(e: &Envelope, limits: &Limits) -> Result<Vec<u8>> {
    limits.validate()?;
    let bytes = serialize_bounded(e, limits.frame_bytes)?;
    depth_check(&bytes)?;
    let mut framed = (bytes.len() as u32).to_be_bytes().to_vec();
    framed.extend(bytes);
    Ok(framed)
}
pub fn decode(bytes: &[u8], limits: &Limits) -> Result<Envelope> {
    limits.validate()?;
    if bytes.len() > limits.frame_bytes {
        return Err(Error::Limit("frame bytes".into()));
    }
    depth_check(bytes)?;
    let unique: UniqueJson =
        serde_json::from_slice(bytes).map_err(|e| Error::Protocol(e.to_string()))?;
    let e: Envelope =
        serde_json::from_value(unique.0).map_err(|e| Error::Protocol(e.to_string()))?;
    if e.schema_version != 1 || e.session_id.is_empty() {
        return Err(Error::Protocol("version/session".into()));
    }
    Ok(e)
}

// serde_json::Value normally accepts repeated object keys. Reject them at every depth.
struct UniqueJson(serde_json::Value);
impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON with unique keys")
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                v: bool,
            ) -> std::result::Result<UniqueJson, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<UniqueJson, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<UniqueJson, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<UniqueJson, E> {
                Err(E::custom("floats require tagged bits"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<UniqueJson, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<UniqueJson, E> {
                Ok(UniqueJson(serde_json::Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<UniqueJson, A::Error> {
                let mut values = Vec::new();
                while let Some(UniqueJson(value)) = a.next_element()? {
                    values.push(value);
                }
                Ok(UniqueJson(values.into()))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<UniqueJson, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = a.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom("duplicate object key"));
                    }
                    let UniqueJson(value) = a.next_value()?;
                    values.insert(key, value);
                }
                Ok(UniqueJson(values.into()))
            }
        }
        d.deserialize_any(Visitor)
    }
}

#[cfg(target_os = "linux")]
mod confinement;
#[cfg(target_os = "linux")]
mod local;
#[cfg(target_os = "linux")]
pub use local::Session;
#[cfg(not(target_os = "linux"))]
pub struct Session;
#[cfg(not(target_os = "linux"))]
impl Session {
    pub fn spawn(_: Config) -> Result<Self> {
        Err(Error::Unavailable("local transport requires Linux".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn value_budget_types_and_session_handles() {
        for encoded in ["", "Zg==", "Zm8=", "Zm9v", "AAH/"] {
            assert!(valid_base64(encoded));
        }
        for encoded in [
            "=", "====", "Zg=", "Zh==", "Zm9=", "a===", "a b=", "abcd====",
        ] {
            assert!(!valid_base64(encoded), "{encoded}");
        }
        assert!(
            WireValue::List(vec![WireValue::Bool(true), WireValue::U64(1)])
                .validate("s", 0)
                .is_err()
        );
        assert!(WireValue::Bytes("Zh==".into()).validate("s", 0).is_err());
        assert!(
            WireValue::Handle(Handle {
                session_id: "other".into(),
                id: 1,
                declared_type: "T".into(),
                owner: "child".into(),
                release: "explicit".into()
            })
            .validate("s", 0)
            .is_err()
        );
    }
    #[test]
    fn nested_empty_lists_do_not_hide_type_conflicts() {
        let value = WireValue::List(vec![
            WireValue::List(vec![]),
            WireValue::List(vec![WireValue::Bool(true)]),
            WireValue::List(vec![WireValue::U64(1)]),
        ]);
        assert!(value.validate_for_session("s", &Limits::default()).is_err());
        let mut deep = WireValue::Unit;
        for _ in 0..33 {
            deep = WireValue::List(vec![deep]);
        }
        assert!(deep.validate_for_session("s", &Limits::default()).is_err());
        assert_eq!(
            Error::OutcomeUnknown("timeout".into()).category(),
            "outcome_unknown"
        );
    }
}
