//! Headless verification helpers.
//!
//! These types do not change parser, animation, or renderer semantics.
//! Unmodeled MDX blocks are reported as such instead of being written as 0.

mod inspect;
mod snapshot;

pub use inspect::{inspect_mdx, InspectError};
pub use snapshot::{dump_structure, Count};

mod tests;
