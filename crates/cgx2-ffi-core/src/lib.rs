//! Portable SDK schema v2. These are owned Rust values, never a C enum layout.
//! Adapters explicitly encode integers, Boolean 0/1 and float bits on their wire.
pub const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Identity,
    Unsupported,
    Limit,
    Layout,
    InvalidUtf8,
    ForeignFailure(i32),
    Cancelled,
    OutcomeUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueType {
    Unit,
    Bool,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32Bits,
    F64Bits,
    Utf8,
    Bytes,
    List(Box<ValueType>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForeignValue {
    Unit,
    /// Only 0 and 1 are valid; no implementation-defined C bool.
    Bool(u8),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    F32Bits(u32),
    F64Bits(u64),
    Utf8(String),
    Bytes(Vec<u8>),
    /// Element type is explicit, including for empty lists. Lists are homogeneous.
    List {
        element: ValueType,
        values: Vec<ForeignValue>,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_bytes: usize,
    pub max_elements: usize,
    pub max_depth: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 1_048_576,
            max_elements: 4096,
            max_depth: 32,
        }
    }
}
impl Limits {
    pub fn validate(self) -> Result<(), Error> {
        if self.max_bytes == 0
            || self.max_bytes > 1_048_576
            || self.max_elements == 0
            || self.max_elements > 4096
            || self.max_depth == 0
            || self.max_depth > 32
        {
            return Err(Error::Limit);
        }
        Ok(())
    }
}

impl ValueType {
    fn validate(&self, depth: usize, maximum: usize) -> Result<(), Error> {
        if depth > maximum {
            return Err(Error::Limit);
        }
        if let Self::List(element) = self {
            element.validate(depth + 1, maximum)?;
        }
        Ok(())
    }
}

impl ForeignValue {
    /// Checks total payload bytes, nodes and type depth, including empty list types.
    /// Every value costs at least one byte, so empty/unit collections are bounded.
    pub fn validate(&self, expected: &ValueType, limits: Limits) -> Result<(), Error> {
        limits.validate()?;
        expected.validate(1, limits.max_depth)?;
        self.visit(expected, limits, 1, &mut 0, &mut 0)
    }

    fn visit(
        &self,
        expected: &ValueType,
        limits: Limits,
        depth: usize,
        bytes: &mut usize,
        elements: &mut usize,
    ) -> Result<(), Error> {
        if depth > limits.max_depth {
            return Err(Error::Limit);
        }
        *elements = elements.checked_add(1).ok_or(Error::Limit)?;
        if *elements > limits.max_elements {
            return Err(Error::Limit);
        }
        let size = match (self, expected) {
            (Self::Unit, ValueType::Unit) => 1,
            (Self::Bool(value), ValueType::Bool) if *value <= 1 => 1,
            (Self::I8(_), ValueType::I8) | (Self::U8(_), ValueType::U8) => 1,
            (Self::I16(_), ValueType::I16) | (Self::U16(_), ValueType::U16) => 2,
            (Self::I32(_), ValueType::I32)
            | (Self::U32(_), ValueType::U32)
            | (Self::F32Bits(_), ValueType::F32Bits) => 4,
            (Self::I64(_), ValueType::I64)
            | (Self::U64(_), ValueType::U64)
            | (Self::F64Bits(_), ValueType::F64Bits) => 8,
            (Self::Utf8(value), ValueType::Utf8) => value.len().max(1),
            (Self::Bytes(value), ValueType::Bytes) => value.len().max(1),
            (Self::List { element, values }, ValueType::List(wanted)) => {
                element.validate(depth + 1, limits.max_depth)?;
                if element != wanted.as_ref() {
                    return Err(Error::Layout);
                }
                if values.len() > limits.max_elements {
                    return Err(Error::Limit);
                }
                for value in values {
                    value.visit(element, limits, depth + 1, bytes, elements)?;
                }
                1
            }
            _ => return Err(Error::Layout),
        };
        *bytes = bytes.checked_add(size).ok_or(Error::Limit)?;
        if *bytes > limits.max_bytes {
            return Err(Error::Limit);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ownership {
    Copied,
    /// A unique native resource. Release consumes it even on nonzero status.
    Owned {
        type_id: String,
        release: String,
    },
    /// Immutable aliases in one adapter/session; the final alias releases once.
    Shared {
        type_id: String,
        release: String,
    },
}

/// Portable worker reference. It is an ID in one live session, never an address.
/// Session lookup and release-once enforcement belong to the receiving adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandleReference {
    pub session_id: u64,
    pub resource_id: u64,
    pub type_id: String,
    pub owner: String,
    pub release_policy: ReferenceRelease,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceRelease {
    ExplicitOrSessionEnd,
    FinalAliasOrSessionEnd,
}
impl HandleReference {
    pub fn validate(&self, session_id: u64, type_id: &str, owner: &str) -> Result<(), Error> {
        if self.session_id == 0
            || self.resource_id == 0
            || self.session_id != session_id
            || self.type_id.is_empty()
            || self.owner.is_empty()
            || self.type_id != type_id
            || self.owner != owner
        {
            return Err(Error::Identity);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Declaration {
    pub schema_version: u32,
    pub package_id: String,
    pub library_id: String,
    pub library_version: String,
    pub artifact_sha256: String,
    pub target_os: String,
    pub target_arch: String,
    pub abi: String,
    pub calling_convention: String,
    pub symbol: String,
    pub parameters: Vec<ValueType>,
    pub result: ValueType,
    pub ownership: Ownership,
    pub error_convention: String,
    pub thread_rule: String,
    pub callbacks: bool,
    pub effects: Vec<String>,
    pub capabilities: Vec<String>,
}
impl Declaration {
    pub fn validate(&self) -> Result<(), Error> {
        if self.schema_version != SCHEMA_VERSION
            || self.calling_convention != "c"
            || self.abi != "c-sdk-v2"
            || self.error_convention != "i32_status_zero_ok"
            || self.thread_rule != "calling_thread"
            || self.callbacks
            || !self.effects.iter().any(|s| s == "foreign")
            || !self.capabilities.iter().any(|s| s == "native")
            || self
                .effects
                .iter()
                .any(|s| !matches!(s.as_str(), "foreign" | "io" | "allocation" | "gpu"))
            || self
                .capabilities
                .iter()
                .any(|s| !matches!(s.as_str(), "native" | "gpu"))
        {
            return Err(Error::Unsupported);
        }
        if self.package_id.is_empty()
            || self.library_id.is_empty()
            || self.library_version.is_empty()
            || self.library_version.split('.').count() != 3
            || self
                .library_version
                .split('.')
                .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()))
            || self.target_os != std::env::consts::OS
            || self.target_arch != std::env::consts::ARCH
            || self.artifact_sha256.len() != 64
            || !self.artifact_sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(Error::Identity);
        }
        if !symbol(&self.symbol) || self.parameters.len() > 32 {
            return Err(Error::Layout);
        }
        for parameter in &self.parameters {
            parameter.validate(1, 32)?;
        }
        self.result.validate(1, 32)?;
        if let Ownership::Owned { type_id, release } | Ownership::Shared { type_id, release } =
            &self.ownership
            && (type_id.is_empty() || !symbol(release))
        {
            return Err(Error::Layout);
        }
        Ok(())
    }
}
pub fn symbol(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
}

/// For Rust facades: catch panics *inside* an extern C entry point. Requires unwind builds.
/// C++ facades must use try/catch; neither language may unwind across the ABI.
pub fn contain_panic(body: impl FnOnce() -> i32) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)).unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn declaration_and_session_reference_fail_closed() {
        let mut declaration = Declaration {
            schema_version: 2,
            package_id: "example".into(),
            library_id: "example".into(),
            library_version: "1.0.0".into(),
            artifact_sha256: "a".repeat(64),
            target_os: std::env::consts::OS.into(),
            target_arch: std::env::consts::ARCH.into(),
            abi: "c-sdk-v2".into(),
            calling_convention: "c".into(),
            symbol: "copy".into(),
            parameters: vec![ValueType::Bytes],
            result: ValueType::Utf8,
            ownership: Ownership::Copied,
            error_convention: "i32_status_zero_ok".into(),
            thread_rule: "calling_thread".into(),
            callbacks: false,
            effects: vec!["foreign".into()],
            capabilities: vec!["native".into()],
        };
        declaration.validate().unwrap();
        declaration.callbacks = true;
        assert_eq!(declaration.validate(), Err(Error::Unsupported));
        declaration.callbacks = false;
        declaration.library_version = "latest".into();
        assert_eq!(declaration.validate(), Err(Error::Identity));
        let reference = HandleReference {
            session_id: 1,
            resource_id: 2,
            type_id: "matrix".into(),
            owner: "worker".into(),
            release_policy: ReferenceRelease::ExplicitOrSessionEnd,
        };
        reference.validate(1, "matrix", "worker").unwrap();
        assert_eq!(
            reference.validate(2, "matrix", "worker"),
            Err(Error::Identity)
        );
        assert_eq!(
            reference.validate(1, "string", "worker"),
            Err(Error::Identity)
        );
    }
    #[test]
    fn rust_facade_catches_before_returning_across_c_and_cleans_up() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static DROPS: AtomicUsize = AtomicUsize::new(0);
        struct Resource;
        impl Drop for Resource {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::Relaxed);
            }
        }
        extern "C" fn entry() -> i32 {
            contain_panic(|| {
                let _resource = Resource;
                panic!("foreign Rust panic");
            })
        }
        assert_eq!(entry(), -1);
        assert_eq!(DROPS.load(Ordering::Relaxed), 1);
    }
    #[test]
    fn portable_values_and_global_limits() {
        assert_eq!(
            ForeignValue::Bool(2).validate(&ValueType::Bool, Limits::default()),
            Err(Error::Layout)
        );
        let value = ForeignValue::List {
            element: ValueType::Bytes,
            values: vec![
                ForeignValue::Bytes(vec![0; 10]),
                ForeignValue::Bytes(vec![0; 10]),
            ],
        };
        assert!(
            value
                .validate(
                    &ValueType::List(Box::new(ValueType::Bytes)),
                    Limits::default()
                )
                .is_ok()
        );
        assert_eq!(
            value.validate(
                &ValueType::List(Box::new(ValueType::Bytes)),
                Limits {
                    max_bytes: 20,
                    ..Limits::default()
                }
            ),
            Err(Error::Limit)
        );
        assert!(
            ForeignValue::F32Bits(0x7fc00001)
                .validate(&ValueType::F32Bits, Limits::default())
                .is_ok()
        );
        let mut nested = ValueType::Unit;
        for _ in 0..33 {
            nested = ValueType::List(Box::new(nested));
        }
        assert_eq!(
            ForeignValue::Unit.validate(&nested, Limits::default()),
            Err(Error::Limit)
        );
        assert_eq!(contain_panic(|| panic!("contained")), -1);
    }
}
