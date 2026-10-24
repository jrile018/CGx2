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
