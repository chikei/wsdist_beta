/// CPython's `round(x, 1)`: round the exact binary input to a decimal tenth,
/// using ties-to-even. Scaling the input in binary first is not equivalent.
#[inline]
pub fn python_round1(value: f64) -> f64 {
    if !value.is_finite() || value == 0.0 || value.abs() >= 4_503_599_627_370_496.0 {
        return value;
    }
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1_u64 << 52) - 1);
    let (mantissa, binary_exponent) = if exponent == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1_u64 << 52), exponent - 1023 - 52)
    };
    // Exact value * 10 = (mantissa * 5) * 2^(binary_exponent + 1).
    // The early large-value return bounds this product to fewer than 56 bits.
    let numerator = mantissa * 5;
    let shift = binary_exponent + 1;
    let rounded = if shift >= 0 {
        numerator << shift as u32
    } else {
        let denominator_bits = (-shift) as u32;
        if denominator_bits >= 64 {
            0
        } else {
            let integer = numerator >> denominator_bits;
            let remainder = numerator & ((1_u64 << denominator_bits) - 1);
            let half = 1_u64 << (denominator_bits - 1);
            integer + u64::from(remainder > half || (remainder == half && integer & 1 != 0))
        }
    };
    let result = tenth_to_f64(rounded);
    result.copysign(value)
}

#[inline]
fn tenth_to_f64(integer: u64) -> f64 {
    if integer <= 1_u64 << 53 {
        return integer as f64 / 10.0;
    }
    // Avoid double rounding when the decimal-scaled integer no longer fits
    // exactly in f64. Divide the integer rational by 10 and round only once.
    let mut exponent = (63 - integer.leading_zeros()) as i32 - 3;
    if integer < 10_u64 << exponent as u32 {
        exponent -= 1;
    }
    let numerator = (integer as u128) << (52 - exponent) as u32;
    let mut mantissa = (numerator / 10) as u64;
    let remainder = numerator % 10;
    if remainder > 5 || (remainder == 5 && mantissa & 1 != 0) {
        mantissa += 1;
    }
    if mantissa == 1_u64 << 53 {
        mantissa >>= 1;
        exponent += 1;
    }
    f64::from_bits(((exponent + 1023) as u64) << 52 | (mantissa & ((1_u64 << 52) - 1)))
}

/// NumPy's `round(x, 1)`: binary scaling, binary nearest-even, binary division.
/// This intentionally differs from Python at inputs including 2.55 and 0.05.
#[inline]
pub fn numpy_round1(value: f64) -> f64 {
    (value * 10.0).round_ties_even() / 10.0
}
