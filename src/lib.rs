//! Physical Device
//!
//! A `PhysicalDevice` represents a (usually) physical machine instance with a CPU.
//! It can be used to query feature support and available resources for the device.

mod device;
mod properties;

pub use crate::device::*;
pub use crate::properties::*;
