//! Fixed-horizon DPS and fixed-count weapon-skill sampling from the frozen driver.
//!
//! Stored TP, action order, and the final stopping cycle deliberately retain the
//! legacy behavior; statistical reductions are shared with distribution plots.

mod dps;
mod distribution;
pub mod statistics;

pub use dps::simulate_dps;
pub use distribution::sample_distribution;

use std::sync::atomic::{AtomicBool, Ordering};

use crate::model::CalcError;

#[inline]
fn check_stop(stop: &AtomicBool, operation: &'static str) -> Result<(), CalcError> {
    if stop.load(Ordering::Relaxed) {
        Err(CalcError::Interrupted { operation })
    } else {
        Ok(())
    }
}
