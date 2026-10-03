//! Legacy NumPy-host and compiled Python MT19937 streams.
//!
//! Scalar seeding and the 53-bit doubles are shared, but bounded integers are
//! not: NumPy masks low bits of `upper - 1`; compiled Python rejects high-bit
//! samples of `upper.bit_length()`. Their two-word integer order also differs.

use rand_mt::Mt;

use crate::model::{CalcError, RandomSource, RandomStream};

mod tape;
pub use tape::{ScriptedTape, TapeRecord};

/// Independently owned MT19937 states, including when cloned for a worker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RandomSources {
    numpy_host: Mt,
    numba_python: Mt,
}

impl RandomSources {
    pub fn from_seeds(host: u32, compiled: u32) -> Self {
        Self { numpy_host: Mt::new(host), numba_python: Mt::new(compiled) }
    }

    /// A blank production seed is OS entropy, never the MT reference seed.
    pub fn from_entropy() -> Result<Self, CalcError> {
        let mut bytes = [0_u8; 8];
        getrandom::fill(&mut bytes).map_err(|error| CalcError::Input {
            operation: "rng.from_entropy", field: "OS entropy", message: error.to_string(),
        })?;
        Ok(Self::from_seeds(
            u32::from_le_bytes(bytes[..4].try_into().expect("four seed bytes")),
            u32::from_le_bytes(bytes[4..].try_into().expect("four seed bytes")),
        ))
    }

    #[inline]
    fn state(&mut self, stream: RandomStream) -> &mut Mt {
        match stream {
            RandomStream::NumpyHost => &mut self.numpy_host,
            RandomStream::NumbaPython => &mut self.numba_python,
        }
    }

    /// Consume an actual tempered MT word. This diagnostic uses the crate's
    /// public generator API, not its private state layout.
    #[inline]
    pub fn next_u32(&mut self, stream: RandomStream) -> u32 {
        self.state(stream).next_u32()
    }

    /// NumPy `random_sample` / Numba `random.random`: two words, 27 + 26 bits.
    #[inline]
    pub fn random53(&mut self, stream: RandomStream) -> f64 {
        let state = self.state(stream);
        let a = state.next_u32() >> 5;
        let b = state.next_u32() >> 6;
        (f64::from(a) * 67_108_864.0 + f64::from(b)) / 9_007_199_254_740_992.0
    }

    /// NumPy rejects nonfinite interval widths before drawing. Compiled
    /// Python performs the arithmetic without that host-only check.
    #[inline]
    pub fn try_uniform(&mut self, stream: RandomStream, low: f64, high: f64) -> Result<f64, CalcError> {
        let width = high - low;
        if stream == RandomStream::NumpyHost && !width.is_finite() {
            return Err(CalcError::legacy("rng.uniform", "range", "OverflowError", "Range exceeds valid bounds"));
        }
        // Do not fuse/reassociate this expression, or skip a zero-width draw.
        Ok(low + width * self.random53(stream))
    }

    /// Legacy scalar `np.random.randint(upper, dtype=np.int64)` or compiled
    /// `random.randrange(upper)`. Invalid bounds do not consume a word.
    pub fn bounded(&mut self, stream: RandomStream, upper: usize) -> Result<usize, CalcError> {
        if upper == 0 {
            let message = match stream {
                RandomStream::NumpyHost => "high <= 0",
                RandomStream::NumbaPython => "empty range for randrange()",
            };
            return Err(CalcError::legacy("rng.bounded", "upper_exclusive", "ValueError", message));
        }
        let upper = upper as u64;
        match stream {
            RandomStream::NumpyHost => {
                if upper > (1_u64 << 63) {
                    return Err(CalcError::legacy("rng.bounded", "upper_exclusive", "ValueError", "high is out of bounds for int64"));
                }
                Ok(self.bounded_numpy_inner(upper) as usize)
            }
            RandomStream::NumbaPython => {
                if upper > i64::MAX as u64 {
                    return Err(CalcError::legacy("rng.bounded", "upper_exclusive", "OverflowError", "int too big to convert"));
                }
                Ok(self.bounded_python_inner(upper) as usize)
            }
        }
    }

    #[inline]
    fn bounded_numpy_inner(&mut self, upper: u64) -> u64 {
        let maximum = upper - 1;
        if maximum == 0 { return 0; }
        let mask = u64::MAX >> maximum.leading_zeros();
        let state = &mut self.numpy_host;
        loop {
            // NumPy's int64 transport still uses one word for 32-bit ranges.
            // For wider ranges mt19937_next64 takes the high word first.
            let bits = if maximum <= u64::from(u32::MAX) {
                u64::from(state.next_u32())
            } else {
                let high = u64::from(state.next_u32());
                let low = u64::from(state.next_u32());
                (high << 32) | low
            };
            let value = bits & mask;
            if value <= maximum { return value; }
        }
    }

    #[inline]
    fn bounded_python_inner(&mut self, upper: u64) -> u64 {
        // Numba randomimpl.py:454-521 deliberately uses upper, not upper-1.
        // In particular bounded(1) draws/rejects even though its result is 0.
        let bits = 64 - upper.leading_zeros();
        let state = &mut self.numba_python;
        loop {
            let value = if bits <= 32 {
                u64::from(state.next_u32() >> (32 - bits))
            } else {
                // Compiled Python takes the unshifted low word first.
                let low = u64::from(state.next_u32());
                let high = u64::from(state.next_u32() >> (64 - bits));
                (high << 32) | low
            };
            if value < upper { return value; }
        }
    }

    /// Unweighted host NumPy choice. Reject empty inputs before drawing;
    /// choosing the only item consumes no MT words.
    pub fn choose_index(&mut self, upper: usize) -> Result<usize, CalcError> {
        if upper == 0 {
            return Err(CalcError::legacy("rng.choice", "values", "ValueError", "'a' cannot be empty unless no samples are taken"));
        }
        self.bounded(RandomStream::NumpyHost, upper)
    }

    /// The same reverse Fisher-Yates walk used for NumPy lists/1D arrays and
    /// compiled Python arrays, with each stream's distinct bounded draw.
    pub fn shuffle<T>(&mut self, stream: RandomStream, values: &mut [T]) {
        for index in (1..values.len()).rev() {
            let selected = self.bounded(stream, index + 1).expect("representable slice length");
            values.swap(index, selected);
        }
    }

    pub fn reseed(&mut self, stream: RandomStream, seed: u32) {
        self.state(stream).reseed(seed);
    }
}

impl RandomSource for RandomSources {
    #[inline]
    fn uniform(&mut self, stream: RandomStream, low: f64, high: f64) -> f64 {
        self.try_uniform(stream, low, high).unwrap_or_else(|error| panic!("{error}"))
    }

    #[inline]
    fn bounded_numpy(&mut self, upper_exclusive: usize) -> usize {
        self.bounded(RandomStream::NumpyHost, upper_exclusive).unwrap_or_else(|error| panic!("{error}"))
    }

    fn reseed_numpy(&mut self, seed: u32) {
        self.reseed(RandomStream::NumpyHost, seed);
    }
}
