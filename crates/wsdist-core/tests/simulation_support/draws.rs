use serde_json::Value;
use wsdist_core::math::{PdifSkill, get_pdif_melee, get_pdif_ranged};
use wsdist_core::model::{RandomSource, RandomStream};
use wsdist_core::rng::RandomSources;

use crate::support::{assert_number, assert_numpy_state, number};

#[derive(Clone, Debug)]
struct Draw {
    stream: RandomStream,
    low: f64,
    high: f64,
    value: f64,
}

#[derive(Clone, Debug)]
pub struct ObservedRandom {
    pub inner: RandomSources,
    draws: Vec<Draw>,
}

impl ObservedRandom {
    pub fn draw_count(&self) -> usize { self.draws.len() }

    pub fn new(seeds: &Value) -> Self {
        Self {
            inner: RandomSources::from_seeds(
                seeds["numpy_host"].as_u64().unwrap() as u32,
                seeds["numba_python"].as_u64().unwrap() as u32,
            ),
            draws: Vec::new(),
        }
    }

    pub fn assert_evidence(&self, seeds: &Value, evidence: &Value, label: &str) {
        // The corpus observes host uniforms directly and compiled kernel calls
        // (including their actual Numba results), not fabricated kernel tapes.
        // Replay that observed logical draw walk to also establish the final
        // compiled stream without importing any generator's private state.
        let mut expected = Self::new(seeds);
        for entry in evidence["ordered_draws_and_compiled_kernel_outputs"].as_array().unwrap() {
            let stream = entry[0].as_str().unwrap();
            let operation = entry[1].as_str().unwrap();
            let args = entry[2].as_array().unwrap();
            let keywords = entry[3].as_object().unwrap();
            match (stream, operation) {
                ("NumpyHost", "uniform") => {
                    let low = args.first().or_else(|| keywords.get("low")).map_or(0.0, number);
                    let high = args.get(1).or_else(|| keywords.get("high")).map_or(1.0, number);
                    assert!(args.len() <= 2 && !keywords.contains_key("size"), "{label}: scalar source uniform");
                    let value = expected.uniform(RandomStream::NumpyHost, low, high);
                    crate::support::observation::record(value, number(&entry[4]), true, label);
                    assert_eq!(value.to_bits(), number(&entry[4]).to_bits(), "{label}: exact host variate");
                }
                ("NumbaPython", "get_pdif_melee" | "get_pdif_ranged") => {
                    let skill = PdifSkill::from_name(args[1].as_str().unwrap());
                    let result = if operation == "get_pdif_melee" {
                        get_pdif_melee(number(&args[0]), skill, number(&args[2]), number(&args[3]),
                            number(&args[4]), number(&args[5]), &mut expected)
                    } else {
                        get_pdif_ranged(number(&args[0]), skill, number(&args[2]), number(&args[3]),
                            number(&args[4]), number(&args[5]), &mut expected)
                    }.unwrap();
                    assert_number(result.0, &entry[4][0], label);
                    assert_eq!(result.1, entry[4][1].as_bool().unwrap(), "{label}: captured compiled crit");
                }
                // Resist uniforms appear as their own host observations before
                // the kernel record; do not consume them a second time.
                ("kernel", "get_phys_damage" | "get_tp" | "get_resist_state") => {}
                _ => panic!("{label}: unhandled captured draw {stream}/{operation}"),
            }
        }
        assert_eq!(self.draws.len(), expected.draws.len(), "{label}: exact total stream draw count");
        for (index, (actual, expected)) in self.draws.iter().zip(&expected.draws).enumerate() {
            assert_eq!(actual.stream, expected.stream, "{label}: draw {index} stream/order");
            assert_eq!(actual.low.to_bits(), expected.low.to_bits(), "{label}: draw {index} lower parameter bits");
            assert_eq!(actual.high.to_bits(), expected.high.to_bits(), "{label}: draw {index} upper parameter bits");
            crate::support::observation::record(actual.value, expected.value, true, label);
            assert_eq!(actual.value.to_bits(), expected.value.to_bits(), "{label}: draw {index} exact variate");
        }
        assert_eq!(self.inner, expected.inner, "{label}: final host and compiled MT19937 states");
        assert_numpy_state(&self.inner, &evidence["numpy_host_final_state"], label);
    }
}

impl RandomSource for ObservedRandom {
    fn uniform(&mut self, stream: RandomStream, low: f64, high: f64) -> f64 {
        let value = self.inner.uniform(stream, low, high);
        self.draws.push(Draw { stream, low, high, value });
        value
    }

    fn bounded_numpy(&mut self, _: usize) -> usize {
        panic!("simulations must not add choice/shuffle draws");
    }

    fn reseed_numpy(&mut self, _: u32) {
        panic!("simulations must not reseed either stream");
    }
}
