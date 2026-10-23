use crate::ForeignError;
use serde::Deserialize;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::fmt;

pub const MAX_DECLARATION_BYTES: usize = 65_536;

/// A top-level bridge value shape. Containers are checked for their outer type;
/// the bridge checks their nested values and copy budgets recursively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueShape {
    None,
    Bool,
    I64,
    F64,
    F64Array1DCopyV1,
    String,
    Bytes,
    List,
    Tuple,
    Dict,
    PythonHandle,
}

impl ValueShape {
    fn parse(value: &Value, context: &str) -> Result<Self, ForeignError> {
        let name = value
            .as_str()
            .ok_or_else(|| declaration(format!("{context} must be a type name")))?;
        match name {
            "none" => Ok(Self::None),
            "bool" => Ok(Self::Bool),
            "i64" => Ok(Self::I64),
            "f64" => Ok(Self::F64),
            "f64_array_1d_copy_v1" => Ok(Self::F64Array1DCopyV1),
            "string" => Ok(Self::String),
            "bytes" => Ok(Self::Bytes),
            "list" => Ok(Self::List),
            "tuple" => Ok(Self::Tuple),
            "dict" => Ok(Self::Dict),
            "python_handle" => Ok(Self::PythonHandle),
            _ => Err(declaration(format!(
                "{context} has unsupported type {name:?}"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Bool => "bool",
            Self::I64 => "i64",
            Self::F64 => "f64",
            Self::F64Array1DCopyV1 => "f64_array_1d_copy_v1",
            Self::String => "string",
            Self::Bytes => "bytes",
            Self::List => "list",
            Self::Tuple => "tuple",
            Self::Dict => "dict",
            Self::PythonHandle => "python_handle",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    Copied,
    PythonHandle,
}

#[derive(Debug, Clone)]
pub struct CallDeclaration {
    pub(crate) module: String,
    pub(crate) attribute: Vec<String>,
    pub(crate) positional: Vec<ValueShape>,
    pub(crate) keywords: BTreeMap<String, ValueShape>,
    pub(crate) returns: ValueShape,
    pub(crate) ownership: Ownership,
}

impl CallDeclaration {
    pub fn module(&self) -> &str {
        &self.module
    }
    pub fn attribute(&self) -> &[String] {
        &self.attribute
    }
    pub fn positional(&self) -> &[ValueShape] {
        &self.positional
    }
    pub fn keywords(&self) -> &BTreeMap<String, ValueShape> {
        &self.keywords
    }
    pub fn returns(&self) -> ValueShape {
        self.returns
    }
    pub fn ownership(&self) -> &'static str {
        match self.ownership {
            Ownership::Copied => "copied",
            Ownership::PythonHandle => "python_handle",
        }
    }
    pub fn effect(&self) -> &'static str {
        "Foreign"
    }
}

#[derive(Debug, Clone, Default)]
pub struct Registry {
    pub(crate) calls: BTreeMap<String, CallDeclaration>,
}

/// Source-backed allowlist entry. The same strict declaration parser validates it.
pub struct SourceCall<'a> {
    pub key: &'a str,
    pub module: &'a str,
    pub attribute: &'a str,
    pub positional: &'a [ValueShape],
    pub returns: ValueShape,
}

pub fn registry_from_source(calls: &[SourceCall<'_>]) -> Result<Registry, ForeignError> {
    let entries: Vec<Value> = calls.iter().map(|call| json!({
        "key": call.key,
        "module": call.module,
        "attribute": [call.attribute],
        "positional": call.positional.iter().map(|shape| shape.as_str()).collect::<Vec<_>>(),
        "keywords": {},
        "returns": call.returns.as_str(),
        "effect": "Foreign",
        "ownership": "copied",
    })).collect();
    parse_declarations(&json!({"version": 1, "calls": entries}).to_string())
}

impl Registry {
    pub fn get(&self, key: &str) -> Option<&CallDeclaration> {
        self.calls.get(key)
    }
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.calls.keys().map(String::as_str)
    }

    /// Canonical compact JSON with sorted allowlist keys and keyword names.
    pub fn to_json(&self) -> String {
        let calls: Vec<Value> = self.calls.iter().map(|(key, call)| {
            json!({
                "key": key,
                "module": call.module,
                "attribute": call.attribute,
                "positional": call.positional.iter().map(|shape| shape.as_str()).collect::<Vec<_>>(),
                "keywords": call.keywords.iter().map(|(name, shape)| (name.clone(), json!(shape.as_str()))).collect::<BTreeMap<_, _>>(),
                "returns": call.returns.as_str(),
                "effect": "Foreign",
                "ownership": call.ownership(),
            })
        }).collect();
        json!({ "version": 1, "calls": calls }).to_string()
    }
}

pub fn parse_declarations(source: &str) -> Result<Registry, ForeignError> {
    if source.len() > MAX_DECLARATION_BYTES {
        return Err(declaration("declaration exceeds 65536 UTF-8 bytes"));
    }
    let StrictValue(root) = serde_json::from_str(source)
        .map_err(|error| declaration(format!("invalid declaration JSON: {error}")))?;
    let root = object(&root, "root")?;
    fields(root, &["version", "calls"], "root")?;
    if root.get("version").and_then(Value::as_u64) != Some(1) {
        return Err(declaration("schema version must be exactly 1"));
    }
    let calls = root
        .get("calls")
        .and_then(Value::as_array)
        .ok_or_else(|| declaration("calls must be an array"))?;
    let mut registry = Registry::default();
    for (index, value) in calls.iter().enumerate() {
        let context = format!("calls[{index}]");
        let entry = object(value, &context)?;
        fields(
            entry,
            &[
                "key",
                "module",
                "attribute",
                "positional",
                "keywords",
                "returns",
                "effect",
                "ownership",
            ],
            &context,
        )?;
        let key = string(entry, "key", &context)?;
        identifier_path(key, &format!("{context}.key"))?;
        let module = string(entry, "module", &context)?;
        identifier_path(module, &format!("{context}.module"))?;
        let attributes = entry
            .get("attribute")
            .and_then(Value::as_array)
            .ok_or_else(|| declaration(format!("{context}.attribute must be a nonempty array")))?;
        if attributes.is_empty() {
            return Err(declaration(format!(
                "{context}.attribute must be a nonempty array"
            )));
        }
        let mut attribute = Vec::with_capacity(attributes.len());
        for (i, value) in attributes.iter().enumerate() {
            let name = value.as_str().ok_or_else(|| {
                declaration(format!("{context}.attribute[{i}] must be an identifier"))
            })?;
            identifier(name, &format!("{context}.attribute[{i}]"))?;
            attribute.push(name.into());
        }
        if forbidden_dynamic_target(module, &attribute) {
            return Err(declaration(format!(
                "{context} target permits arbitrary Python evaluation or import"
            )));
        }
        let args = entry
            .get("positional")
            .and_then(Value::as_array)
            .ok_or_else(|| declaration(format!("{context}.positional must be an array")))?;
        let positional = args
            .iter()
            .enumerate()
            .map(|(i, value)| ValueShape::parse(value, &format!("{context}.positional[{i}]")))
            .collect::<Result<Vec<_>, _>>()?;
        let names = entry
            .get("keywords")
            .and_then(Value::as_object)
            .ok_or_else(|| declaration(format!("{context}.keywords must be an object")))?;
        let mut keywords = BTreeMap::new();
        for (name, value) in names {
            identifier(name, &format!("{context}.keywords"))?;
            keywords.insert(
                name.clone(),
                ValueShape::parse(value, &format!("{context}.keywords.{name}"))?,
            );
        }
        let returns =
            ValueShape::parse(entry.get("returns").unwrap(), &format!("{context}.returns"))?;
        if string(entry, "effect", &context)? != "Foreign" {
            return Err(declaration(format!("{context}.effect must be Foreign")));
        }
        let ownership = match string(entry, "ownership", &context)? {
            "copied" => Ownership::Copied,
            "python_handle" => Ownership::PythonHandle,
            _ => {
                return Err(declaration(format!(
                    "{context}.ownership must be copied or python_handle"
                )));
            }
        };
        if (returns == ValueShape::PythonHandle) != (ownership == Ownership::PythonHandle) {
            return Err(declaration(format!(
                "{context} return type and ownership disagree"
            )));
        }
        let call = CallDeclaration {
            module: module.into(),
            attribute,
            positional,
            keywords,
            returns,
            ownership,
        };
        if registry.calls.insert(key.into(), call).is_some() {
            return Err(declaration(format!("duplicate allowlist key {key:?}")));
        }
    }
    Ok(registry)
}

fn declaration(message: impl Into<String>) -> ForeignError {
    ForeignError::Declaration(message.into())
}

fn object<'a>(value: &'a Value, context: &str) -> Result<&'a Map<String, Value>, ForeignError> {
    value
        .as_object()
        .ok_or_else(|| declaration(format!("{context} must be an object")))
}

fn fields(
    object: &Map<String, Value>,
    expected: &[&str],
    context: &str,
) -> Result<(), ForeignError> {
    for key in object.keys() {
        if !expected.contains(&key.as_str()) {
            return Err(declaration(format!("unknown field {context}.{key}")));
        }
    }
    for key in expected {
        if !object.contains_key(*key) {
            return Err(declaration(format!("missing field {context}.{key}")));
        }
    }
    Ok(())
}

fn string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    context: &str,
) -> Result<&'a str, ForeignError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| declaration(format!("{context}.{key} must be a string")))
}

fn identifier_path(path: &str, context: &str) -> Result<(), ForeignError> {
    for part in path.split('.') {
        identifier(part, context)?;
    }
    Ok(())
}

fn identifier(name: &str, context: &str) -> Result<(), ForeignError> {
    let mut chars = name.chars();
    if !matches!(chars.next(), Some('a'..='z' | 'A'..='Z' | '_'))
        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(declaration(format!(
            "{context} has invalid Python identifier {name:?}"
        )));
    }
    Ok(())
}

fn forbidden_dynamic_target(module: &str, attribute: &[String]) -> bool {
    match (module, attribute) {
        ("builtins", [name]) => {
            matches!(name.as_str(), "eval" | "exec" | "compile" | "__import__")
        }
        ("importlib", [name]) => name == "import_module",
        ("runpy", [name]) => matches!(name.as_str(), "run_module" | "run_path"),
        _ => false,
    }
}

/// `serde_json::Value` normally keeps the final occurrence of a repeated object
/// field. Reject duplicates at every nesting level before interpreting the schema.
pub(crate) struct StrictValue(pub(crate) Value);

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StrictVisitor;

        impl<'de> Visitor<'de> for StrictVisitor {
            type Value = StrictValue;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a JSON value without duplicate object fields")
            }

            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }

            fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }

            fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Bool(value)))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Number(value.into())))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Number(value.into())))
            }

            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                let number = serde_json::Number::from_f64(value)
                    .ok_or_else(|| E::custom("nonfinite JSON number"))?;
                Ok(StrictValue(Value::Number(number)))
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::String(value.into())))
            }

            fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::String(value)))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(StrictValue(value)) = seq.next_element()? {
                    values.push(value);
                }
                Ok(StrictValue(Value::Array(values)))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut values = Map::new();
                while let Some((key, StrictValue(value))) =
                    map.next_entry::<String, StrictValue>()?
                {
                    if values.insert(key.clone(), value).is_some() {
                        return Err(<A::Error as de::Error>::custom(format!(
                            "duplicate JSON field {key:?}"
                        )));
                    }
                }
                Ok(StrictValue(Value::Object(values)))
            }
        }

        deserializer.deserialize_any(StrictVisitor)
    }
}
