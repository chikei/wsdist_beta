use std::borrow::Cow;

use crate::model::CalcError;

// Decimal digit blocks from CPython 3.13.6's Unicode 15.1.0 database.
const DECIMAL_STARTS: [u32; 68] = [
    48, 1632, 1776, 1984, 2406, 2534, 2662, 2790, 2918, 3046, 3174, 3302,
    3430, 3558, 3664, 3792, 3872, 4160, 4240, 6112, 6160, 6470, 6608, 6784,
    6800, 6992, 7088, 7232, 7248, 42528, 43216, 43264, 43472, 43504, 43600,
    44016, 65296, 66720, 68912, 69734, 69872, 69942, 70096, 70384, 70736,
    70864, 71248, 71360, 71472, 71904, 72016, 72784, 73040, 73120, 73552,
    92768, 92864, 93008, 120782, 120792, 120802, 120812, 120822, 123200,
    123632, 124144, 125264, 130032,
];

pub(super) struct DecimalInteger<'a> {
    decimal: Cow<'a, str>,
    approximate: f64,
    field: &'static str,
}

impl<'a> DecimalInteger<'a> {
    pub fn parse(input: &'a str, field: &'static str) -> Result<Self, CalcError> {
        let text = input.trim();
        let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
        let mut previous_digit = false;
        let mut digits = 0;
        let mut normalized = None;
        for (offset, ch) in text.char_indices() {
            if offset == 0 && matches!(ch, '+' | '-') { continue; }
            if ch == '_' {
                if !previous_digit { return Err(invalid(input, field)); }
                previous_digit = false;
                if normalized.is_none() {
                    let mut output = String::with_capacity(text.len());
                    output.push_str(&text[..offset]);
                    normalized = Some(output);
                }
            } else {
                let digit = decimal_digit(ch).ok_or_else(|| invalid(input, field))?;
                previous_digit = true;
                digits += 1;
                if normalized.is_none() && !ch.is_ascii() {
                    let mut output = String::with_capacity(text.len());
                    output.push_str(&text[..offset]);
                    normalized = Some(output);
                }
                if let Some(output) = &mut normalized { output.push(char::from(b'0' + digit)); }
            }
        }
        if unsigned.is_empty() || !previous_digit { return Err(invalid(input, field)); }
        if digits > 4300 {
            return Err(CalcError::legacy("aggregate_buffs", field, "ValueError",
                format!("Exceeds the limit (4300 digits) for integer string conversion: value has {digits} digits; use sys.set_int_max_str_digits() to increase the limit")));
        }
        let decimal = match normalized { Some(value) => Cow::Owned(value), None => Cow::Borrowed(text) };
        let approximate = decimal.parse::<f64>().map_err(|_| invalid(input, field))?;
        Ok(Self { decimal, approximate: if approximate == 0.0 { 0.0 } else { approximate }, field })
    }

    pub fn as_float(&self) -> Result<f64, CalcError> {
        if self.approximate.is_finite() { Ok(self.approximate) }
        else { Err(CalcError::legacy("aggregate_buffs", self.field, "OverflowError", "int too large to convert to float")) }
    }

    pub fn capped(&self, cap: f64) -> Result<f64, CalcError> {
        if cap < self.approximate { Ok(cap) } else { self.as_float() }
    }

    pub fn divided_by_100(&self) -> Result<f64, CalcError> {
        // Python integer true division rounds the exact ratio, not an already
        // rounded integer float. This also permits 10**309 / 100 to be finite.
        let result = if self.approximate.abs() < (1_u64 << 53) as f64 {
            self.approximate / 100.0
        } else {
            let mut decimal = self.decimal.to_string();
            decimal.push_str("e-2");
            decimal.parse::<f64>().expect("validated decimal integer with exponent")
        };
        if result.is_finite() { Ok(if result == 0.0 { 0.0 } else { result }) }
        else { Err(CalcError::legacy("aggregate_buffs", self.field, "OverflowError", "integer division result too large for a float")) }
    }
}

fn decimal_digit(ch: char) -> Option<u8> {
    if ch.is_ascii_digit() { return Some(ch as u8 - b'0'); }
    let code = u32::from(ch);
    let index = DECIMAL_STARTS.partition_point(|start| *start <= code).checked_sub(1)?;
    let offset = code - DECIMAL_STARTS[index];
    (offset < 10).then_some(offset as u8)
}

fn invalid(input: &str, field: &'static str) -> CalcError {
    let quote = if input.contains('\'') && !input.contains('"') { '"' } else { '\'' };
    let mut literal = String::with_capacity(input.len() + 2);
    literal.push(quote);
    for ch in input.chars() {
        match ch {
            '\\' => literal.push_str("\\\\"), '\n' => literal.push_str("\\n"),
            '\r' => literal.push_str("\\r"), '\t' => literal.push_str("\\t"),
            ch if ch == quote => { literal.push('\\'); literal.push(ch); }
            ch if ch.is_control() => {
                use std::fmt::Write;
                write!(literal, "\\x{:02x}", u32::from(ch)).unwrap();
            }
            ch => literal.push(ch),
        }
    }
    literal.push(quote);
    CalcError::legacy("aggregate_buffs", field, "ValueError", format!("invalid literal for int() with base 10: {literal}"))
}
