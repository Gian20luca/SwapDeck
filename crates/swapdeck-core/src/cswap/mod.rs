//! Client di `cswap`: tipi del JSON, azioni, errori ed esecuzione.

pub mod action;
pub mod error;
pub mod model;
pub mod secret;

pub use action::{
    AutoOptions, AutoStrategy, Confirmation, CswapAction, RunOptions, SwitchStrategy,
};
pub use error::{CswapError, CswapErrorKind, CswapVersion};
pub use model::*;
pub use secret::SecretToken;
