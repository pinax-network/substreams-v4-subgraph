//! Minimal Graph Node v0.44 Decimal128 arithmetic used by cacheable,
//! event-local computations. Keep this byte-for-byte semantically aligned
//! with the native reducer's `src/decimal.rs` implementation.

use graph_bigdecimal::BigDecimal as OldBigDecimal;
use graph_num_bigint::BigInt as OldBigInt;
use num_bigint::BigInt;
use num_traits::Zero;
use std::{fmt, ops, str::FromStr};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct GraphDecimal(OldBigDecimal);

impl GraphDecimal {
    const MAX_SIGNIFICANT_DIGITS: u64 = 34;

    pub(crate) fn zero() -> Self {
        Self(OldBigDecimal::from(0))
    }

    pub(crate) fn one() -> Self {
        Self(OldBigDecimal::from(1))
    }

    pub(crate) fn from_bigint(value: &BigInt) -> Self {
        value
            .to_string()
            .parse()
            .expect("a base-10 BigInt is a valid decimal")
    }

    pub(crate) fn is_zero(&self) -> bool {
        self == &Self::zero()
    }

    fn normalized(value: OldBigDecimal) -> Self {
        let zero = OldBigDecimal::from(0);
        if value == zero {
            return Self(zero);
        }

        let rounded = value.with_prec(Self::MAX_SIGNIFICANT_DIGITS);
        let (bigint, exp) = rounded.as_bigint_and_exponent();
        let (sign, mut digits) = bigint.to_radix_be(10);
        let trailing = digits.iter().rev().take_while(|digit| **digit == 0).count();
        digits.truncate(digits.len() - trailing);
        let integer = OldBigInt::from_radix_be(sign, &digits, 10)
            .expect("normalization retains at least one base-10 digit");
        Self(OldBigDecimal::new(integer, exp - trailing as i64))
    }

    fn wasm_result(value: OldBigDecimal) -> Self {
        let host_result = Self::normalized(value);
        Self::normalized(host_result.0)
    }
}

impl FromStr for GraphDecimal {
    type Err = <OldBigDecimal as FromStr>::Err;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self::normalized(OldBigDecimal::from_str(value)?))
    }
}

impl From<i32> for GraphDecimal {
    fn from(value: i32) -> Self {
        Self::normalized(OldBigDecimal::from(value))
    }
}

impl fmt::Display for GraphDecimal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

macro_rules! arithmetic {
    ($trait:ident, $method:ident) => {
        impl ops::$trait for GraphDecimal {
            type Output = Self;

            fn $method(self, rhs: Self) -> Self::Output {
                Self::wasm_result(self.0.$method(rhs.0))
            }
        }
    };
}

arithmetic!(Mul, mul);

impl ops::Div for GraphDecimal {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        assert!(!rhs.is_zero(), "Cannot divide by zero-valued BigDecimal!");
        Self::wasm_result(self.0.div(rhs.0))
    }
}

pub(crate) fn pow10(decimals: &BigInt) -> GraphDecimal {
    let count: usize = decimals
        .to_string()
        .parse()
        .expect("token decimals fit usize after the deployed <255 guard");
    let mut value = String::with_capacity(count + 1);
    value.push('1');
    value.extend(std::iter::repeat_n('0', count));
    value.parse().expect("power-of-ten decimal is valid")
}

pub(crate) fn token_to_decimal(amount: &BigInt, decimals: &BigInt) -> GraphDecimal {
    let amount = GraphDecimal::from_bigint(amount);
    if decimals.is_zero() {
        amount
    } else {
        amount / pow10(decimals)
    }
}

pub(crate) fn sqrt_price_x96_to_token_prices(
    sqrt_price_x96: &BigInt,
    token0_decimals: &BigInt,
    token1_decimals: &BigInt,
) -> (GraphDecimal, GraphDecimal) {
    let squared = sqrt_price_x96 * sqrt_price_x96;
    let q192 = BigInt::from(2u8).pow(192);
    let price1 = (GraphDecimal::from_bigint(&squared) / GraphDecimal::from_bigint(&q192))
        * pow10(token0_decimals)
        / pow10(token1_decimals);
    let price0 = if price1.is_zero() {
        GraphDecimal::zero()
    } else {
        GraphDecimal::one() / price1.clone()
    };
    (price0, price1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_native_decimal128_price_fixture() {
        let sqrt: BigInt = "79228162514264337593543950336".parse().unwrap();
        let (price0, price1) =
            sqrt_price_x96_to_token_prices(&sqrt, &BigInt::from(18), &BigInt::from(6));
        assert_eq!(price0.to_string(), "0.000000000001");
        assert_eq!(price1.to_string(), "1000000000000");
    }

    #[test]
    fn matches_native_token_conversion_fixture() {
        assert_eq!(
            token_to_decimal(&BigInt::from(123_456), &BigInt::from(3)).to_string(),
            "123.456"
        );
    }
}
