//! Optional, test-side measurements of comparisons already made by the adapters.
//! Collection is disabled for ordinary tests and never changes acceptance rules.
use std::cell::RefCell;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use wsdist_core::model::LegacyF64;

fn serialize_number<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    LegacyF64(*value).serialize(serializer)
}

fn deserialize_number<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    LegacyF64::deserialize(deserializer).map(|value| value.0)
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct Measurements {
    pub comparisons: u64,
    pub exact_numeric_comparisons: u64,
    pub continuous_comparisons: u64,
    pub nonfinite_comparisons: u64,
    pub finite_expected_nonfinite_actual_comparisons: u64,
    #[serde(serialize_with = "serialize_number", deserialize_with = "deserialize_number")]
    pub maximum_absolute_error: f64,
    pub maximum_absolute_error_path: Option<String>,
    #[serde(serialize_with = "serialize_number", deserialize_with = "deserialize_number")]
    pub maximum_tolerance_fraction: f64,
    pub maximum_tolerance_fraction_path: Option<String>,
}

thread_local! {
    static ACTIVE: RefCell<Option<Measurements>> = const { RefCell::new(None) };
}

pub fn begin() { ACTIVE.with(|active| *active.borrow_mut() = Some(Measurements::default())); }

pub fn record(actual: f64, expected: f64, exact: bool, path: &str) {
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        let Some(measurements) = active.as_mut() else { return };
        measurements.comparisons += 1;
        if !expected.is_finite() || !actual.is_finite() {
            measurements.nonfinite_comparisons += 1;
            measurements.finite_expected_nonfinite_actual_comparisons += u64::from(expected.is_finite() && !actual.is_finite());
            return;
        }
        if exact || expected == 0.0 {
            measurements.exact_numeric_comparisons += 1;
            return;
        }
        measurements.continuous_comparisons += 1;
        let error = (actual - expected).abs();
        let fraction = error / (1e-9 + 1e-12 * actual.abs().max(expected.abs()));
        if measurements.maximum_absolute_error_path.is_none() || error > measurements.maximum_absolute_error {
            measurements.maximum_absolute_error = error;
            measurements.maximum_absolute_error_path = Some(path.to_owned());
        }
        if measurements.maximum_tolerance_fraction_path.is_none() || fraction > measurements.maximum_tolerance_fraction {
            measurements.maximum_tolerance_fraction = fraction;
            measurements.maximum_tolerance_fraction_path = Some(path.to_owned());
        }
    });
}

pub fn take() -> Value {
    ACTIVE.with(|active| serde_json::to_value(active.borrow_mut().take().unwrap_or_default()).unwrap())
}

impl Measurements {
    pub fn merge(&mut self, value: Value) {
        let other: Self = serde_json::from_value(value).expect("adapter measurements");
        self.comparisons += other.comparisons;
        self.exact_numeric_comparisons += other.exact_numeric_comparisons;
        self.continuous_comparisons += other.continuous_comparisons;
        self.nonfinite_comparisons += other.nonfinite_comparisons;
        self.finite_expected_nonfinite_actual_comparisons += other.finite_expected_nonfinite_actual_comparisons;
        if other.maximum_absolute_error_path.is_some()
            && (self.maximum_absolute_error_path.is_none() || other.maximum_absolute_error > self.maximum_absolute_error)
        {
            self.maximum_absolute_error = other.maximum_absolute_error;
            self.maximum_absolute_error_path = other.maximum_absolute_error_path;
        }
        if other.maximum_tolerance_fraction_path.is_some()
            && (self.maximum_tolerance_fraction_path.is_none() || other.maximum_tolerance_fraction > self.maximum_tolerance_fraction)
        {
            self.maximum_tolerance_fraction = other.maximum_tolerance_fraction;
            self.maximum_tolerance_fraction_path = other.maximum_tolerance_fraction_path;
        }
    }
}
