//! Native ChaCha8 random sources with independent Primary and Pdif streams.
//!
//! A single u64 seed occupies key bytes 0..8 in little-endian order; the
//! remaining key bytes are zero. Primary uses stream 0 and Pdif uses stream 1.
//! Uniform draws consume one u64 and retain its high 53 bits. Bounded draws use
//! masked rejection on Primary; reseeding resets both streams together.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use crate::model::{CalcError, RandomSource, RandomStream};

mod tape;
pub use tape::{ScriptedTape, TapeRecord};

/// Independently owned ChaCha8 streams, including when cloned for a worker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RandomSources {
    primary: ChaCha8Rng,
    pdif: ChaCha8Rng,
}

impl RandomSources {
    pub fn from_seed(seed: u64) -> Self {
        let mut key = [0_u8; 32];
        key[..8].copy_from_slice(&seed.to_le_bytes());
        let mut primary = ChaCha8Rng::from_seed(key);
        primary.set_stream(0);
        let mut pdif = ChaCha8Rng::from_seed(key);
        pdif.set_stream(1);
        Self { primary, pdif }
    }

    /// A blank production seed comes from OS entropy.
    pub fn from_entropy() -> Result<Self, CalcError> {
        let mut bytes = [0_u8; 8];
        getrandom::fill(&mut bytes).map_err(|error| CalcError::Input {
            operation: "rng.from_entropy", field: "OS entropy", message: error.to_string(),
        })?;
        Ok(Self::from_seed(u64::from_le_bytes(bytes)))
    }
}

#[inline]
fn unit(random: &mut ChaCha8Rng) -> f64 {
    (random.next_u64() >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
}

impl RandomSource for RandomSources {
    #[inline]
    fn uniform(&mut self, stream: RandomStream, low: f64, high: f64) -> f64 {
        let random = match stream {
            RandomStream::Primary => &mut self.primary,
            RandomStream::Pdif => &mut self.pdif,
        };
        // Do not fuse/reassociate this expression, or skip a zero-width draw.
        low + (high - low) * unit(random)
    }

    fn bounded(&mut self, upper_exclusive: usize) -> usize {
        assert!(upper_exclusive > 0, "bounded random range must be nonempty");
        if upper_exclusive == 1 { return 0; }
        let upper = upper_exclusive as u64;
        let mask = u64::MAX >> (upper - 1).leading_zeros();
        loop {
            let value = self.primary.next_u64() & mask;
            if value < upper { return value as usize; }
        }
    }

    fn reseed(&mut self, seed: u64) {
        *self = Self::from_seed(seed);
    }
}
