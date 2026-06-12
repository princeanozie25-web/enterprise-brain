//! M4: the proposal-only agent. An agent principal runs standing queries
//! through the EXISTING governed pipeline at its intersection scope and
//! emits proposals. It executes nothing, mutates nothing, approves nothing.

pub mod context;
pub mod proposals;
pub mod runner;
pub mod standing;
