//! NumPy 2.4.6 binary64 reductions and equal-width histogram semantics.

use crate::math::python_int;
use crate::model::{CalcError, DistributionResult, Histogram};

// numpy/_core/src/umath/loops_utils.h.src: DOUBLE_pairwise_sum.
fn pairwise_sum(values: &[f64]) -> f64 {
    let n = values.len();
    if n < 8 {
        let mut result = -0.0;
        for &value in values { result += value; }
        result
    } else if n <= 128 {
        let mut partial: [f64; 8] = values[..8].try_into().unwrap();
        let unrolled = n - n % 8;
        for chunk in values[8..unrolled].chunks_exact(8) {
            for index in 0..8 { partial[index] += chunk[index]; }
        }
        let mut result = ((partial[0] + partial[1]) + (partial[2] + partial[3]))
            + ((partial[4] + partial[5]) + (partial[6] + partial[7]));
        for &value in &values[unrolled..] { result += value; }
        result
    } else {
        let mut middle = n / 2;
        middle -= middle % 8;
        pairwise_sum(&values[..middle]) + pairwise_sum(&values[middle..])
    }
}

pub fn numpy_mean(values: &[f64]) -> f64 {
    // The add reduction begins with its +0 identity, including all-negative-zero
    // arrays. Contiguous arrays use one recursive block, not 8192-item chunks.
    (0.0 + pairwise_sum(values)) / values.len() as f64
}

fn extrema(values: &[f64]) -> Option<(f64, f64)> {
    let (&first, rest) = values.split_first()?;
    let (mut minimum, mut maximum) = (first, first);
    for &value in rest {
        if value.is_nan() { return Some((f64::NAN, f64::NAN)); }
        if value <= minimum { minimum = value; }
        if value >= maximum { maximum = value; }
    }
    if first.is_nan() { Some((f64::NAN, f64::NAN)) } else { Some((minimum, maximum)) }
}

fn numpy_float_text(value: f64) -> String {
    if value.is_nan() { "nan".to_owned() }
    else if value == f64::INFINITY { "inf".to_owned() }
    else if value == f64::NEG_INFINITY { "-inf".to_owned() }
    else { format!("{value:?}") }
}

pub fn histogram(values: &[f64], bins: usize) -> Result<Histogram, CalcError> {
    if bins == 0 {
        return Err(CalcError::legacy("histogram", "bins", "ValueError", "`bins` must be positive, when an integer"));
    }
    let (mut first, mut last) = extrema(values).unwrap_or((0.0, 1.0));
    if !first.is_finite() || !last.is_finite() {
        return Err(CalcError::legacy("histogram", "range", "ValueError", format!(
            "autodetected range of [{}, {}] is not finite", numpy_float_text(first), numpy_float_text(last),
        )));
    }
    if first == last { first -= 0.5; last += 0.5; }
    let width = last - first;
    let step = width / bins as f64;
    let mut edges = Vec::with_capacity(bins + 1);
    for index in 0..=bins {
        // numpy.linspace handles an underflowed step by dividing first.
        let offset = if step == 0.0 { (index as f64 / bins as f64) * width }
            else { index as f64 * step };
        edges.push(offset + first);
    }
    edges[bins] = last;
    if edges.windows(2).any(|edge| edge[0] >= edge[1]) {
        return Err(CalcError::legacy("histogram", "bins", "ValueError", format!(
            "Too many bins for data range. Cannot create {bins} finite-sized bins.",
        )));
    }
    let mut counts = vec![0_u64; bins];
    for &value in values {
        // NumPy's fast uniform-bin path: divide then multiply, correct by the
        // actual linspace edge. Floor alone misclassifies adjacent f64 values.
        let index = ((value - first) / width) * bins as f64;
        if !index.is_finite() {
            return Err(CalcError::legacy("histogram", "indices", "IndexError", format!(
                "index -9223372036854775808 is out of bounds for axis 0 with size {}", bins + 1)));
        }
        let mut index = index as usize;
        if index == bins { index -= 1; }
        if value < edges[index] { index -= 1; }
        if index != bins - 1 && value >= edges[index + 1] { index += 1; }
        counts[index] += 1;
    }
    // numpy.histogram's density is n / diff(edges) / n.sum(), in that order.
    let total = counts.iter().sum::<u64>() as f64;
    let density = counts.iter().zip(edges.windows(2)).map(|(&count, edge)|
        count as f64 / (edge[1] - edge[0]) / total).collect();
    Ok(Histogram { counts, density, edges })
}

pub fn summarize_distribution(damage: Vec<f64>, tp: Vec<f64>) -> Result<DistributionResult, CalcError> {
    let histogram = histogram(&damage, 300)?;
    let mean = numpy_mean(&damage);
    let title_mean = python_int(mean, "distribution", "mean title")?;
    let (minimum, maximum) = extrema(&damage).ok_or_else(|| CalcError::legacy(
        "distribution", "minimum", "ValueError", "zero-size array to reduction operation minimum which has no identity",
    ))?;
    // Preserve sample order in the returned data. Only the median scratch copy
    // is partitioned; sorting the complete sample array is unnecessary.
    let mut ordered = damage.clone();
    let middle = ordered.len() / 2;
    let (lower_half, upper, _) = ordered.select_nth_unstable_by(middle, f64::total_cmp);
    let upper = *upper;
    let median = if damage.len() % 2 == 1 { numpy_mean(&[upper]) }
        else {
            // select_nth's prefix is below upper; its maximum is the other
            // middle value. Avoid a second partition or full sorting.
            let lower = lower_half.iter().copied().max_by(f64::total_cmp).unwrap();
            numpy_mean(&[lower, upper])
        };
    let title_median = python_int(median, "distribution", "median title")?;
    Ok(DistributionResult { damage, tp, histogram, minimum, mean, median, maximum, title_mean, title_median })
}
