use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};
use std::fmt;

/// How many decimal places an approximate value is shown to.
pub const APPROX_PLACES: usize = 10;

/// How many times the repeating part of a decimal is shown before the `...`.
pub const REPEAT_TIMES: usize = 4;

/// A repeating cycle longer than this is shown as `about ...` instead.
pub const MAX_PERIOD: usize = 20;

/// The largest exact integer we will build. Past this the result is refused
/// rather than computed, so `2 to the power of 99999` cannot hang the machine.
pub const MAX_DIGITS: usize = 1000;

/// The message used when a power or factorial is past the cap.
pub const TOO_BIG: &str = "that number is too big to calculate";

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// An exact whole number.
    Integer(BigInt),
    /// An exact fraction that is not a whole number.
    Rational(BigRational),
    /// An approximate number, such as the square root of 2.
    Approx(f64),
    String(String),
    List(Vec<Value>),
    /// Yes or no, from the comparisons and the logic words.
    Bool(bool),
    Undefined,
}

impl Value {
    pub fn integer(value: i64) -> Value {
        Value::Integer(BigInt::from(value))
    }

    pub fn number(value: f64) -> Value {
        Value::Approx(value)
    }

    pub fn as_rational(&self) -> Option<BigRational> {
        match self {
            Value::Integer(i) => Some(BigRational::from_integer(i.clone())),
            Value::Rational(r) => Some(r.clone()),
            Value::Approx(f) if f.is_finite() => BigRational::from_float(*f),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Integer(i) => i.to_f64(),
            Value::Rational(r) => r.to_f64(),
            Value::Approx(f) => Some(*f),
            _ => None,
        }
    }

    /// The value as a whole number, if it really is one.
    pub fn as_integer(&self) -> Option<BigInt> {
        match self {
            Value::Integer(i) => Some(i.clone()),
            Value::Rational(r) if r.is_integer() => Some(r.to_integer()),
            _ => None,
        }
    }

    pub fn as_usize(&self) -> Option<usize> {
        self.as_integer().and_then(|i| i.to_usize())
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&Vec<Value>> {
        match self {
            Value::List(items) => Some(items),
            _ => None,
        }
    }

    pub fn is_undefined(&self) -> bool {
        matches!(self, Value::Undefined)
    }

    pub fn is_number(&self) -> bool {
        matches!(
            self,
            Value::Integer(_) | Value::Rational(_) | Value::Approx(_)
        )
    }

    /// Truthiness for `if`, `repeat while`, `and`, `or` and `not`.
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Integer(i) => !i.is_zero(),
            Value::Rational(r) => !r.is_zero(),
            Value::Approx(f) => *f != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::List(items) => !items.is_empty(),
            Value::Undefined => false,
        }
    }

    /// How the value is shown to the user.
    pub fn display(&self) -> String {
        match self {
            Value::Integer(i) => i.to_string(),
            Value::Rational(r) => display_rational(r),
            Value::Approx(f) => display_approx(*f),
            Value::String(s) => s.clone(),
            Value::List(items) => {
                let parts: Vec<String> = items.iter().map(|item| item.display()).collect();
                format!("[{}]", parts.join(", "))
            }
            Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
            Value::Undefined => "undefined".to_string(),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display())
    }
}

/// `about 1.4142135624`
pub fn display_approx(value: f64) -> String {
    if !value.is_finite() {
        return "undefined".to_string();
    }
    let mut text = format!("{value:.*}", APPROX_PLACES);
    if text.contains('.') {
        // Drop trailing zeros so that 2.0 is shown as 2, not 2.0000000000.
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    format!("about {text}")
}

/// Exact decimals when the denominator is made only of 2s and 5s, otherwise the
/// repeating part is shown followed by `...`.
pub fn display_rational(value: &BigRational) -> String {
    if value.is_integer() {
        return value.to_integer().to_string();
    }

    let negative = value.is_negative();
    let magnitude = value.abs();
    let numerator = magnitude.numer().clone();
    let denominator = magnitude.denom().clone();

    let integer_part = &numerator / &denominator;
    let mut remainder = (numerator % denominator.clone()).abs();
    let mut digits = String::new();
    let mut seen: std::collections::HashMap<BigInt, usize> = std::collections::HashMap::new();
    let mut repeat_start: Option<usize> = None;

    while !remainder.is_zero() {
        if let Some(position) = seen.get(&remainder) {
            repeat_start = Some(*position);
            break;
        }
        seen.insert(remainder.clone(), digits.len());

        remainder *= 10;
        let digit = &remainder / &denominator;
        digits.push_str(&digit.to_string());
        remainder %= denominator.clone();

        // A guard against a runaway loop; real cycles are far shorter.
        if digits.len() > 4 * MAX_DIGITS {
            return display_approx(magnitude.to_f64().unwrap_or(0.0));
        }
    }

    let sign = if negative { "-" } else { "" };

    match repeat_start {
        Some(start) => {
            let cycle = &digits[start..];
            if cycle.len() > MAX_PERIOD {
                return display_approx(magnitude.to_f64().unwrap_or(0.0));
            }
            let repeated = cycle.repeat(REPEAT_TIMES);
            format!("{sign}{integer_part}.{}{repeated}...", &digits[..start])
        }
        None => format!("{sign}{integer_part}.{digits}"),
    }
}

/// Parse a number written in a Vynglish program.
pub fn parse_number(text: &str) -> Value {
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();
    if cleaned.contains('.') {
        match cleaned.parse::<f64>() {
            Ok(value) => Value::Approx(value),
            Err(_) => Value::Undefined,
        }
    } else {
        match cleaned.parse::<BigInt>() {
            Ok(value) => Value::Integer(value),
            Err(_) => match cleaned.parse::<f64>() {
                Ok(value) => Value::Approx(value),
                Err(_) => Value::Undefined,
            },
        }
    }
}

/// Compare two values exactly when both are exact.
fn compare(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::String(x), Value::String(y)) => Some(x.cmp(y)),
        (Value::Bool(x), Value::Bool(y)) => Some(x.cmp(y)),
        (Value::List(x), Value::List(y)) => {
            for (left, right) in x.iter().zip(y.iter()) {
                match compare(left, right) {
                    Some(std::cmp::Ordering::Equal) => continue,
                    other => return other,
                }
            }
            Some(x.len().cmp(&y.len()))
        }
        _ => {
            let left = a.as_rational()?;
            let right = b.as_rational()?;
            left.partial_cmp(&right)
        }
    }
}

pub fn add(a: &Value, b: &Value) -> Value {
    with_numbers(a, b, |x, y| x + y)
}

pub fn subtract(a: &Value, b: &Value) -> Value {
    with_numbers(a, b, |x, y| x - y)
}

pub fn multiply(a: &Value, b: &Value) -> Value {
    with_numbers(a, b, |x, y| x * y)
}

/// `1 divided by 0` is undefined. `0 divided by anything` is 0.
pub fn divide(a: &Value, b: &Value) -> Value {
    let (Some(left), Some(right)) = (a.as_rational(), b.as_rational()) else {
        return Value::Undefined;
    };
    if right.is_zero() {
        return Value::Undefined;
    }
    if left.is_zero() {
        return Value::Integer(BigInt::zero());
    }
    exact(left / right)
}

/// Keep a result exact, but refuse one that is past the digit cap.
fn exact(value: BigRational) -> Value {
    if value.is_integer() {
        let integer = value.to_integer();
        if integer.to_string().len() > MAX_DIGITS {
            return Value::String(TOO_BIG.to_string());
        }
        return Value::Integer(integer);
    }
    let digits = value.numer().to_string().len() + value.denom().to_string().len();
    if digits > 2 * MAX_DIGITS {
        return Value::String(TOO_BIG.to_string());
    }
    Value::Rational(value)
}

/// Numbers add. Anything that is not a number has no answer.
fn with_numbers(
    a: &Value,
    b: &Value,
    op: impl Fn(BigRational, BigRational) -> BigRational,
) -> Value {
    match (a.as_rational(), b.as_rational()) {
        (Some(left), Some(right)) => exact(op(left, right)),
        _ => Value::Undefined,
    }
}

/// `a to the power of b`, exact for whole exponents.
pub fn power(a: &Value, b: &Value) -> Value {
    let (Some(base), Some(exponent)) = (a.as_rational(), b.as_integer()) else {
        return Value::Undefined;
    };
    if base.is_zero() && exponent.is_negative() {
        return Value::Undefined;
    }
    let Some(count) = exponent.to_i64() else {
        return Value::String(TOO_BIG.to_string());
    };
    if count.unsigned_abs() > MAX_DIGITS as u64 {
        return Value::String(TOO_BIG.to_string());
    }

    let mut result = BigRational::one();
    let factor = if count < 0 {
        base.recip()
    } else {
        base.clone()
    };
    for _ in 0..count.abs() {
        result *= &factor;
        if result.numer().to_string().len() > MAX_DIGITS
            || result.denom().to_string().len() > MAX_DIGITS
        {
            return Value::String(TOO_BIG.to_string());
        }
    }
    exact(result)
}

/// `factorial of n` for whole `n` that is not negative.
pub fn factorial(a: &Value) -> Value {
    let Some(count) = a.as_integer() else {
        return Value::Undefined;
    };
    if count.is_negative() {
        return Value::Undefined;
    }
    if count > BigInt::from(MAX_FACTORIAL) {
        return Value::String(TOO_BIG.to_string());
    }
    let count = count.to_usize().unwrap_or(0);
    let mut result = BigInt::one();
    for step in 2..=count {
        result *= step;
        if result.to_string().len() > MAX_DIGITS {
            return Value::String(TOO_BIG.to_string());
        }
    }
    Value::Integer(result)
}

/// The largest factorial we will compute before saying it is too big.
pub const MAX_FACTORIAL: u32 = 500;

/// The square root, exact when the value is a perfect square.
pub fn square_root(a: &Value) -> Value {
    let Some(value) = a.as_rational() else {
        return Value::Undefined;
    };
    if value.is_negative() {
        return Value::Undefined;
    }
    if value.is_zero() {
        return Value::Integer(BigInt::zero());
    }
    if let Some(exact_root) = integer_square_root(&value) {
        return Value::Integer(exact_root);
    }
    Value::Approx(value.to_f64().unwrap_or(0.0).sqrt())
}

/// The integer square root when one exists.
fn integer_square_root(value: &BigRational) -> Option<BigInt> {
    if !value.is_integer() {
        // `sqrt(9/4)` is exactly `3/2`.
        let numerator = integer_sqrt_exact(value.numer())?;
        let denominator = integer_sqrt_exact(value.denom())?;
        let root = BigRational::from_integer(numerator) / BigRational::from_integer(denominator);
        if (root.clone() * root.clone()) == *value {
            return Some(root.to_integer());
        }
        return None;
    }
    integer_sqrt_exact(value.numer())
}

/// The integer square root of a whole number, if it is a perfect square.
fn integer_sqrt_exact(value: &BigInt) -> Option<BigInt> {
    if value.is_negative() {
        return None;
    }
    let root = value.sqrt();
    if root.clone() * root.clone() == *value {
        Some(root)
    } else {
        None
    }
}

pub fn natural_log(a: &Value) -> Value {
    match a.as_f64() {
        Some(value) if value > 0.0 => Value::Approx(value.ln()),
        _ => Value::Undefined,
    }
}

pub fn log_base(base: &Value, value: &Value) -> Value {
    match (base.as_f64(), value.as_f64()) {
        (Some(base), Some(value)) if base > 0.0 && base != 1.0 && value > 0.0 => {
            Value::Approx(value.log(base))
        }
        _ => Value::Undefined,
    }
}

pub fn absolute(a: &Value) -> Value {
    match a {
        Value::Integer(i) => Value::Integer(i.abs()),
        Value::Rational(r) => Value::Rational(r.abs()),
        Value::Approx(f) => Value::Approx(f.abs()),
        _ => Value::Undefined,
    }
}

/// Rounding that keeps the exact type when the value is already whole.
pub fn round(a: &Value) -> Value {
    match a {
        Value::Integer(i) => Value::Integer(i.clone()),
        Value::Rational(r) => {
            let doubled = r.clone() + r.clone();
            let half_up = if doubled.is_negative() {
                (doubled - BigRational::new(BigInt::one(), BigInt::from(2))).floor()
            } else {
                (doubled + BigRational::new(BigInt::one(), BigInt::from(2))).floor()
            };
            Value::Integer(half_up.to_integer())
        }
        Value::Approx(f) => Value::Approx(f.round()),
        _ => Value::Undefined,
    }
}

/// Round to the nearest whole number, half away from zero.
pub fn to_whole(a: &Value) -> Value {
    let Some(value) = a.as_rational() else {
        return Value::Undefined;
    };
    let shifted = value.clone() + BigRational::new(BigInt::one(), BigInt::from(2));
    Value::Integer(shifted.floor().to_integer())
}

pub fn compare_with(a: &Value, b: &Value, how: Comparison) -> Value {
    let Some(ordering) = compare(a, b) else {
        return Value::Bool(false);
    };
    use std::cmp::Ordering::*;
    Value::Bool(match how {
        Comparison::Greater => ordering == Greater,
        Comparison::Less => ordering == Less,
        Comparison::Equal => ordering == Equal,
        Comparison::NotEqual => ordering != Equal,
        Comparison::AtLeast => ordering != Less,
        Comparison::AtMost => ordering != Greater,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    Greater,
    Less,
    Equal,
    NotEqual,
    AtLeast,
    AtMost,
}

pub fn text_length(a: &Value) -> Value {
    match a.as_text() {
        Some(text) => Value::integer(text.chars().count() as i64),
        None => Value::Undefined,
    }
}

pub fn join_texts(a: &Value, b: &Value) -> Value {
    match (a.as_text(), b.as_text()) {
        (Some(left), Some(right)) => Value::String(format!("{left}{right}")),
        _ => Value::Undefined,
    }
}

pub fn contains_text(haystack: &Value, needle: &Value) -> Value {
    match (haystack.as_text(), needle.as_text()) {
        (Some(haystack), Some(needle)) => Value::Bool(haystack.contains(needle)),
        _ => Value::Bool(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exact_text(value: &str) -> String {
        display_rational(&BigRational::new(
            value.split('/').next().unwrap().parse().unwrap(),
            value.split('/').nth(1).unwrap().parse().unwrap(),
        ))
    }

    #[test]
    fn repeating_decimal_shows_the_cycle_then_dots() {
        assert_eq!(exact_text("23/6"), "3.83333...");
        assert_eq!(exact_text("1/2"), "0.5");
        assert_eq!(exact_text("1/3"), "0.3333...");
    }

    #[test]
    fn division_rules() {
        assert_eq!(
            divide(&Value::integer(1), &Value::integer(0)),
            Value::Undefined
        );
        assert_eq!(
            divide(&Value::integer(0), &Value::integer(1)),
            Value::Integer(BigInt::zero())
        );
        assert_eq!(exact_text("23/6"), "3.83333...");
    }

    #[test]
    fn square_roots() {
        assert_eq!(square_root(&Value::integer(16)), Value::integer(4));
        assert_eq!(square_root(&Value::integer(-4)), Value::Undefined);
        assert_eq!(
            square_root(&Value::integer(2)).display(),
            "about 1.4142135624"
        );
    }
}
