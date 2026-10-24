//! Checked payloads and bounded, traced chunk storage. No host object table.
use crate::{
    AllocationError, Handle, MemoryError, RegionId, Session, MAX_OBJECT_PAYLOAD_BYTES,
    MAX_OBJECT_REFERENCE_SLOTS,
};

pub const MAX_GRAPH_REFERENCES: usize = 65_536;
pub const MAX_GRAPH_PAYLOAD_BYTES: usize = 1_048_576;
const HEADER: usize = 24;
const CHILDREN: usize = MAX_OBJECT_REFERENCE_SLOTS - 1;
const PAYLOAD: usize = MAX_OBJECT_PAYLOAD_BYTES - HEADER;
const MAGIC: &[u8; 4] = b"CGG1";

/// Additive error envelope; existing exhaustive MemoryError stays unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadError {
    Memory(MemoryError),
    OutOfBounds,
}

impl From<MemoryError> for PayloadError {
    fn from(value: MemoryError) -> Self {
        Self::Memory(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphError {
    Memory(MemoryError),
    ShapeLimit,
    InvalidLayout,
    OutOfBounds,
}

impl From<MemoryError> for GraphError {
    fn from(value: MemoryError) -> Self {
        Self::Memory(value)
    }
}

impl From<AllocationError> for GraphError {
    fn from(value: AllocationError) -> Self {
        Self::Memory(MemoryError::Allocation(value))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphShape {
    pub payload_bytes: usize,
    pub references: usize,
}

impl Session {
    /// Overwrites an existing byte range without allocation or collection.
    /// All validation precedes mutation, including checked offset arithmetic.
    pub fn write_bytes(
        &mut self,
        handle: Handle,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), PayloadError> {
        let object = self.object_mut(handle)?;
        let end = offset
            .checked_add(bytes.len())
            .ok_or(PayloadError::OutOfBounds)?;
        let target = object
            .bytes
            .get_mut(offset..end)
            .ok_or(PayloadError::OutOfBounds)?;
        target.copy_from_slice(bytes);
        Ok(())
    }

    /// Initializes an ordinary object with evaluated payload and children.
    /// Children are validated and rooted before allocation can collect.
    /// The returned handle is unrooted, following alloc_object's contract.
    pub fn alloc_initialized_object(
        &mut self,
        region: RegionId,
        payload: &[u8],
        references: &[Option<Handle>],
    ) -> Result<Handle, GraphError> {
        self.mode(region)?;
        if payload.len() > MAX_OBJECT_PAYLOAD_BYTES || references.len() > MAX_OBJECT_REFERENCE_SLOTS
        {
            return Err(GraphError::ShapeLimit);
        }
        self.validate_children(region, references)?;
        let mut roots = self.scoped_roots();
        roots.root_many(references)?;
        let result = (|| {
            let handle = roots.alloc_object(region, payload.len(), references.len())?;
            // Allocation has established exactly this payload and slot layout.
            roots.object_mut(handle)?.bytes.copy_from_slice(payload);
            for (index, child) in references.iter().enumerate() {
                roots.set_reference(handle, index, *child)?;
            }
            Ok(handle)
        })();
        let outcome = roots.finish(result);
        match (outcome.result, outcome.cleanup.first_error) {
            (Err(error), _) => Err(error),
            (Ok(_), Some(error)) => Err(error.into()),
            (Ok(handle), None) => Ok(handle),
        }
    }

    fn validate_children(
        &self,
        region: RegionId,
        references: &[Option<Handle>],
    ) -> Result<(), GraphError> {
        for child in references.iter().flatten() {
            self.read_bytes(*child)?;
            if child.region() != region {
                return Err(MemoryError::CrossRegionEdge.into());
            }
        }
        Ok(())
    }

    /// Constructs a traced chain with up to 65,536 logical reference slots and
    /// 1 MiB logical bytes. Each chunk respects alloc_object's existing bounds.
    /// Slot zero links chunks; remaining slots hold logical graph children.
    /// Header fields are fixed little-endian u32 values, never host pointers.
    /// Earlier successful allocations are not rolled back on failure.
    pub fn alloc_graph(
        &mut self,
        region: RegionId,
        payload: &[u8],
        references: &[Option<Handle>],
    ) -> Result<Handle, GraphError> {
        self.alloc_graph_layout(
            region,
            payload.len(),
            references.len(),
            Some(payload),
            references,
        )
    }

    /// Zero-initialized graph storage without temporary host buffers.
    pub fn alloc_zeroed_graph(
        &mut self,
        region: RegionId,
        payload_bytes: usize,
        references: usize,
    ) -> Result<Handle, GraphError> {
        self.alloc_graph_layout(region, payload_bytes, references, None, &[])
    }

    fn alloc_graph_layout(
        &mut self,
        region: RegionId,
        payload_len: usize,
        reference_len: usize,
        payload: Option<&[u8]>,
        references: &[Option<Handle>],
    ) -> Result<Handle, GraphError> {
        self.mode(region)?;
        if payload_len > MAX_GRAPH_PAYLOAD_BYTES || reference_len > MAX_GRAPH_REFERENCES {
            return Err(GraphError::ShapeLimit);
        }
        self.validate_children(region, references)?;
        let mut roots = self.scoped_roots();
        roots.root_many(references)?;
        let result = (|| {
            let chunks = reference_len
                .div_ceil(CHILDREN)
                .max(payload_len.div_ceil(PAYLOAD))
                .max(1);
            let mut next = None;
            for chunk in (0..chunks).rev() {
                let ref_start = (chunk * CHILDREN).min(reference_len);
                let ref_end = (ref_start + CHILDREN).min(reference_len);
                let byte_start = (chunk * PAYLOAD).min(payload_len);
                let byte_end = (byte_start + PAYLOAD).min(payload_len);
                let handle = roots.alloc_object(
                    region,
                    HEADER + byte_end - byte_start,
                    1 + ref_end - ref_start,
                )?;
                roots.root(handle)?;
                let bytes = &mut roots.object_mut(handle)?.bytes;
                bytes[..4].copy_from_slice(MAGIC);
                for (offset, value) in [
                    reference_len,
                    payload_len,
                    ref_end - ref_start,
                    byte_end - byte_start,
                ]
                .into_iter()
                .enumerate()
                {
                    bytes[4 + offset * 4..8 + offset * 4]
                        .copy_from_slice(&(value as u32).to_le_bytes());
                }
                if let Some(payload) = payload {
                    bytes[HEADER..].copy_from_slice(&payload[byte_start..byte_end]);
                }
                roots.set_reference(handle, 0, next)?;
                if !references.is_empty() {
                    for (index, child) in references[ref_start..ref_end].iter().enumerate() {
                        roots.set_reference(handle, index + 1, *child)?;
                    }
                }
                next = Some(handle);
            }
            Ok(next.expect("at least one chunk"))
        })();
        let outcome = roots.finish(result);
        match (outcome.result, outcome.cleanup.first_error) {
            (Err(error), _) => Err(error),
            (Ok(_), Some(error)) => Err(error.into()),
            (Ok(handle), None) => Ok(handle),
        }
    }

    fn chunk_shape(&self, handle: Handle) -> Result<(GraphShape, usize, usize), GraphError> {
        let bytes = self.read_bytes(handle)?;
        if bytes.len() < HEADER || &bytes[..4] != MAGIC || bytes[20..HEADER] != [0; 4] {
            return Err(GraphError::InvalidLayout);
        }
        let field = |index| {
            u32::from_le_bytes(bytes[index..index + 4].try_into().expect("header field")) as usize
        };
        let shape = GraphShape {
            references: field(4),
            payload_bytes: field(8),
        };
        let refs = field(12);
        let payload = field(16);
        if shape.references > MAX_GRAPH_REFERENCES
            || shape.payload_bytes > MAX_GRAPH_PAYLOAD_BYTES
            || refs > CHILDREN
            || payload > PAYLOAD
            || bytes.len() != HEADER + payload
            || self.region(handle.region())?.slots[handle.slot]
                .object
                .as_ref()
                .and_then(|object| object.references.as_ref())
                .map(Vec::len)
                != Some(1 + refs)
        {
            return Err(GraphError::InvalidLayout);
        }
        Ok((shape, refs, payload))
    }

    /// Validates the entire bounded chain, including shape, length and links.
    pub fn graph_shape(&self, handle: Handle) -> Result<GraphShape, GraphError> {
        let shape = self.chunk_shape(handle)?.0;
        let chunks = shape
            .references
            .div_ceil(CHILDREN)
            .max(shape.payload_bytes.div_ceil(PAYLOAD))
            .max(1);
        let mut next = Some(handle);
        for index in 0..chunks {
            let current = next.ok_or(GraphError::InvalidLayout)?;
            let (actual, refs, bytes) = self.chunk_shape(current)?;
            if actual != shape
                || refs
                    != shape
                        .references
                        .saturating_sub(index * CHILDREN)
                        .min(CHILDREN)
                || bytes
                    != shape
                        .payload_bytes
                        .saturating_sub(index * PAYLOAD)
                        .min(PAYLOAD)
            {
                return Err(GraphError::InvalidLayout);
            }
            next = self.reference(current, 0)?;
        }
        if next.is_some() {
            return Err(GraphError::InvalidLayout);
        }
        Ok(shape)
    }

    fn graph_slot(&self, handle: Handle, index: usize) -> Result<(Handle, usize), GraphError> {
        if index >= self.graph_shape(handle)?.references {
            return Err(GraphError::OutOfBounds);
        }
        let mut chunk = handle;
        for _ in 0..index / CHILDREN {
            chunk = self.reference(chunk, 0)?.ok_or(GraphError::InvalidLayout)?;
        }
        Ok((chunk, 1 + index % CHILDREN))
    }

    pub fn graph_reference(
        &self,
        handle: Handle,
        index: usize,
    ) -> Result<Option<Handle>, GraphError> {
        let (chunk, slot) = self.graph_slot(handle, index)?;
        Ok(self.reference(chunk, slot)?)
    }

    /// Visits logical child slots in source order after validating the whole
    /// chain. Callback storage belongs to the caller; this never allocates.
    pub fn visit_graph_references(
        &self,
        handle: Handle,
        mut visit: impl FnMut(usize, Option<Handle>),
    ) -> Result<(), GraphError> {
        let shape = self.graph_shape(handle)?;
        let mut chunk = handle;
        let mut index = 0;
        while index < shape.references {
            let count = (shape.references - index).min(CHILDREN);
            for slot in 0..count {
                visit(index + slot, self.reference(chunk, slot + 1)?);
            }
            index += count;
            if index < shape.references {
                chunk = self.reference(chunk, 0)?.ok_or(GraphError::InvalidLayout)?;
            }
        }
        Ok(())
    }

    /// In-place checked edge update; supports same-region aliases and cycles.
    pub fn set_graph_reference(
        &mut self,
        handle: Handle,
        index: usize,
        child: Option<Handle>,
    ) -> Result<(), GraphError> {
        let (chunk, slot) = self.graph_slot(handle, index)?;
        Ok(self.set_reference(chunk, slot, child)?)
    }

    /// Copies a logical byte range into caller-owned storage without allocation.
    /// Layout and complete range validation precede the first output write.
    pub fn read_graph_bytes(
        &self,
        handle: Handle,
        offset: usize,
        output: &mut [u8],
    ) -> Result<(), GraphError> {
        let shape = self.graph_shape(handle)?;
        if offset
            .checked_add(output.len())
            .is_none_or(|end| end > shape.payload_bytes)
        {
            return Err(GraphError::OutOfBounds);
        }
        let mut chunk = handle;
        let mut remaining_offset = offset;
        let mut written = 0;
        while written < output.len() {
            let bytes = &self.read_bytes(chunk)?[HEADER..];
            if remaining_offset < bytes.len() {
                let count = (bytes.len() - remaining_offset).min(output.len() - written);
                output[written..written + count]
                    .copy_from_slice(&bytes[remaining_offset..remaining_offset + count]);
                written += count;
                remaining_offset = 0;
            } else {
                remaining_offset -= bytes.len();
            }
            if written < output.len() {
                chunk = self.reference(chunk, 0)?.ok_or(GraphError::InvalidLayout)?;
            }
        }
        Ok(())
    }
    /// Writes logical payload bytes after validating the complete layout/range.
    /// Rejected writes leave the graph unchanged; no reserve or collection occurs.
    pub fn write_graph_bytes(
        &mut self,
        handle: Handle,
        offset: usize,
        input: &[u8],
    ) -> Result<(), GraphError> {
        let shape = self.graph_shape(handle)?;
        if offset
            .checked_add(input.len())
            .is_none_or(|end| end > shape.payload_bytes)
        {
            return Err(GraphError::OutOfBounds);
        }
        let mut chunk = handle;
        let mut remaining_offset = offset;
        let mut written = 0;
        while written < input.len() {
            let bytes = &mut self.object_mut(chunk)?.bytes[HEADER..];
            if remaining_offset < bytes.len() {
                let count = (bytes.len() - remaining_offset).min(input.len() - written);
                bytes[remaining_offset..remaining_offset + count]
                    .copy_from_slice(&input[written..written + count]);
                written += count;
                remaining_offset = 0;
            } else {
                remaining_offset -= bytes.len();
            }
            if written < input.len() {
                chunk = self.reference(chunk, 0)?.ok_or(GraphError::InvalidLayout)?;
            }
        }
        Ok(())
    }
}

/// Additive source-managed model. Kept in the frozen nine-file runtime inventory
/// so previously emitted standalone programs still stage a complete runtime.
pub mod default_managed {
    pub mod model {
        //! Additive closed execution model; compiler authentication is separate from runtime validation.
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        pub struct Loc {
            pub start: usize,
            pub end: usize,
        }
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        pub struct MemoryEffectsV1 {
            pub may_allocate_managed: bool,
            pub may_auto_collect_at_allocation: bool,
            pub may_explicitly_collect: bool,
            pub may_allocate_root_bookkeeping: bool,
        }
        impl MemoryEffectsV1 {
            pub fn union(self, other: Self) -> Self {
                Self {
                    may_allocate_managed: self.may_allocate_managed || other.may_allocate_managed,
                    may_auto_collect_at_allocation: self.may_auto_collect_at_allocation
                        || other.may_auto_collect_at_allocation,
                    may_explicitly_collect: self.may_explicitly_collect
                        || other.may_explicitly_collect,
                    may_allocate_root_bookkeeping: self.may_allocate_root_bookkeeping
                        || other.may_allocate_root_bookkeeping,
                }
            }
        }
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub enum Shape {
            KInt,
            KBool,
            KUnit,
            KString,
            KError,
            KList(usize),
            KArray(usize),
            KRef(usize),
            KRecord(Vec<(usize, usize)>),
            KVariant(Vec<(usize, Vec<usize>)>),
        }
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct Layout {
            pub name: usize,
            pub shape: Shape,
        }
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub enum Op {
            EInt(i64),
            EBool(bool),
            EUnit,
            EString(usize),
            ELocal(usize),
            EField(usize, usize),
            EBinary(usize, usize, usize),
            ERecord(Vec<(usize, usize)>),
            ECall(usize, Vec<usize>),
            EConstruct(usize, Vec<usize>),
            EData(usize, Vec<usize>),
            ETry(usize),
            ERef(usize),
            EGet(usize),
            ESet(usize, usize),
            ECollect,
        }
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct Expression {
            pub ty: usize,
            pub loc: Loc,
            pub op: Op,
        }
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct Arm {
            pub case: Option<usize>,
            pub bindings: Vec<usize>,
            pub body: Vec<Statement>,
        }
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub enum Statement {
            SLet(usize, usize),
            SAssign(usize, usize, usize),
            SExpr(usize),
            SReturn(Option<usize>),
            SIf(usize, Vec<Statement>, Vec<Statement>),
            SMatch(usize, Vec<Arm>),
            SRange(usize, usize, usize, Vec<Statement>),
            SFor(usize, usize, Vec<Statement>),
            SGuard(usize, Loc, Vec<Statement>),
        }
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct Function {
            pub name: usize,
            pub parameters: Vec<usize>,
            pub parameter_types: Vec<usize>,
            pub result: usize,
            pub behavioral_effects: Vec<usize>,
            pub capabilities: Vec<usize>,
            pub requires: Vec<usize>,
            pub ensures: Vec<usize>,
            pub result_name: usize,
            pub body: Vec<Statement>,
            pub effects: MemoryEffectsV1,
            pub loc: Loc,
        }
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct RootMapV1 {
            pub expression: usize,
            pub operands: Vec<usize>,
            pub may_allocate_managed: bool,
            pub root_result: bool,
        }
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct Program {
            pub root_maps: Vec<RootMapV1>,
            pub strings: Vec<String>,
            pub layouts: Vec<Layout>,
            pub expressions: Vec<Expression>,
            pub functions: Vec<Function>,
            pub entry: usize,
            pub schema_version: u32,
            pub output_loc: Loc,
        }
        impl Program {
            /// All non-string vectors contain closed, numeric model data. Debug is valid
            /// Rust after adding Vec constructors; string pool is quoted independently.
            pub fn rust(&self) -> String {
                if self.expressions.len() > 2048 {
                    let hex = self
                        .wire_v1()
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>();
                    return format!("Program::decode_hex_v1({hex:?}).unwrap_or_else(|error|{{eprintln!(\"CGR090: managed model initialization failed: {{error}} at {}..{}\");std::process::exit(1)}})",self.output_loc.start,self.output_loc.end);
                }
                fn data<T: std::fmt::Debug>(value: &T) -> String {
                    format!("{value:?}").replace('[', "vec![")
                }
                format!("Program{{strings:vec![{}],layouts:{},expressions:{},functions:{},entry:{},schema_version:{},root_maps:{},output_loc:{:?}}}",
            self.strings.iter().map(|s|format!("{s:?}.into()")).collect::<Vec<_>>().join(","),
            data(&self.layouts),data(&self.expressions),data(&self.functions),self.entry,self.schema_version,data(&self.root_maps),self.output_loc)
            }
        }
    }
    /// Closed versioned metadata encoding, not an execution acceptance flag.
    /// Large generated programs use one hex literal rather than hundreds of
    /// thousands of Rust struct literals. All executable source is checked first.
    mod wire {
        use super::model::*;
        type Result<T> = std::result::Result<T, &'static str>;
        struct Writer(Vec<u8>);
        impl Writer {
            fn n(&mut self, mut n: u64) {
                while n >= 128 {
                    self.0.push((n as u8 & 127) | 128);
                    n >>= 7;
                }
                self.0.push(n as u8);
            }
            fn u(&mut self, n: usize) {
                self.n(n as u64)
            }
            fn b(&mut self, b: bool) {
                self.0.push(u8::from(b))
            }
            fn ids(&mut self, v: &[usize]) {
                self.u(v.len());
                for x in v {
                    self.u(*x);
                }
            }
            fn loc(&mut self, l: Loc) {
                self.u(l.start);
                self.u(l.end);
            }
            fn effects(&mut self, e: MemoryEffectsV1) {
                self.b(e.may_allocate_managed);
                self.b(e.may_auto_collect_at_allocation);
                self.b(e.may_explicitly_collect);
                self.b(e.may_allocate_root_bookkeeping);
            }
            fn option(&mut self, v: Option<usize>) {
                self.b(v.is_some());
                if let Some(v) = v {
                    self.u(v);
                }
            }
            fn shape(&mut self, s: &Shape) {
                match s {
                    Shape::KInt => self.u(0),
                    Shape::KBool => self.u(1),
                    Shape::KUnit => self.u(2),
                    Shape::KString => self.u(3),
                    Shape::KError => self.u(4),
                    Shape::KList(t) => {
                        self.u(5);
                        self.u(*t)
                    }
                    Shape::KArray(t) => {
                        self.u(6);
                        self.u(*t)
                    }
                    Shape::KRef(t) => {
                        self.u(7);
                        self.u(*t)
                    }
                    Shape::KRecord(fs) => {
                        self.u(8);
                        self.u(fs.len());
                        for (n, t) in fs {
                            self.u(*n);
                            self.u(*t);
                        }
                    }
                    Shape::KVariant(cs) => {
                        self.u(9);
                        self.u(cs.len());
                        for (n, ts) in cs {
                            self.u(*n);
                            self.ids(ts);
                        }
                    }
                }
            }
            fn op(&mut self, op: &Op) {
                match op {
                    Op::EInt(n) => {
                        self.u(0);
                        self.n(((*n as u64) << 1) ^ ((*n >> 63) as u64));
                    }
                    Op::EBool(b) => {
                        self.u(1);
                        self.b(*b)
                    }
                    Op::EUnit => self.u(2),
                    Op::EString(n) => {
                        self.u(3);
                        self.u(*n)
                    }
                    Op::ELocal(n) => {
                        self.u(4);
                        self.u(*n)
                    }
                    Op::EField(e, f) => {
                        self.u(5);
                        self.u(*e);
                        self.u(*f)
                    }
                    Op::EBinary(a, o, b) => {
                        self.u(6);
                        self.u(*a);
                        self.u(*o);
                        self.u(*b)
                    }
                    Op::ECall(f, es) => {
                        self.u(7);
                        self.u(*f);
                        self.ids(es)
                    }
                    Op::EConstruct(c, es) => {
                        self.u(8);
                        self.u(*c);
                        self.ids(es)
                    }
                    Op::EData(o, es) => {
                        self.u(9);
                        self.u(*o);
                        self.ids(es)
                    }
                    Op::ETry(e) => {
                        self.u(10);
                        self.u(*e)
                    }
                    Op::ERef(e) => {
                        self.u(11);
                        self.u(*e)
                    }
                    Op::EGet(e) => {
                        self.u(12);
                        self.u(*e)
                    }
                    Op::ESet(a, b) => {
                        self.u(13);
                        self.u(*a);
                        self.u(*b)
                    }
                    Op::ECollect => self.u(14),
                    Op::ERecord(fs) => {
                        self.u(15);
                        self.u(fs.len());
                        for (f, e) in fs {
                            self.u(*f);
                            self.u(*e);
                        }
                    }
                }
            }
            fn body(&mut self, body: &[Statement]) {
                self.u(body.len());
                for s in body {
                    match s {
                        Statement::SLet(n, e) => {
                            self.u(0);
                            self.u(*n);
                            self.u(*e)
                        }
                        Statement::SAssign(n, o, e) => {
                            self.u(1);
                            self.u(*n);
                            self.u(*o);
                            self.u(*e)
                        }
                        Statement::SExpr(e) => {
                            self.u(2);
                            self.u(*e)
                        }
                        Statement::SReturn(e) => {
                            self.u(3);
                            self.option(*e)
                        }
                        Statement::SIf(e, a, b) => {
                            self.u(4);
                            self.u(*e);
                            self.body(a);
                            self.body(b)
                        }
                        Statement::SMatch(e, arms) => {
                            self.u(5);
                            self.u(*e);
                            self.u(arms.len());
                            for a in arms {
                                self.option(a.case);
                                self.ids(&a.bindings);
                                self.body(&a.body)
                            }
                        }
                        Statement::SFor(n, e, b) => {
                            self.u(6);
                            self.u(*n);
                            self.u(*e);
                            self.body(b)
                        }
                        Statement::SGuard(n, l, b) => {
                            self.u(7);
                            self.u(*n);
                            self.loc(*l);
                            self.body(b)
                        }
                        Statement::SRange(n, a, b, body) => {
                            self.u(8);
                            self.u(*n);
                            self.u(*a);
                            self.u(*b);
                            self.body(body)
                        }
                    }
                }
            }
        }
        struct Reader<'a> {
            bytes: &'a [u8],
            at: usize,
        }
        impl Reader<'_> {
            fn byte(&mut self) -> Result<u8> {
                let b = *self.bytes.get(self.at).ok_or("truncated model")?;
                self.at += 1;
                Ok(b)
            }
            fn n(&mut self) -> Result<u64> {
                let mut n = 0u64;
                for shift in (0..=63).step_by(7) {
                    let b = self.byte()?;
                    if shift == 63 && b > 1 {
                        return Err("model integer overflow");
                    }
                    n |= u64::from(b & 127) << shift;
                    if b & 128 == 0 {
                        return Ok(n);
                    }
                }
                Err("model integer overflow")
            }
            fn u(&mut self) -> Result<usize> {
                usize::try_from(self.n()?).map_err(|_| "model target integer overflow")
            }
            fn b(&mut self) -> Result<bool> {
                match self.byte()? {
                    0 => Ok(false),
                    1 => Ok(true),
                    _ => Err("invalid model boolean"),
                }
            }
            fn vec<T>(&mut self, mut read: impl FnMut(&mut Self) -> Result<T>) -> Result<Vec<T>> {
                let n = self.u()?;
                if n > self.bytes.len().saturating_sub(self.at) {
                    return Err("model vector envelope");
                }
                let mut v = Vec::new();
                v.try_reserve_exact(n)
                    .map_err(|_| "model host allocation failed")?;
                for _ in 0..n {
                    v.push(read(self)?);
                }
                Ok(v)
            }
            fn ids(&mut self) -> Result<Vec<usize>> {
                self.vec(Self::u)
            }
            fn loc(&mut self) -> Result<Loc> {
                let loc = Loc {
                    start: self.u()?,
                    end: self.u()?,
                };
                if loc.start > loc.end {
                    return Err("invalid model source span");
                }
                Ok(loc)
            }
            fn effects(&mut self) -> Result<MemoryEffectsV1> {
                Ok(MemoryEffectsV1 {
                    may_allocate_managed: self.b()?,
                    may_auto_collect_at_allocation: self.b()?,
                    may_explicitly_collect: self.b()?,
                    may_allocate_root_bookkeeping: self.b()?,
                })
            }
            fn option(&mut self) -> Result<Option<usize>> {
                if self.b()? {
                    Ok(Some(self.u()?))
                } else {
                    Ok(None)
                }
            }
            fn shape(&mut self) -> Result<Shape> {
                Ok(match self.u()? {
                    0 => Shape::KInt,
                    1 => Shape::KBool,
                    2 => Shape::KUnit,
                    3 => Shape::KString,
                    4 => Shape::KError,
                    5 => Shape::KList(self.u()?),
                    6 => Shape::KArray(self.u()?),
                    7 => Shape::KRef(self.u()?),
                    8 => Shape::KRecord(self.vec(|r| Ok((r.u()?, r.u()?)))?),
                    9 => Shape::KVariant(self.vec(|r| Ok((r.u()?, r.ids()?)))?),
                    _ => return Err("unknown model shape"),
                })
            }
            fn op(&mut self) -> Result<Op> {
                Ok(match self.u()? {
                    0 => {
                        let n = self.n()?;
                        Op::EInt(((n >> 1) as i64) ^ (-((n & 1) as i64)))
                    }
                    1 => Op::EBool(self.b()?),
                    2 => Op::EUnit,
                    3 => Op::EString(self.u()?),
                    4 => Op::ELocal(self.u()?),
                    5 => Op::EField(self.u()?, self.u()?),
                    6 => Op::EBinary(self.u()?, self.u()?, self.u()?),
                    7 => Op::ECall(self.u()?, self.ids()?),
                    8 => Op::EConstruct(self.u()?, self.ids()?),
                    9 => Op::EData(self.u()?, self.ids()?),
                    10 => Op::ETry(self.u()?),
                    11 => Op::ERef(self.u()?),
                    12 => Op::EGet(self.u()?),
                    13 => Op::ESet(self.u()?, self.u()?),
                    14 => Op::ECollect,
                    15 => Op::ERecord(self.vec(|r| Ok((r.u()?, r.u()?)))?),
                    _ => return Err("unknown model operation"),
                })
            }
            fn body(&mut self, depth: usize) -> Result<Vec<Statement>> {
                if depth > 256 {
                    return Err("model statement depth exceeds 256");
                }
                self.vec(|r| {
                    Ok(match r.u()? {
                        0 => Statement::SLet(r.u()?, r.u()?),
                        1 => Statement::SAssign(r.u()?, r.u()?, r.u()?),
                        2 => Statement::SExpr(r.u()?),
                        3 => Statement::SReturn(r.option()?),
                        4 => Statement::SIf(r.u()?, r.body(depth + 1)?, r.body(depth + 1)?),
                        5 => Statement::SMatch(
                            r.u()?,
                            r.vec(|r| {
                                Ok(Arm {
                                    case: r.option()?,
                                    bindings: r.ids()?,
                                    body: r.body(depth + 1)?,
                                })
                            })?,
                        ),
                        6 => Statement::SFor(r.u()?, r.u()?, r.body(depth + 1)?),
                        7 => Statement::SGuard(r.u()?, r.loc()?, r.body(depth + 1)?),
                        8 => Statement::SRange(r.u()?, r.u()?, r.u()?, r.body(depth + 1)?),
                        _ => return Err("unknown model statement"),
                    })
                })
            }
        }
        impl Program {
            pub fn wire_v1(&self) -> Vec<u8> {
                let mut w = Writer(b"CGMD\x01".to_vec());
                w.u(self.schema_version as usize);
                w.u(self.strings.len());
                for s in &self.strings {
                    w.u(s.len());
                    w.0.extend_from_slice(s.as_bytes());
                }
                w.u(self.layouts.len());
                for l in &self.layouts {
                    w.u(l.name);
                    w.shape(&l.shape);
                }
                w.u(self.expressions.len());
                for e in &self.expressions {
                    w.u(e.ty);
                    w.loc(e.loc);
                    w.op(&e.op);
                }
                w.u(self.functions.len());
                for f in &self.functions {
                    w.u(f.name);
                    w.ids(&f.parameters);
                    w.ids(&f.parameter_types);
                    w.u(f.result);
                    w.ids(&f.behavioral_effects);
                    w.ids(&f.capabilities);
                    w.ids(&f.requires);
                    w.ids(&f.ensures);
                    w.u(f.result_name);
                    w.body(&f.body);
                    w.effects(f.effects);
                    w.loc(f.loc);
                }
                w.u(self.entry);
                w.loc(self.output_loc);
                w.u(self.root_maps.len());
                for r in &self.root_maps {
                    w.u(r.expression);
                    w.ids(&r.operands);
                    w.b(r.may_allocate_managed);
                    w.b(r.root_result);
                }
                w.0
            }
            pub fn decode_hex_v1(hex: &str) -> Result<Self> {
                if hex.len() > 64 * 1024 * 1024 || !hex.len().is_multiple_of(2) {
                    return Err("model hex envelope");
                }
                fn digit(b: u8) -> Result<u8> {
                    match b {
                        b'0'..=b'9' => Ok(b - b'0'),
                        b'a'..=b'f' => Ok(b - b'a' + 10),
                        _ => Err("invalid model hex digit"),
                    }
                }
                let mut bytes = Vec::new();
                bytes
                    .try_reserve_exact(hex.len() / 2)
                    .map_err(|_| "model host allocation failed")?;
                for pair in hex.as_bytes().as_chunks::<2>().0 {
                    bytes.push((digit(pair[0])? << 4) | digit(pair[1])?);
                }
                Self::decode_wire_v1(&bytes)
            }
            pub fn decode_wire_v1(bytes: &[u8]) -> Result<Self> {
                if bytes.len() > 32 * 1024 * 1024 || !bytes.starts_with(b"CGMD\x01") {
                    return Err("model wire envelope");
                }
                let mut r = Reader { bytes, at: 5 };
                let schema_version = u32::try_from(r.u()?).map_err(|_| "model schema overflow")?;
                if schema_version != 1 {
                    return Err("unknown model schema");
                }
                let strings = r.vec(|r| {
                    let n = r.u()?;
                    let end = r.at.checked_add(n).ok_or("model string overflow")?;
                    let bytes = r.bytes.get(r.at..end).ok_or("truncated model string")?;
                    r.at = end;
                    let text = std::str::from_utf8(bytes).map_err(|_| "invalid model UTF-8")?;
                    let mut s = String::new();
                    s.try_reserve_exact(n)
                        .map_err(|_| "model host allocation failed")?;
                    s.push_str(text);
                    Ok(s)
                })?;
                let layouts = r.vec(|r| {
                    Ok(Layout {
                        name: r.u()?,
                        shape: r.shape()?,
                    })
                })?;
                if layouts.len() > 4096 {
                    return Err("model layout envelope");
                }
                let expressions = r.vec(|r| {
                    Ok(Expression {
                        ty: r.u()?,
                        loc: r.loc()?,
                        op: r.op()?,
                    })
                })?;
                if expressions.len() > 1_000_000 {
                    return Err("model expression envelope");
                }
                let functions = r.vec(|r| {
                    Ok(Function {
                        name: r.u()?,
                        parameters: r.ids()?,
                        parameter_types: r.ids()?,
                        result: r.u()?,
                        behavioral_effects: r.ids()?,
                        capabilities: r.ids()?,
                        requires: r.ids()?,
                        ensures: r.ids()?,
                        result_name: r.u()?,
                        body: r.body(0)?,
                        effects: r.effects()?,
                        loc: r.loc()?,
                    })
                })?;
                if functions.len() > 256 {
                    return Err("model function envelope");
                }
                let entry = r.u()?;
                let output_loc = r.loc()?;
                let root_maps = r.vec(|r| {
                    Ok(RootMapV1 {
                        expression: r.u()?,
                        operands: r.ids()?,
                        may_allocate_managed: r.b()?,
                        root_result: r.b()?,
                    })
                })?;
                if r.at != bytes.len() {
                    return Err("trailing model bytes");
                }
                Ok(Self {
                    strings,
                    layouts,
                    expressions,
                    functions,
                    entry,
                    schema_version,
                    output_loc,
                    root_maps,
                })
            }
        }
    }
    mod engine {
        use super::model::*;
        use crate::{
            AllocationError, CleanupReport, GraphError, Handle, MemoryError, MemoryReport,
            ScopedRoots, Session,
        };
        use std::collections::{HashMap, HashSet};
        use std::fmt;

        const STEPS: usize = 1_000_000;
        const NODES: usize = 65_537;
        const ELEMENTS: usize = 65_536;
        const BYTES: usize = 1_048_576;
        const WRAPPERS: usize = 32;
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum Atom {
            Int(i64),
            Bool(bool),
            Unit,
            Heap(Handle),
            Error(u8),
        }
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        struct Carrier {
            ty: usize,
            case: usize,
        }
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        struct Value {
            ty: usize,
            atom: Atom,
            carriers: [Carrier; WRAPPERS],
            depth: usize,
        }
        impl Value {
            fn new(ty: usize, atom: Atom) -> Self {
                Self {
                    ty,
                    atom,
                    carriers: [Carrier::default(); WRAPPERS],
                    depth: 0,
                }
            }
            fn handle(self) -> Option<Handle> {
                if let Atom::Heap(h) = self.atom {
                    Some(h)
                } else {
                    None
                }
            }
            fn wrap(mut self, ty: usize, case: usize) -> Result<Self, Fault> {
                if self.depth == WRAPPERS {
                    return Err(Fault::allocation(AllocationError::ShapeLimit));
                }
                self.carriers[self.depth] = Carrier { ty: self.ty, case };
                self.depth += 1;
                self.ty = ty;
                Ok(self)
            }
            fn unwrap(mut self) -> (usize, Self) {
                self.depth -= 1;
                let c = self.carriers[self.depth];
                self.ty = c.ty;
                (c.case, self)
            }
        }
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub enum Snapshot {
            Integer(i64),
            Boolean(bool),
            Text(String),
            Unit,
            Record(String, Vec<(String, Snapshot)>),
            Variant(String, String, Vec<Snapshot>),
            Sequence(bool, Vec<Snapshot>),
        }
        impl fmt::Display for Snapshot {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    Self::Integer(v) => write!(f, "{v}"),
                    Self::Boolean(v) => write!(f, "{v}"),
                    Self::Text(v) => f.write_str(v),
                    Self::Unit => Ok(()),
                    Self::Record(n, fields) => {
                        write!(f, "{n}(")?;
                        for (i, (n, v)) in fields.iter().enumerate() {
                            if i > 0 {
                                f.write_str(", ")?;
                            }
                            write!(f, "{n}: {v}")?;
                        }
                        f.write_str(")")
                    }
                    Self::Variant(n, c, vs) => {
                        write!(f, "{n}.{c}(")?;
                        for (i, v) in vs.iter().enumerate() {
                            if i > 0 {
                                f.write_str(", ")?;
                            }
                            write!(f, "{v}")?;
                        }
                        f.write_str(")")
                    }
                    Self::Sequence(array, vs) => {
                        if *array {
                            f.write_str("Array")?;
                        }
                        f.write_str("[")?;
                        for (i, v) in vs.iter().enumerate() {
                            if i > 0 {
                                f.write_str(", ")?;
                            }
                            write!(f, "{v}")?;
                        }
                        f.write_str("]")
                    }
                }
            }
        }
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct Failure {
            pub code: &'static str,
            pub message: String,
            pub loc: Loc,
            pub function: Option<usize>,
        }
        #[derive(Clone, Debug, Default, PartialEq, Eq)]
        pub struct Evidence {
            pub allocations: usize,
            pub roots_registered: usize,
            pub roots_released: usize,
            pub cleanup_failures: usize,
            pub recovered: usize,
            pub guard_entries: usize,
            pub guard_exits: usize,
            pub guard_entry_failures: usize,
            pub graph_reads: usize,
            pub graph_writes: usize,
            pub baseline_roots: usize,
            pub final_roots: usize,
            pub charged_bytes: u128,
            pub steps: usize,
            pub memory_effects_v1: MemoryEffectsV1,
            pub behavioral_effects: Vec<String>,
            pub capabilities: Vec<String>,
        }
        impl Evidence {
            pub fn json(&self, memory: &MemoryReport) -> String {
                format!("{{\"schema_version\":1,\"allocations\":{},\"roots_registered\":{},\"roots_released\":{},\"cleanup_failures\":{},\"recovered\":{},\"guard_entries\":{},\"guard_exits\":{},\"guard_entry_failures\":{},\"graph_reads\":{},\"graph_writes\":{},\"baseline_roots\":{},\"final_roots\":{},\"charged_bytes\":{},\"behavioral_effects\":{:?},\"capabilities\":{:?},\"memory_effects_v1\":{{\"may_allocate_managed\":{},\"may_auto_collect_at_allocation\":{},\"may_explicitly_collect\":{},\"may_allocate_root_bookkeeping\":{}}},\"memory\":{}}}",self.allocations,self.roots_registered,self.roots_released,self.cleanup_failures,self.recovered,self.guard_entries,self.guard_exits,self.guard_entry_failures,self.graph_reads,self.graph_writes,self.baseline_roots,self.final_roots,self.charged_bytes,self.behavioral_effects,self.capabilities,self.memory_effects_v1.may_allocate_managed,self.memory_effects_v1.may_auto_collect_at_allocation,self.memory_effects_v1.may_explicitly_collect,self.memory_effects_v1.may_allocate_root_bookkeeping,memory.to_json())
            }
        }
        #[derive(Clone, Debug)]
        pub struct Execution {
            pub result: Result<Snapshot, Failure>,
            pub evidence: Evidence,
            pub memory: MemoryReport,
        }
        #[derive(Debug)]
        enum Fault {
            AllocationAt(AllocationError, Loc, Option<usize>),
            Primary(Failure),
        }
        impl Fault {
            fn allocation(error: AllocationError) -> Self {
                Self::AllocationAt(error, Loc::default(), None)
            }
        }
        type Fallible<T> = Result<T, Fault>;
        fn invariant(message: impl Into<String>) -> Fault {
            Fault::Primary(Failure {
                code: "CGR091",
                message: message.into(),
                loc: Loc::default(),
                function: None,
            })
        }
        impl From<GraphError> for Fault {
            fn from(e: GraphError) -> Self {
                match e {
                    GraphError::Memory(MemoryError::Allocation(e)) => Self::allocation(e),
                    GraphError::ShapeLimit => Self::allocation(AllocationError::ShapeLimit),
                    _ => invariant(format!("managed graph invariant: {e:?}")),
                }
            }
        }
        impl From<MemoryError> for Fault {
            fn from(e: MemoryError) -> Self {
                match e {
                    MemoryError::Allocation(e) => Self::allocation(e),
                    _ => invariant(format!("managed memory invariant: {e:?}")),
                }
            }
        }
        fn primary(code: &'static str, message: impl Into<String>, loc: Loc) -> Fault {
            Fault::Primary(Failure {
                code,
                message: message.into(),
                loc,
                function: None,
            })
        }
        fn failure(f: Fault, loc: Loc) -> Failure {
            match f {
                Fault::AllocationAt(e, at, function) => Failure {
                    code: "CGR090",
                    message: format!("managed allocation failed: {e:?}"),
                    loc: if at == Loc::default() { loc } else { at },
                    function,
                },
                Fault::Primary(mut e) => {
                    if e.loc == Loc::default() {
                        e.loc = loc;
                    }
                    e
                }
            }
        }
        struct Machine<'a> {
            p: &'a Program,
            args: &'a [String],
            steps: usize,
            depth: usize,
            step_limit: usize,
            evidence: Evidence,
        }
        struct Env(Vec<HashMap<usize, Value>>);
        impl Env {
            fn new() -> Self {
                Self(vec![HashMap::new()])
            }
            fn get(&self, n: usize) -> Fallible<Value> {
                self.0
                    .iter()
                    .rev()
                    .find_map(|m| m.get(&n).copied())
                    .ok_or_else(|| invariant("uninitialized local"))
            }
            fn pop(
                &mut self,
                roots: &mut ScopedRoots<'_>,
                evidence: &mut Evidence,
            ) -> Fallible<()> {
                for v in self
                    .0
                    .pop()
                    .ok_or_else(|| invariant("scope underflow"))?
                    .into_values()
                {
                    release(roots, v, evidence)?;
                }
                Ok(())
            }
        }
        fn root(roots: &mut ScopedRoots<'_>, v: Value, e: &mut Evidence) -> Fallible<()> {
            if let Some(h) = v.handle() {
                roots.root(h)?;
                e.roots_registered += 1;
            }
            Ok(())
        }
        fn release(roots: &mut ScopedRoots<'_>, v: Value, e: &mut Evidence) -> Fallible<()> {
            if let Some(h) = v.handle() {
                roots.release(h)?;
                e.roots_released += 1;
            }
            Ok(())
        }
        fn cleanup(e: &mut Evidence, r: CleanupReport) {
            e.roots_released += r.released_roots;
            if r.first_error.is_some() {
                e.cleanup_failures += 1;
            }
        }
        fn integer(v: Value) -> Fallible<i64> {
            match v.atom {
                Atom::Int(n) if v.depth == 0 => Ok(n),
                _ => Err(invariant("expected integer")),
            }
        }
        fn boolean(v: Value) -> Fallible<bool> {
            match v.atom {
                Atom::Bool(n) if v.depth == 0 => Ok(n),
                _ => Err(invariant("expected Boolean")),
            }
        }
        fn reserve<T>(v: &mut Vec<T>, n: usize) -> Fallible<()> {
            v.try_reserve_exact(n)
                .map_err(|_| Fault::allocation(AllocationError::HostAllocation))
        }

        pub fn run(p: &Program, args: &[String]) -> Execution {
            match Session::try_new() {
                Ok(s) => run_with_session(p, args, s),
                Err(e) => Execution {
                    result: Err(Failure {
                        code: "CGR090",
                        message: format!("managed session entry failed: {e:?}"),
                        loc: p.functions.get(p.entry).map_or(Loc::default(), |f| f.loc),
                        function: None,
                    }),
                    evidence: Evidence::default(),
                    memory: MemoryReport {
                        schema_version: 1,
                        regions: vec![],
                        totals: Default::default(),
                    },
                },
            }
        }
        /// Copied-scalar bridge into an existing execution Session. No session is
        /// created, replaced or collected at entry/cleanup.
        pub fn run_function_borrowed(
            p: &Program,
            function: usize,
            args: &[Snapshot],
            session: &mut Session,
            remaining_steps: usize,
            depth: usize,
        ) -> Execution {
            let baseline = session.root_census().map_or(0, |r| r.root_registrations);
            let mut m = Machine {
                p,
                args: &[],
                steps: 0,
                depth,
                step_limit: remaining_steps,
                evidence: Evidence {
                    baseline_roots: baseline,
                    ..Evidence::default()
                },
            };
            let mut roots = session.scoped_roots();
            let loc = p.functions.get(function).map_or(Loc::default(), |f| f.loc);
            let result = (|| {
                m.validate()?;
                let f = p
                    .functions
                    .get(function)
                    .ok_or_else(|| invariant("bridge callee"))?;
                if args.len() != f.parameter_types.len() {
                    return Err(invariant("bridge arity"));
                }
                let mut values = Vec::new();
                reserve(&mut values, args.len())?;
                for (arg, ty) in args.iter().zip(&f.parameter_types) {
                    let atom = match (arg, &p.layouts[*ty].shape) {
                        (Snapshot::Integer(n), Shape::KInt) => Atom::Int(*n),
                        (Snapshot::Boolean(b), Shape::KBool) => Atom::Bool(*b),
                        (Snapshot::Unit, Shape::KUnit) => Atom::Unit,
                        _ => return Err(invariant("bridge requires copied scalar arguments")),
                    };
                    values.push(Value::new(*ty, atom));
                }
                let value = m.call(function, values, &mut roots)?;
                root(&mut roots, value, &mut m.evidence)?;
                m.snapshot(value, &roots, &mut 0, 0)
            })();
            let outcome = roots.finish(result);
            cleanup(&mut m.evidence, outcome.cleanup);
            drop(roots);
            m.evidence.final_roots = session
                .root_census()
                .map_or(usize::MAX, |r| r.root_registrations);
            m.evidence.steps = m.steps;
            m.evidence.charged_bytes = session.memory_observation_v2().charged_bytes;
            Execution {
                result: outcome.result.map_err(|e| failure(e, loc)),
                evidence: m.evidence,
                memory: session.memory_report(),
            }
        }
        pub fn run_with_session(p: &Program, args: &[String], mut session: Session) -> Execution {
            let baseline = session.root_census().map_or(0, |r| r.root_registrations);
            let mut m = Machine {
                p,
                args,
                steps: 0,
                depth: 0,
                step_limit: STEPS,
                evidence: Evidence {
                    baseline_roots: baseline,
                    ..Evidence::default()
                },
            };
            if let Some(f) = p.functions.get(p.entry) {
                m.evidence.memory_effects_v1 = f.effects;
                m.evidence.behavioral_effects = f
                    .behavioral_effects
                    .iter()
                    .map(|i| p.strings[*i].clone())
                    .collect();
                m.evidence.capabilities = f
                    .capabilities
                    .iter()
                    .map(|i| p.strings[*i].clone())
                    .collect();
            }
            let mut roots = session.scoped_roots();
            let loc = p.functions.get(p.entry).map_or(Loc::default(), |f| f.loc);
            let result = (|| {
                m.validate()?;
                if p.entry == usize::MAX {
                    return Err(primary("CGR008", "module requires main", p.output_loc));
                }
                let v = m.call(p.entry, vec![], &mut roots)?;
                root(&mut roots, v, &mut m.evidence)?;
                let mut count = 0;
                let snap = m.snapshot(v, &roots, &mut count, 0).map_err(|e| match e {
                    Fault::Primary(mut e) => {
                        if e.loc == Loc::default() {
                            e.loc = p.output_loc;
                        }
                        Fault::Primary(e)
                    }
                    e => e,
                })?;
                if snap.to_string().len() > BYTES {
                    return Err(primary(
                        "CGR010",
                        "output exceeds 1048576 bytes",
                        p.output_loc,
                    ));
                }
                Ok(snap)
            })();
            let outcome = roots.finish(result);
            cleanup(&mut m.evidence, outcome.cleanup);
            drop(roots);
            m.evidence.final_roots = session
                .root_census()
                .map_or(usize::MAX, |r| r.root_registrations);
            m.evidence.charged_bytes = session.memory_observation_v2().charged_bytes;
            m.evidence.steps = m.steps;
            Execution {
                result: outcome.result.map_err(|e| failure(e, loc)),
                evidence: m.evidence,
                memory: session.memory_report(),
            }
        }
        impl Machine<'_> {
            fn validate(&self) -> Fallible<()> {
                if self.p.schema_version != 1
                    || (self.p.entry >= self.p.functions.len() && self.p.entry != usize::MAX)
                    || self.p.layouts.len() > 4096
                    || self.p.expressions.len() > 1_000_000
                {
                    return Err(invariant("invalid managed program envelope"));
                }
                for l in &self.p.layouts {
                    if l.name >= self.p.strings.len() {
                        return Err(invariant("layout name"));
                    }
                }
                Ok(())
            }
            fn step(&mut self, loc: Loc) -> Fallible<()> {
                self.work(1, loc)
            }
            fn work(&mut self, units: usize, loc: Loc) -> Fallible<()> {
                self.steps = self
                    .steps
                    .checked_add(units)
                    .ok_or_else(|| primary("CGR010", "execution work budget exceeded", loc))?;
                if self.steps > self.step_limit {
                    Err(primary("CGR010", "execution work budget exceeded", loc))
                } else {
                    Ok(())
                }
            }
            fn unit(&self) -> Value {
                Value::new(
                    self.p
                        .layouts
                        .iter()
                        .position(|t| t.shape == Shape::KUnit)
                        .unwrap_or(0),
                    Atom::Unit,
                )
            }
            fn int(&self, n: i64) -> Value {
                Value::new(
                    self.p
                        .layouts
                        .iter()
                        .position(|t| t.shape == Shape::KInt)
                        .unwrap_or(0),
                    Atom::Int(n),
                )
            }
            fn bind(
                &mut self,
                env: &mut Env,
                roots: &mut ScopedRoots<'_>,
                n: usize,
                v: Value,
            ) -> Fallible<()> {
                if let Some(old) = env
                    .0
                    .last_mut()
                    .ok_or_else(|| invariant("scope"))?
                    .insert(n, v)
                {
                    release(roots, old, &mut self.evidence)?;
                }
                Ok(())
            }
            fn assign(
                &mut self,
                env: &mut Env,
                roots: &mut ScopedRoots<'_>,
                n: usize,
                v: Value,
            ) -> Fallible<()> {
                let old = env
                    .0
                    .iter_mut()
                    .rev()
                    .find_map(|m| m.get_mut(&n))
                    .ok_or_else(|| invariant("assignment local"))?;
                let old = std::mem::replace(old, v);
                release(roots, old, &mut self.evidence)
            }
            fn call(&mut self, id: usize, args: Vec<Value>, s: &mut Session) -> Fallible<Value> {
                let f = self
                    .p
                    .functions
                    .get(id)
                    .ok_or_else(|| invariant("callee index"))?;
                if self.depth >= 64 {
                    return Err(primary(
                        "CGR011",
                        "function call depth limit of 64 exceeded",
                        f.loc,
                    ));
                }
                if args.len() != f.parameters.len() {
                    return Err(primary("CGR012", "function argument count mismatch", f.loc));
                }
                self.depth += 1;
                let mut roots = s.scoped_roots();
                let mut env = Env::new();
                let result = (|| {
                    for (n, v) in f.parameters.iter().zip(args) {
                        root(&mut roots, v, &mut self.evidence)?;
                        self.bind(&mut env, &mut roots, *n, v)?;
                    }
                    for &e in &f.requires {
                        if !boolean(self.expr(e, &env, &mut roots)?)? {
                            return Err(primary(
                                "CGR015",
                                "requires condition failed",
                                self.p.expressions[e].loc,
                            ));
                        }
                    }
                    let (v, _) = self.block(&f.body, &mut env, &mut roots, false)?;
                    root(&mut roots, v, &mut self.evidence)?;
                    self.bind(&mut env, &mut roots, f.result_name, v)?;
                    for &e in &f.ensures {
                        if !boolean(self.expr(e, &env, &mut roots)?)? {
                            return Err(primary(
                                "CGR016",
                                "ensures condition failed",
                                self.p.expressions[e].loc,
                            ));
                        }
                    }
                    Ok(v)
                })();
                let outcome = roots.finish(result);
                cleanup(&mut self.evidence, outcome.cleanup);
                self.depth -= 1;
                outcome.result.map_err(|e| match e {
                    Fault::Primary(mut e) => {
                        if e.function.is_none() {
                            e.function = Some(id);
                        }
                        Fault::Primary(e)
                    }
                    Fault::AllocationAt(error, at, function) => Fault::AllocationAt(
                        error,
                        if at == Loc::default() { f.loc } else { at },
                        function.or(Some(id)),
                    ),
                })
            }
            fn block(
                &mut self,
                body: &[Statement],
                env: &mut Env,
                roots: &mut ScopedRoots<'_>,
                nested: bool,
            ) -> Fallible<(Value, bool)> {
                if nested {
                    env.0.push(HashMap::new());
                }
                let mut tail = self.unit();
                let result = (|| {
                    for st in body {
                        self.step(Loc::default())?;
                        release(roots, tail, &mut self.evidence)?;
                        tail = self.unit();
                        match st {
                            Statement::SLet(n, e) => {
                                let v = self.expr(*e, env, roots)?;
                                self.bind(env, roots, *n, v)?;
                            }
                            Statement::SAssign(n, op, e) => {
                                let mut v = self.expr(*e, env, roots)?;
                                if *op != 0 {
                                    v = self.binary(
                                        env.get(*n)?,
                                        v,
                                        *op,
                                        self.p.expressions[*e].loc,
                                        roots,
                                    )?;
                                }
                                self.assign(env, roots, *n, v)?;
                            }
                            Statement::SExpr(e) => {
                                tail = self.expr(*e, env, roots)?;
                            }
                            Statement::SReturn(e) => {
                                let v = if let Some(e) = e {
                                    self.expr(*e, env, roots)?
                                } else {
                                    self.unit()
                                };
                                return Ok((v, true));
                            }
                            Statement::SIf(e, yes, no) => {
                                let test = boolean(self.expr(*e, env, roots)?)?;
                                let (v, ret) =
                                    self.block(if test { yes } else { no }, env, roots, true)?;
                                if ret {
                                    return Ok((v, true));
                                }
                                release(roots, v, &mut self.evidence)?;
                            }
                            Statement::SMatch(e, arms) => {
                                let v = self.expr(*e, env, roots)?;
                                let (case, children) = self.parts(v, roots)?;
                                let arm = arms
                                    .iter()
                                    .find(|a| a.case.is_none_or(|c| c == case))
                                    .ok_or_else(|| invariant("nonexhaustive runtime match"))?;
                                env.0.push(HashMap::new());
                                let branch = (|| {
                                    for (n, v) in arm.bindings.iter().zip(children) {
                                        root(roots, v, &mut self.evidence)?;
                                        self.bind(env, roots, *n, v)?;
                                    }
                                    self.block(&arm.body, env, roots, false)
                                })();
                                let popped = env.pop(roots, &mut self.evidence);
                                release(roots, v, &mut self.evidence)?;
                                let (v, ret) = branch?;
                                popped?;
                                if ret {
                                    return Ok((v, true));
                                }
                                tail = v;
                            }
                            Statement::SFor(n, e, body) => {
                                let xs = self.expr(*e, env, roots)?;
                                let values = self.parts(xs, roots)?.1;
                                let result: Fallible<Option<Value>> = (|| {
                                    for v in values {
                                        env.0.push(HashMap::new());
                                        root(roots, v, &mut self.evidence)?;
                                        self.bind(env, roots, *n, v)?;
                                        let branch = self.block(body, env, roots, false);
                                        let popped = env.pop(roots, &mut self.evidence);
                                        let (v, ret) = branch?;
                                        popped?;
                                        if ret {
                                            return Ok(Some(v));
                                        }
                                        release(roots, v, &mut self.evidence)?;
                                    }
                                    Ok(None)
                                })(
                                );
                                release(roots, xs, &mut self.evidence)?;
                                if let Some(v) = result? {
                                    return Ok((v, true));
                                }
                            }
                            Statement::SRange(n, start, end, body) => {
                                let a = integer(self.expr(*start, env, roots)?)?;
                                let b = integer(self.expr(*end, env, roots)?)?;
                                for i in a..b {
                                    self.step(Loc::default())?;
                                    env.0.push(HashMap::new());
                                    self.bind(env, roots, *n, self.int(i))?;
                                    let branch = self.block(body, env, roots, false);
                                    let popped = env.pop(roots, &mut self.evidence);
                                    let (v, ret) = branch?;
                                    popped?;
                                    if ret {
                                        return Ok((v, true));
                                    }
                                    release(roots, v, &mut self.evidence)?;
                                }
                            }
                            Statement::SGuard(budget, loc, body) => {
                                let result = roots.with_no_gc(*budget, |r| {
                                    self.evidence.guard_entries += 1;
                                    let out = self.block(body, env, r, true);
                                    self.evidence.guard_exits += 1;
                                    out
                                });
                                match result {
                                    Err(e) => {
                                        self.evidence.guard_entry_failures += 1;
                                        return Err(primary(
                                            "CGR090",
                                            format!("no_gc guard entry failed: {e:?}"),
                                            *loc,
                                        ));
                                    }
                                    Ok(r) => {
                                        let (v, ret) = r?;
                                        if ret {
                                            return Ok((v, true));
                                        }
                                        release(roots, v, &mut self.evidence)?;
                                    }
                                }
                            }
                        }
                    }
                    Ok((tail, false))
                })();
                // Returned handles remain protected by existing registrations until this
                // nonallocating cleanup finishes; callers root before their next safepoint.
                // A successful block transfers its tail registration to its
                // caller. The caller either keeps it or releases it immediately.
                // Function-level finish handles both error and unwind cleanup.
                let popped = if nested {
                    env.pop(roots, &mut self.evidence)
                } else {
                    Ok(())
                };
                match result {
                    Err(e) => Err(e),
                    Ok(v) => {
                        popped?;
                        Ok(v)
                    }
                }
            }
            fn expr(
                &mut self,
                id: usize,
                env: &Env,
                roots: &mut ScopedRoots<'_>,
            ) -> Fallible<Value> {
                let e = self
                    .p
                    .expressions
                    .get(id)
                    .ok_or_else(|| invariant("expression index"))?;
                self.step(e.loc)?;
                let checkpoint = roots.checkpoint();
                let mut result = self.expr_inner(e, env, roots).and_then(|v| {
                    if let Some(h) = v.handle() {
                        if !roots.contains_since(checkpoint, h) {
                            root(roots, v, &mut self.evidence)?;
                        }
                    }
                    Ok(v)
                });
                if matches!(e.op, Op::ETry(_)) {
                    if let Err(Fault::AllocationAt(error, _, _)) = result {
                        self.evidence.recovered += 1;
                        let error_ty = match &self.p.layouts[e.ty].shape {
                            Shape::KVariant(v) => v[1].1[0],
                            _ => return Err(invariant("recovery layout")),
                        };
                        result = Value::new(error_ty, Atom::Error(error_code(error))).wrap(e.ty, 1);
                    }
                }
                let retained = result.as_ref().ok().and_then(|v| v.handle());
                let report = roots.finish_since(checkpoint, retained);
                cleanup(&mut self.evidence, report);
                if result.is_ok() {
                    self.step(e.loc)?;
                }
                result.map_err(|f| match f {
                    Fault::Primary(mut d) => {
                        if d.loc == Loc::default() {
                            d.loc = e.loc;
                        }
                        Fault::Primary(d)
                    }
                    Fault::AllocationAt(error, at, function) => Fault::AllocationAt(
                        error,
                        if at == Loc::default() { e.loc } else { at },
                        function,
                    ),
                })
            }
            fn child(
                &mut self,
                id: usize,
                env: &Env,
                roots: &mut ScopedRoots<'_>,
            ) -> Fallible<Value> {
                self.expr(id, env, roots)
            }
            fn children(
                &mut self,
                ids: &[usize],
                env: &Env,
                roots: &mut ScopedRoots<'_>,
            ) -> Fallible<Vec<Value>> {
                let mut out = Vec::new();
                reserve(&mut out, ids.len())?;
                for id in ids {
                    out.push(self.child(*id, env, roots)?);
                }
                Ok(out)
            }
            fn expr_inner(
                &mut self,
                e: &Expression,
                env: &Env,
                roots: &mut ScopedRoots<'_>,
            ) -> Fallible<Value> {
                match &e.op {
                    Op::EInt(n) => Ok(Value::new(e.ty, Atom::Int(*n))),
                    Op::EBool(v) => Ok(Value::new(e.ty, Atom::Bool(*v))),
                    Op::EUnit => Ok(self.unit()),
                    Op::EString(n) => self.string(e.ty, &self.p.strings[*n], roots),
                    Op::ELocal(n) => env.get(*n),
                    Op::EField(value, index) => {
                        let v = self.child(*value, env, roots)?;
                        self.parts(v, roots)?
                            .1
                            .get(*index)
                            .copied()
                            .ok_or_else(|| invariant("field index"))
                    }
                    Op::EBinary(a, op, b) => {
                        let a = self.child(*a, env, roots)?;
                        let b = self.child(*b, env, roots)?;
                        self.binary(a, b, *op, e.loc, roots)
                    }
                    Op::ERecord(fields) => {
                        let mut values = vec![self.unit(); fields.len()];
                        for (index, id) in fields {
                            let v = self.child(*id, env, roots)?;
                            *values
                                .get_mut(*index)
                                .ok_or_else(|| invariant("record field ordering"))? = v;
                        }
                        self.construct(e.ty, 0, &values, roots)
                    }
                    Op::ECall(f, args) => {
                        let args = self.children(args, env, roots)?;
                        self.call(*f, args, roots)
                    }
                    Op::EConstruct(case, args) => {
                        let args = self.children(args, env, roots)?;
                        self.construct(e.ty, *case, &args, roots)
                    }
                    Op::EData(op, args) => {
                        let loc = if *op == 11 {
                            args.first().map_or(e.loc, |id| self.p.expressions[*id].loc)
                        } else {
                            e.loc
                        };
                        let args = self.children(args, env, roots)?;
                        self.data(*op, e.ty, args, roots, loc)
                    }
                    Op::ETry(inner) => {
                        let result = self.child(*inner, env, roots);
                        match result {
                            Ok(v) => v.wrap(e.ty, 0),
                            Err(Fault::AllocationAt(error, _, _)) => {
                                self.evidence.recovered += 1;
                                let error_ty = match &self.p.layouts[e.ty].shape {
                                    Shape::KVariant(v) => v[1].1[0],
                                    _ => return Err(invariant("recovery Result layout")),
                                };
                                Value::new(error_ty, Atom::Error(error_code(error))).wrap(e.ty, 1)
                            }
                            Err(e) => Err(e),
                        }
                    }
                    Op::ERef(inner) => {
                        let v = self.child(*inner, env, roots)?;
                        self.construct(e.ty, 0, &[v], roots)
                    }
                    Op::EGet(inner) => {
                        let v = self.child(*inner, env, roots)?;
                        self.parts(v, roots)?
                            .1
                            .first()
                            .copied()
                            .ok_or_else(|| invariant("cell payload"))
                    }
                    Op::ESet(reference, value) => {
                        let r = self.child(*reference, env, roots)?;
                        let v = self.child(*value, env, roots)?;
                        let Shape::KRef(target) = self.p.layouts[r.ty].shape else {
                            return Err(invariant("cell layout"));
                        };
                        if v.ty != target {
                            return Err(invariant("cell payload type"));
                        }
                        let h = r.handle().ok_or_else(|| invariant("cell handle"))?;
                        let mut seen = HashSet::new();
                        seen.insert(h);
                        let mut count = 1;
                        self.count(v, roots, &mut seen, &mut count, 0)?;
                        self.work(count.saturating_sub(1), e.loc)?;
                        let (mut bytes, refs) = self.encode(r.ty, 0, &[v])?;
                        let shape = roots.graph_shape(h)?;
                        if shape.payload_bytes != bytes.len() || shape.references != refs.len() {
                            return Err(invariant("cell encoding changed shape"));
                        }
                        for (i, h2) in refs.into_iter().enumerate() {
                            roots.set_graph_reference(h, i, h2)?;
                        }
                        roots.write_graph_bytes(h, 0, &bytes)?;
                        bytes.clear();
                        self.evidence.graph_writes += 1;
                        Ok(self.unit())
                    }
                    Op::ECollect => {
                        let region = roots.default_region();
                        roots.collect(region)?;
                        Ok(self.unit())
                    }
                }
            }
            fn encode(
                &self,
                ty: usize,
                case: usize,
                values: &[Value],
            ) -> Fallible<(Vec<u8>, Vec<Option<Handle>>)> {
                if values.len() > ELEMENTS {
                    return Err(Fault::allocation(AllocationError::ShapeLimit));
                }
                let mut bytes = Vec::new();
                reserve(&mut bytes, 12 + values.len().saturating_mul(18))?;
                bytes.extend_from_slice(&(ty as u32).to_le_bytes());
                bytes.extend_from_slice(&(case as u32).to_le_bytes());
                bytes.extend_from_slice(&(values.len() as u32).to_le_bytes());
                let mut refs = Vec::new();
                reserve(&mut refs, values.len())?;
                for v in values {
                    bytes.extend_from_slice(&(v.ty as u32).to_le_bytes());
                    bytes.push(v.depth as u8);
                    for c in &v.carriers[..v.depth] {
                        bytes.extend_from_slice(&(c.ty as u32).to_le_bytes());
                        bytes.extend_from_slice(&(c.case as u32).to_le_bytes());
                    }
                    match v.atom {
                        Atom::Int(n) => {
                            bytes.push(0);
                            bytes.extend_from_slice(&n.to_le_bytes());
                        }
                        Atom::Bool(b) => {
                            bytes.push(1);
                            bytes.push(u8::from(b));
                        }
                        Atom::Unit => bytes.push(2),
                        Atom::Heap(_) => bytes.push(3),
                        Atom::Error(n) => {
                            bytes.push(4);
                            bytes.push(n);
                        }
                    }
                    refs.push(v.handle());
                }
                // A recoverable Result may be either an intrinsic carrier or an
                // ordinary variant handle. Cells reserve the maximum carrier
                // encoding so replacing either representation never reallocates.
                if let Shape::KRef(target) = self.p.layouts[ty].shape {
                    if matches!(&self.p.layouts[target].shape,Shape::KVariant(cases) if cases.len()==2 && cases[1].1.len()==1 && self.p.layouts[cases[1].1[0]].shape==Shape::KError)
                    {
                        let padded = 12 + 4 + 1 + WRAPPERS * 8 + 1 + 8;
                        if bytes.len() < padded {
                            let extra = padded - bytes.len();
                            reserve(&mut bytes, extra)?;
                            bytes.resize(padded, 0);
                        }
                    }
                }
                if bytes.len() > BYTES {
                    return Err(Fault::allocation(AllocationError::ShapeLimit));
                }
                Ok((bytes, refs))
            }
            fn construct(
                &mut self,
                ty: usize,
                case: usize,
                values: &[Value],
                s: &mut Session,
            ) -> Fallible<Value> {
                match &self.p.layouts[ty].shape {
                    Shape::KRecord(fs) => {
                        if fs.len() != values.len()
                            || fs.iter().zip(values).any(|((_, t), v)| *t != v.ty)
                        {
                            return Err(invariant("record type/layout mismatch"));
                        }
                    }
                    Shape::KVariant(cases) => {
                        let fs = &cases.get(case).ok_or_else(|| invariant("variant case"))?.1;
                        if fs.len() != values.len()
                            || fs.iter().zip(values).any(|(t, v)| *t != v.ty)
                        {
                            return Err(invariant("variant type/layout mismatch"));
                        }
                    }
                    Shape::KList(t) | Shape::KArray(t) => {
                        if values.iter().any(|v| v.ty != *t) {
                            return Err(invariant("collection type mismatch"));
                        }
                    }
                    Shape::KRef(t) => {
                        if values.len() != 1 || values[0].ty != *t {
                            return Err(invariant("reference target type mismatch"));
                        }
                    }
                    _ => return Err(invariant("nonaggregate construction")),
                }
                let mut visited = HashSet::new();
                let mut count = 1;
                for v in values {
                    self.count(*v, s, &mut visited, &mut count, 0)?;
                }
                self.work(count.saturating_sub(1), Loc::default())?;
                let (bytes, refs) = self.encode(ty, case, values)?;
                let h = s.alloc_graph(s.default_region(), &bytes, &refs)?;
                self.evidence.allocations += 1;
                self.evidence.graph_writes += 1;
                Ok(Value::new(ty, Atom::Heap(h)))
            }
            fn string(&mut self, ty: usize, text: &str, s: &mut Session) -> Fallible<Value> {
                if text.len() > BYTES {
                    return Err(Fault::allocation(AllocationError::ShapeLimit));
                }
                let h = s.alloc_graph(s.default_region(), text.as_bytes(), &[])?;
                self.evidence.allocations += 1;
                self.evidence.graph_writes += 1;
                Ok(Value::new(ty, Atom::Heap(h)))
            }
            fn text(&mut self, v: Value, s: &Session) -> Fallible<String> {
                if v.depth != 0 || self.p.layouts[v.ty].shape != Shape::KString {
                    return Err(invariant("string type"));
                }
                let h = v.handle().ok_or_else(|| invariant("string handle"))?;
                let shape = s.graph_shape(h)?;
                if shape.payload_bytes > BYTES || shape.references != 0 {
                    return Err(invariant("string graph layout"));
                }
                let mut bytes = Vec::new();
                reserve(&mut bytes, shape.payload_bytes)?;
                bytes.resize(shape.payload_bytes, 0);
                s.read_graph_bytes(h, 0, &mut bytes)?;
                self.evidence.graph_reads += 1;
                String::from_utf8(bytes).map_err(|_| invariant("invalid stored UTF-8"))
            }
            fn parts(&mut self, v: Value, s: &Session) -> Fallible<(usize, Vec<Value>)> {
                if v.depth > 0 {
                    let (c, p) = v.unwrap();
                    return Ok((c, vec![p]));
                }
                if let Atom::Error(c) = v.atom {
                    return Ok((c as usize, vec![]));
                }
                let h = v.handle().ok_or_else(|| invariant("aggregate handle"))?;
                let shape = s.graph_shape(h)?;
                if shape.payload_bytes > BYTES || shape.references > ELEMENTS {
                    return Err(invariant("aggregate graph envelope"));
                }
                let mut bytes = Vec::new();
                reserve(&mut bytes, shape.payload_bytes)?;
                bytes.resize(shape.payload_bytes, 0);
                s.read_graph_bytes(h, 0, &mut bytes)?;
                self.evidence.graph_reads += 1;
                let mut cursor = 0;
                fn byte(bytes: &[u8], at: &mut usize) -> Fallible<u8> {
                    let b = *bytes
                        .get(*at)
                        .ok_or_else(|| invariant("truncated graph payload"))?;
                    *at += 1;
                    Ok(b)
                }
                fn word(bytes: &[u8], at: &mut usize) -> Fallible<usize> {
                    let end = at
                        .checked_add(4)
                        .ok_or_else(|| invariant("payload overflow"))?;
                    let b = bytes
                        .get(*at..end)
                        .ok_or_else(|| invariant("truncated graph word"))?;
                    *at = end;
                    Ok(
                        u32::from_le_bytes(b.try_into().map_err(|_| invariant("graph word"))?)
                            as usize,
                    )
                }
                if word(&bytes, &mut cursor)? != v.ty {
                    return Err(invariant("stored type identity mismatch"));
                }
                let case = word(&bytes, &mut cursor)?;
                let n = word(&bytes, &mut cursor)?;
                if n != shape.references {
                    return Err(invariant("stored child count"));
                }
                let mut out = Vec::new();
                reserve(&mut out, n)?;
                for i in 0..n {
                    let ty = word(&bytes, &mut cursor)?;
                    if ty >= self.p.layouts.len() {
                        return Err(invariant("stored child type"));
                    }
                    let depth = byte(&bytes, &mut cursor)? as usize;
                    if depth > WRAPPERS {
                        return Err(invariant("carrier depth"));
                    }
                    let mut carriers = [Carrier::default(); WRAPPERS];
                    for c in &mut carriers[..depth] {
                        c.ty = word(&bytes, &mut cursor)?;
                        c.case = word(&bytes, &mut cursor)?;
                    }
                    let atom = match byte(&bytes, &mut cursor)? {
                        0 => {
                            let end = cursor
                                .checked_add(8)
                                .ok_or_else(|| invariant("integer payload overflow"))?;
                            let b = bytes
                                .get(cursor..end)
                                .ok_or_else(|| invariant("integer payload"))?;
                            cursor = end;
                            Atom::Int(i64::from_le_bytes(
                                b.try_into().map_err(|_| invariant("integer bytes"))?,
                            ))
                        }
                        1 => {
                            let b = byte(&bytes, &mut cursor)?;
                            if b > 1 {
                                return Err(invariant("Boolean encoding"));
                            }
                            Atom::Bool(b == 1)
                        }
                        2 => Atom::Unit,
                        3 => Atom::Heap(
                            s.graph_reference(h, i)?
                                .ok_or_else(|| invariant("missing traced child"))?,
                        ),
                        4 => Atom::Error(byte(&bytes, &mut cursor)?),
                        _ => return Err(invariant("unknown payload tag")),
                    };
                    out.push(Value {
                        ty,
                        atom,
                        carriers,
                        depth,
                    });
                }
                if cursor != bytes.len()
                    && !(matches!(self.p.layouts[v.ty].shape, Shape::KRef(_))
                        && bytes[cursor..].iter().all(|b| *b == 0))
                {
                    return Err(invariant("trailing payload bytes"));
                }
                Ok((case, out))
            }
            fn count(
                &mut self,
                v: Value,
                s: &Session,
                seen: &mut HashSet<Handle>,
                count: &mut usize,
                _depth: usize,
            ) -> Fallible<()> {
                fn enqueue(
                    mut value: Value,
                    queue: &mut Vec<Value>,
                    seen: &mut HashSet<Handle>,
                    count: &mut usize,
                ) -> Fallible<()> {
                    *count = count
                        .checked_add(value.depth)
                        .ok_or(Fault::allocation(AllocationError::ShapeLimit))?;
                    while value.depth > 0 {
                        value = value.unwrap().1;
                    }
                    let fresh = if let Some(h) = value.handle() {
                        if seen.contains(&h) {
                            false
                        } else {
                            seen.try_reserve(1)
                                .map_err(|_| Fault::allocation(AllocationError::HostAllocation))?;
                            seen.insert(h);
                            true
                        }
                    } else {
                        true
                    };
                    if fresh {
                        *count = count
                            .checked_add(1)
                            .ok_or(Fault::allocation(AllocationError::ShapeLimit))?;
                    }
                    if *count > NODES {
                        return Err(Fault::allocation(AllocationError::ShapeLimit));
                    }
                    if fresh && value.handle().is_some() {
                        queue
                            .try_reserve(1)
                            .map_err(|_| Fault::allocation(AllocationError::HostAllocation))?;
                        queue.push(value);
                    }
                    Ok(())
                }
                let mut queue = Vec::new();
                enqueue(v, &mut queue, seen, count)?;
                while let Some(value) = queue.pop() {
                    if self.p.layouts[value.ty].shape != Shape::KString {
                        for child in self.parts(value, s)?.1 {
                            enqueue(child, &mut queue, seen, count)?;
                        }
                    }
                }
                Ok(())
            }
            fn snapshot(
                &mut self,
                v: Value,
                s: &Session,
                count: &mut usize,
                depth: usize,
            ) -> Fallible<Snapshot> {
                let mut bytes = 0;
                self.snapshot_bounded(v, s, count, &mut bytes, depth)
            }
            fn snapshot_bounded(
                &mut self,
                v: Value,
                s: &Session,
                count: &mut usize,
                bytes: &mut usize,
                depth: usize,
            ) -> Fallible<Snapshot> {
                self.step(Loc::default())?;
                *count += 1;
                if *count > NODES || depth >= 64 {
                    return Err(primary(
                        "CGR010",
                        "materialized value bound exceeded",
                        Loc::default(),
                    ));
                }
                if v.depth > 0 {
                    let name = self.p.strings[self.p.layouts[v.ty].name].clone();
                    let Shape::KVariant(cases) = &self.p.layouts[v.ty].shape else {
                        return Err(invariant("carrier layout"));
                    };
                    let (c, p) = v.unwrap();
                    let case = self.p.strings[cases[c].0].clone();
                    display_charge(bytes, name.len() + case.len() + 3)?;
                    return Ok(Snapshot::Variant(
                        name,
                        case,
                        vec![self.snapshot_bounded(p, s, count, bytes, depth + 1)?],
                    ));
                }
                match v.atom {
                    Atom::Int(n) => {
                        display_charge(bytes, n.to_string().len())?;
                        Ok(Snapshot::Integer(n))
                    }
                    Atom::Bool(b) => {
                        display_charge(bytes, if b { 4 } else { 5 })?;
                        Ok(Snapshot::Boolean(b))
                    }
                    Atom::Unit => Ok(Snapshot::Unit),
                    Atom::Error(c) => {
                        display_charge(bytes, 23 + ERRORS[c as usize].len() + 3)?;
                        Ok(Snapshot::Variant(
                            "cgx2.core.AllocationError".into(),
                            ERRORS[c as usize].into(),
                            vec![],
                        ))
                    }
                    Atom::Heap(_) => match &self.p.layouts[v.ty].shape {
                        Shape::KString => {
                            display_charge(
                                bytes,
                                s.graph_shape(
                                    v.handle().ok_or_else(|| invariant("string handle"))?,
                                )?
                                .payload_bytes,
                            )?;
                            Ok(Snapshot::Text(self.text(v, s)?))
                        }
                        Shape::KRef(_) => Err(primary(
                            "CGR091",
                            "reference-bearing public snapshot is unsupported",
                            Loc::default(),
                        )),
                        Shape::KRecord(fields) => {
                            let name = self.p.strings[self.p.layouts[v.ty].name].clone();
                            let values = self.parts(v, s)?.1;
                            display_charge(
                                bytes,
                                name.len()
                                    + 2
                                    + values.len().saturating_sub(1) * 2
                                    + fields
                                        .iter()
                                        .map(|(n, _)| self.p.strings[*n].len() + 2)
                                        .sum::<usize>(),
                            )?;
                            let mut out = Vec::new();
                            reserve(&mut out, values.len())?;
                            for ((n, _), v) in fields.iter().zip(values) {
                                out.push((
                                    self.p.strings[*n].clone(),
                                    self.snapshot_bounded(v, s, count, bytes, depth + 1)?,
                                ));
                            }
                            Ok(Snapshot::Record(name, out))
                        }
                        Shape::KVariant(cases) => {
                            let name = self.p.strings[self.p.layouts[v.ty].name].clone();
                            let (c, values) = self.parts(v, s)?;
                            let case = self.p.strings[cases[c].0].clone();
                            display_charge(
                                bytes,
                                name.len() + case.len() + 3 + values.len().saturating_sub(1) * 2,
                            )?;
                            let mut out = Vec::new();
                            reserve(&mut out, values.len())?;
                            for v in values {
                                out.push(self.snapshot_bounded(v, s, count, bytes, depth + 1)?);
                            }
                            Ok(Snapshot::Variant(name, case, out))
                        }
                        Shape::KList(_) | Shape::KArray(_) => {
                            let array = matches!(self.p.layouts[v.ty].shape, Shape::KArray(_));
                            let values = self.parts(v, s)?.1;
                            display_charge(
                                bytes,
                                2 + if array { 5 } else { 0 } + values.len().saturating_sub(1) * 2,
                            )?;
                            let mut out = Vec::new();
                            reserve(&mut out, values.len())?;
                            for v in values {
                                out.push(self.snapshot_bounded(v, s, count, bytes, depth + 1)?);
                            }
                            Ok(Snapshot::Sequence(array, out))
                        }
                        _ => Err(invariant("heap scalar layout")),
                    },
                }
            }
            fn equal(
                &mut self,
                a: Value,
                b: Value,
                s: &Session,
                seen: &mut HashSet<(Handle, Handle)>,
                nodes: &mut usize,
                depth: usize,
            ) -> Fallible<bool> {
                self.step(Loc::default())?;
                if a.ty != b.ty || a.depth != b.depth {
                    return Ok(false);
                }
                *nodes += 1;
                if *nodes > NODES || depth >= 64 {
                    return Err(primary(
                        "CGR010",
                        "equality work bound exceeded",
                        Loc::default(),
                    ));
                }
                if a.depth > 0 {
                    let (c, a) = a.unwrap();
                    let (d, b) = b.unwrap();
                    return if c != d {
                        Ok(false)
                    } else {
                        self.equal(a, b, s, seen, nodes, depth + 1)
                    };
                }
                match (a.atom, b.atom) {
                    (Atom::Heap(x), Atom::Heap(y)) => {
                        if x == y {
                            return Ok(true);
                        }
                        if seen.contains(&(x, y)) {
                            return Ok(true);
                        }
                        seen.try_reserve(1)
                            .map_err(|_| Fault::allocation(AllocationError::HostAllocation))?;
                        seen.insert((x, y));
                        if self.p.layouts[a.ty].shape == Shape::KString {
                            return Ok(self.text(a, s)? == self.text(b, s)?);
                        }
                        let (c, aa) = self.parts(a, s)?;
                        let (d, bb) = self.parts(b, s)?;
                        if c != d || aa.len() != bb.len() {
                            return Ok(false);
                        }
                        for (a, b) in aa.into_iter().zip(bb) {
                            if !self.equal(a, b, s, seen, nodes, depth + 1)? {
                                return Ok(false);
                            }
                        }
                        Ok(true)
                    }
                    (a, b) => Ok(a == b),
                }
            }
            fn binary(
                &mut self,
                a: Value,
                b: Value,
                op: usize,
                loc: Loc,
                s: &Session,
            ) -> Fallible<Value> {
                if (1..=4).contains(&op) {
                    let a = integer(a)?;
                    let b = integer(b)?;
                    if op == 4 && b == 0 {
                        return Err(primary("CGR006", "division by zero", loc));
                    }
                    let n = match op {
                        1 => a.checked_add(b),
                        2 => a.checked_sub(b),
                        3 => a.checked_mul(b),
                        _ => a.checked_div(b),
                    }
                    .ok_or_else(|| primary("CGR005", "i64 arithmetic overflow", loc))?;
                    return Ok(self.int(n));
                }
                let boolean_ty = self
                    .p
                    .layouts
                    .iter()
                    .position(|t| t.shape == Shape::KBool)
                    .ok_or_else(|| invariant("Bool layout"))?;
                let result = match op {
                    5 | 6 => {
                        let mut n = 0;
                        let same = self.equal(a, b, s, &mut HashSet::new(), &mut n, 0)?;
                        if op == 5 {
                            same
                        } else {
                            !same
                        }
                    }
                    7 => integer(a)? < integer(b)?,
                    8 => integer(a)? <= integer(b)?,
                    9 => integer(a)? > integer(b)?,
                    10 => integer(a)? >= integer(b)?,
                    _ => return Err(invariant("binary opcode")),
                };
                Ok(Value::new(boolean_ty, Atom::Bool(result)))
            }
            fn option(&mut self, ty: usize, v: Option<Value>, s: &mut Session) -> Fallible<Value> {
                match v {
                    Some(v) => self.construct(ty, 0, &[v], s),
                    None => self.construct(ty, 1, &[], s),
                }
            }
            fn data(
                &mut self,
                op: usize,
                ty: usize,
                vs: Vec<Value>,
                s: &mut Session,
                loc: Loc,
            ) -> Fallible<Value> {
                match op {
                    0 => {
                        let n = if self.p.layouts[vs[0].ty].shape == Shape::KString {
                            self.text(vs[0], s)?.chars().count()
                        } else {
                            self.parts(vs[0], s)?.1.len()
                        };
                        Ok(self.int(n as i64))
                    }
                    1 => {
                        let index = integer(vs[1])?;
                        let value = if self.p.layouts[vs[0].ty].shape == Shape::KString {
                            let text = self.text(vs[0], s)?;
                            if let Some(c) = usize::try_from(index)
                                .ok()
                                .and_then(|i| text.chars().nth(i))
                            {
                                let string_ty = vs[0].ty;
                                Some(self.string(string_ty, &c.to_string(), s)?)
                            } else {
                                None
                            }
                        } else {
                            let values = self.parts(vs[0], s)?.1;
                            usize::try_from(index)
                                .ok()
                                .and_then(|i| values.get(i).copied())
                        };
                        if let Some(v) = value {
                            let mut r = s.scoped_roots();
                            root(&mut r, v, &mut self.evidence)?;
                            let out = self.option(ty, Some(v), &mut r);
                            let o = r.finish(out);
                            cleanup(&mut self.evidence, o.cleanup);
                            o.result
                        } else {
                            self.option(ty, None, s)
                        }
                    }
                    2 => {
                        let mut values = self.parts(vs[0], s)?.1;
                        let index = integer(vs[1])?;
                        if let Some(slot) =
                            usize::try_from(index).ok().and_then(|i| values.get_mut(i))
                        {
                            *slot = vs[2];
                            let updated = self.construct(vs[0].ty, 0, &values, s)?;
                            let mut r = s.scoped_roots();
                            root(&mut r, updated, &mut self.evidence)?;
                            let result = self.option(ty, Some(updated), &mut r);
                            let out = r.finish(result);
                            cleanup(&mut self.evidence, out.cleanup);
                            out.result
                        } else {
                            self.option(ty, None, s)
                        }
                    }
                    3 => {
                        let mut values = self.parts(vs[0], s)?.1;
                        reserve(&mut values, 1)?;
                        values.push(vs[1]);
                        self.construct(ty, 0, &values, s)
                    }
                    4 | 5 => {
                        let values = self.parts(vs[0], s)?.1;
                        self.construct(ty, 0, &values, s)
                    }
                    6 => {
                        let mut a = self.text(vs[0], s)?;
                        let b = self.text(vs[1], s)?;
                        let len = a
                            .len()
                            .checked_add(b.len())
                            .ok_or(Fault::allocation(AllocationError::ShapeLimit))?;
                        if len > BYTES {
                            return Err(Fault::allocation(AllocationError::ShapeLimit));
                        }
                        a.try_reserve_exact(b.len())
                            .map_err(|_| Fault::allocation(AllocationError::HostAllocation))?;
                        a.push_str(&b);
                        self.string(ty, &a, s)
                    }
                    7 => {
                        let text = self.text(vs[0], s)?;
                        let a = integer(vs[1])?;
                        let b = integer(vs[2])?;
                        if a < 0 || b < a || b as usize > text.chars().count() {
                            return self.option(ty, None, s);
                        }
                        let result = text
                            .chars()
                            .skip(a as usize)
                            .take((b - a) as usize)
                            .collect::<String>();
                        let v = self.string(vs[0].ty, &result, s)?;
                        let mut r = s.scoped_roots();
                        root(&mut r, v, &mut self.evidence)?;
                        let result = self.option(ty, Some(v), &mut r);
                        let out = r.finish(result);
                        cleanup(&mut self.evidence, out.cleanup);
                        out.result
                    }
                    8 => {
                        let text = self.text(vs[0], s)?;
                        let digits = text
                            .strip_prefix('-')
                            .or_else(|| text.strip_prefix('+'))
                            .unwrap_or(&text);
                        let parsed =
                            if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                                text.parse::<i64>().ok()
                            } else {
                                None
                            };
                        if let Some(n) = parsed {
                            self.construct(ty, 0, &[self.int(n)], s)
                        } else {
                            let error_ty = match &self.p.layouts[ty].shape {
                                Shape::KVariant(c) => c[1].1[0],
                                _ => return Err(invariant("parse Result")),
                            };
                            let v = self.string(error_ty, "invalid integer", s)?;
                            let mut r = s.scoped_roots();
                            root(&mut r, v, &mut self.evidence)?;
                            let result = self.construct(ty, 1, &[v], &mut r);
                            let out = r.finish(result);
                            cleanup(&mut self.evidence, out.cleanup);
                            out.result
                        }
                    }
                    9 => {
                        let mut count = 0;
                        let shown = self.snapshot(vs[0], s, &mut count, 0)?.to_string();
                        if shown.len() > BYTES {
                            return Err(primary("CGR010", "output exceeds 1048576 bytes", loc));
                        }
                        self.string(ty, &shown, s)
                    }
                    10 => Ok(self.int(self.args.len() as i64)),
                    11 => {
                        let i = integer(vs[0])?;
                        let index = usize::try_from(i)
                            .map_err(|_| primary("CGR007", "arg index must be nonnegative", loc))?;
                        let text = self.args.get(index).ok_or_else(|| {
                            primary("CGR007", format!("arg index {index} is out of range"), loc)
                        })?;
                        self.string(ty, text, s)
                    }
                    _ => Err(invariant("data intrinsic opcode")),
                }
            }
        }
        fn display_charge(bytes: &mut usize, n: usize) -> Fallible<()> {
            *bytes = bytes
                .checked_add(n)
                .ok_or_else(|| primary("CGR010", "output exceeds 1048576 bytes", Loc::default()))?;
            if *bytes > BYTES {
                Err(primary(
                    "CGR010",
                    "output exceeds 1048576 bytes",
                    Loc::default(),
                ))
            } else {
                Ok(())
            }
        }
        const ERRORS: [&str; 5] = [
            "BudgetExceeded",
            "RegionLimit",
            "HostAllocation",
            "InvalidRegion",
            "ShapeLimit",
        ];
        fn error_code(e: AllocationError) -> u8 {
            match e {
                AllocationError::BudgetExceeded => 0,
                AllocationError::RegionLimit => 1,
                AllocationError::HostAllocation => 2,
                AllocationError::InvalidRegion => 3,
                AllocationError::ShapeLimit => 4,
            }
        }
    }
    pub use engine::{
        run, run_function_borrowed, run_with_session, Evidence, Execution, Failure, Snapshot,
    };
}
