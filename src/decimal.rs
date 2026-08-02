//! Graph Node v0.44's pinned decimal arithmetic and normalization rules.

use graph_bigdecimal::BigDecimal as OldBigDecimal;
use graph_num_bigint::BigInt as OldBigInt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, ops, str::FromStr};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GraphDecimal(OldBigDecimal);

impl GraphDecimal {
    pub const MAX_SIGNIFICANT_DIGITS: u64 = 34;
    pub const MIN_EXP: i32 = -6143;
    pub const MAX_EXP: i32 = 6144;

    pub fn zero() -> Self {
        Self(OldBigDecimal::from(0))
    }

    pub fn one() -> Self {
        Self(OldBigDecimal::from(1))
    }

    pub fn from_bigint(value: &num_bigint::BigInt) -> Self {
        value
            .to_string()
            .parse()
            .expect("a base-10 BigInt is a valid decimal")
    }

    pub fn is_zero(&self) -> bool {
        self == &Self::zero()
    }

    pub fn abs(&self) -> Self {
        if self < &Self::zero() {
            Self::zero() - self.clone()
        } else {
            self.clone()
        }
    }

    pub fn as_bigint_and_exponent(&self) -> (OldBigInt, i64) {
        self.0.as_bigint_and_exponent()
    }

    pub fn exponent_in_range(&self) -> bool {
        let (digits, scale) = self.as_bigint_and_exponent();
        let significant_digits = digits.to_str_radix(10).trim_start_matches('-').len() as i64;
        let exponent = significant_digits - scale;
        (i64::from(Self::MIN_EXP)..=i64::from(Self::MAX_EXP)).contains(&exponent)
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
}

impl Default for GraphDecimal {
    fn default() -> Self {
        Self::zero()
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

impl From<i64> for GraphDecimal {
    fn from(value: i64) -> Self {
        Self::normalized(OldBigDecimal::from(value))
    }
}

impl fmt::Display for GraphDecimal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl fmt::Debug for GraphDecimal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "GraphDecimal({})", self.0)
    }
}

impl Serialize for GraphDecimal {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for GraphDecimal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

macro_rules! arithmetic {
    ($trait:ident, $method:ident) => {
        impl ops::$trait for GraphDecimal {
            type Output = Self;

            fn $method(self, rhs: Self) -> Self::Output {
                Self::normalized(self.0.$method(rhs.0))
            }
        }
    };
}

arithmetic!(Add, add);
arithmetic!(Sub, sub);
arithmetic!(Mul, mul);

impl ops::Div for GraphDecimal {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        assert!(!rhs.is_zero(), "Cannot divide by zero-valued BigDecimal!");
        Self::normalized(self.0.div(rhs.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_every_operation_to_decimal128_precision() {
        let one = GraphDecimal::one();
        let three: GraphDecimal = "3".parse().unwrap();
        assert_eq!(
            (one / three).to_string(),
            "0.3333333333333333333333333333333333"
        );
        assert_eq!(
            "1.230000".parse::<GraphDecimal>().unwrap().to_string(),
            "1.23"
        );
    }

    #[test]
    fn matches_graph_nodes_intermediate_rounding() {
        let a: GraphDecimal = "99999999999999999999999999999999999".parse().unwrap();
        assert_eq!(a.to_string(), "100000000000000000000000000000000000");
        assert_eq!((a.clone() + GraphDecimal::one()).to_string(), a.to_string());
    }
}
