//! Client di `cswap`: tipi del JSON, azioni, errori ed esecuzione.

pub mod action;
pub mod client;
pub mod error;
pub mod locate;
pub mod model;
pub mod process;
pub mod secret;

pub use action::{
    AutoOptions, AutoStrategy, Confirmation, CswapAction, RunOptions, SwitchStrategy,
};
pub use client::{
    AUTO_IDLE_WAIT, ActionOutcome, AutoMessage, AutoProcess, CommandSpec, CswapClient, Timeouts,
};
pub use error::{CswapError, CswapErrorKind, CswapVersion};
pub use locate::{CSWAP_OVERRIDE_ENV, SearchEnv, locate_cswap, locate_uv};
pub use model::*;
pub use secret::SecretToken;
