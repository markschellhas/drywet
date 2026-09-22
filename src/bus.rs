use std::error::Error;
use std::fmt;
use std::rc::Rc;

use crate::insert::{Insert, InsertError};
use crate::limits::MAX_BUSES;

/// Extra named mix destination on a Context sink.
///
/// Created with [`crate::Context::bus`]. Same name returns the same bus for
/// the Context lifetime. Dropping this handle does not destroy the bus.
#[derive(Clone)]
pub struct Bus {
    id: BusId,
    name: String,
    control: Rc<dyn BusControl>,
}

impl Bus {
    pub(crate) fn new(id: BusId, name: impl Into<String>, control: Rc<dyn BusControl>) -> Self {
        Self {
            id,
            name: name.into(),
            control,
        }
    }

    /// Slot index among extra buses (not master).
    pub fn id(&self) -> BusId {
        self.id
    }

    /// Name passed to [`crate::Context::bus`].
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Replace this bus's insert chain. Mix onto the bus stays dry.
    pub fn set_inserts(
        &self,
        inserts: impl IntoIterator<Item = Box<dyn Insert>>,
    ) -> Result<(), InsertError> {
        self.control
            .set_inserts(self.id, inserts.into_iter().collect())
    }

    /// Clear this bus's insert chain (identity).
    pub fn clear_inserts(&self) -> Result<(), InsertError> {
        self.set_inserts(Vec::new())
    }
}

impl fmt::Debug for Bus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bus")
            .field("id", &self.id)
            .field("name", &self.name)
            .finish()
    }
}

/// Index of an extra named bus (`0..MAX_BUSES`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BusId(u8);

impl BusId {
    pub(crate) fn new(index: u8) -> Self {
        Self(index)
    }

    /// Zero-based slot among extra buses.
    pub fn index(self) -> usize {
        usize::from(self.0)
    }
}

/// Mix destination: implicit master or a named extra bus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MixDest {
    /// Context sink / master dry store.
    Master,
    /// Extra named bus created with [`crate::Context::bus`].
    Bus(BusId),
}

impl Default for MixDest {
    fn default() -> Self {
        Self::Master
    }
}

/// Failure creating or mixing onto a bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusError {
    /// Creating another extra bus would exceed [`MAX_BUSES`].
    BusFull {
        /// Maximum extra named buses (not counting master).
        max: usize,
        /// Number of extra buses the rejected create would have produced.
        got: usize,
    },
    /// Empty name or reserved `"master"`.
    InvalidName,
    /// Destination is not a live bus on this sink.
    UnknownBus,
}

impl fmt::Display for BusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BusError::BusFull { max, got } => {
                write!(f, "cannot exceed {max} extra buses, got {got}")
            }
            BusError::InvalidName => {
                write!(f, "bus name is empty or reserved (\"master\")")
            }
            BusError::UnknownBus => write!(f, "unknown bus"),
        }
    }
}

impl Error for BusError {}

pub(crate) trait BusControl {
    fn set_inserts(&self, id: BusId, inserts: Vec<Box<dyn Insert>>) -> Result<(), InsertError>;
}

pub(crate) fn validate_name(name: &str) -> Result<(), BusError> {
    if name.is_empty() || name.eq_ignore_ascii_case("master") {
        return Err(BusError::InvalidName);
    }
    Ok(())
}

#[derive(Debug)]
pub(crate) struct BusSlot<T> {
    pub name: String,
    pub data: T,
}

#[derive(Debug)]
pub(crate) struct BusTable<T> {
    slots: [Option<BusSlot<T>>; MAX_BUSES],
}

impl<T> BusTable<T> {
    pub fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
        }
    }

    pub fn ensure(&mut self, name: &str) -> Result<BusId, BusError>
    where
        T: Default,
    {
        validate_name(name)?;
        if let Some(id) = self.id_of(name) {
            return Ok(id);
        }
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(BusSlot {
                    name: name.to_string(),
                    data: T::default(),
                });
                return Ok(BusId::new(i as u8));
            }
        }
        Err(BusError::BusFull {
            max: MAX_BUSES,
            got: MAX_BUSES + 1,
        })
    }

    pub fn id_of(&self, name: &str) -> Option<BusId> {
        self.slots.iter().enumerate().find_map(|(i, slot)| {
            slot.as_ref()
                .filter(|s| s.name == name)
                .map(|_| BusId::new(i as u8))
        })
    }

    pub fn get(&self, id: BusId) -> Option<&BusSlot<T>> {
        self.slots.get(id.index())?.as_ref()
    }

    pub fn get_mut(&mut self, id: BusId) -> Option<&mut BusSlot<T>> {
        self.slots.get_mut(id.index())?.as_mut()
    }

    pub fn iter(&self) -> impl Iterator<Item = (BusId, &BusSlot<T>)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| slot.as_ref().map(|s| (BusId::new(i as u8), s)))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (BusId, &mut BusSlot<T>)> {
        self.slots
            .iter_mut()
            .enumerate()
            .filter_map(|(i, slot)| slot.as_mut().map(|s| (BusId::new(i as u8), s)))
    }
}

impl<T> Default for BusTable<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Add mono frames into an expanding dry store. Does not interleave.
pub(crate) fn mix_mono(buf: &mut Vec<f32>, frames: &[f32], at: usize) {
    let needed = at.saturating_add(frames.len());
    if buf.len() < needed {
        buf.resize(needed, 0.0);
    }
    for (i, &sample) in frames.iter().enumerate() {
        buf[at + i] += sample;
    }
}
