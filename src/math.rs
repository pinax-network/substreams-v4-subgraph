//! Exact ports of the deployed mapping's BigInt and BigDecimal helpers.

use crate::decimal::GraphDecimal;
use num_bigint::BigInt;
use num_traits::{One, Signed, Zero};

pub fn pow10(decimals: &BigInt) -> GraphDecimal {
    let count: usize = decimals
        .to_string()
        .parse()
        .expect("token decimals fit usize after the deployed <255 guard");
    let mut value = String::with_capacity(count + 1);
    value.push('1');
    value.extend(std::iter::repeat_n('0', count));
    value.parse().expect("power-of-ten decimal is valid")
}

pub fn token_to_decimal(amount: &BigInt, decimals: &BigInt) -> GraphDecimal {
    let amount = GraphDecimal::from_bigint(amount);
    if decimals.is_zero() {
        amount
    } else {
        amount / pow10(decimals)
    }
}

pub fn safe_div(lhs: GraphDecimal, rhs: GraphDecimal) -> GraphDecimal {
    if rhs.is_zero() {
        GraphDecimal::zero()
    } else {
        lhs / rhs
    }
}

pub fn sqrt_price_x96_to_token_prices(
    sqrt_price_x96: &BigInt,
    token0_decimals: &BigInt,
    token1_decimals: &BigInt,
) -> (GraphDecimal, GraphDecimal) {
    let squared = sqrt_price_x96 * sqrt_price_x96;
    let q192 = BigInt::from(2u8).pow(192);
    let price1 = (GraphDecimal::from_bigint(&squared) / GraphDecimal::from_bigint(&q192))
        * pow10(token0_decimals)
        / pow10(token1_decimals);
    let price0 = safe_div(GraphDecimal::one(), price1.clone());
    (price0, price1)
}

pub fn fast_exponentiation(value: GraphDecimal, power: i32) -> GraphDecimal {
    if power < 0 {
        return safe_div(GraphDecimal::one(), fast_exponentiation(value, -power));
    }
    if power == 0 {
        return GraphDecimal::one();
    }
    if power == 1 {
        return value;
    }
    let half = fast_exponentiation(value.clone(), power / 2);
    let mut result = half.clone() * half;
    if power % 2 == 1 {
        result = result * value;
    }
    result
}

fn bigint_hex(value: &str) -> BigInt {
    BigInt::parse_bytes(value.trim_start_matches("0x").as_bytes(), 16)
        .expect("pinned hexadecimal constant is valid")
}

fn mul_shift(value: BigInt, multiplier: &str) -> BigInt {
    (value * bigint_hex(multiplier)) >> 128
}

pub fn sqrt_ratio_at_tick(tick: i32) -> BigInt {
    assert!((-887_272..=887_272).contains(&tick), "TICK");
    let abs_tick = tick.unsigned_abs();
    let mut ratio = if abs_tick & 0x1 != 0 {
        bigint_hex("fffcb933bd6fad37aa2d162d1a594001")
    } else {
        bigint_hex("100000000000000000000000000000000")
    };
    for (mask, multiplier) in [
        (0x2, "fff97272373d413259a46990580e213a"),
        (0x4, "fff2e50f5f656932ef12357cf3c7fdcc"),
        (0x8, "ffe5caca7e10e4e61c3624eaa0941cd0"),
        (0x10, "ffcb9843d60f6159c9db58835c926644"),
        (0x20, "ff973b41fa98c081472e6896dfb254c0"),
        (0x40, "ff2ea16466c96a3843ec78b326b52861"),
        (0x80, "fe5dee046a99a2a811c461f1969c3053"),
        (0x100, "fcbe86c7900a88aedcffc83b479aa3a4"),
        (0x200, "f987a7253ac413176f2b074cf7815e54"),
        (0x400, "f3392b0822b70005940c7a398e4b70f3"),
        (0x800, "e7159475a2c29b7443b29c7fa6e889d9"),
        (0x1000, "d097f3bdfd2022b8845ad8f792aa5825"),
        (0x2000, "a9f746462d870fdf8a65dc1f90e061e5"),
        (0x4000, "70d869a156d2a1b890bb3df62baf32f7"),
        (0x8000, "31be135f97d08fd981231505542fcfa6"),
        (0x10000, "9aa508b5b7a84e1c677de54f3e99bc9"),
        (0x20000, "5d6af8dedb81196699c329225ee604"),
        (0x40000, "2216e584f5fa1ea926041bedfe98"),
        (0x80000, "48a170391f7dc42444e8fa2"),
    ] {
        if abs_tick & mask != 0 {
            ratio = mul_shift(ratio, multiplier);
        }
    }
    if tick > 0 {
        ratio = ((BigInt::one() << 256) - BigInt::one()) / ratio;
    }
    let denominator = BigInt::one() << 32;
    let remainder: BigInt = &ratio % &denominator;
    ratio / denominator
        + if remainder.is_zero() {
            BigInt::zero()
        } else {
            BigInt::one()
        }
}

fn mul_div_rounding_up(a: &BigInt, b: &BigInt, denominator: &BigInt) -> BigInt {
    let product = a * b;
    let result = &product / denominator;
    result
        + if (&product % denominator).is_zero() {
            BigInt::zero()
        } else {
            BigInt::one()
        }
}

fn amount0_delta(a: BigInt, b: BigInt, liquidity: &BigInt, round_up: bool) -> BigInt {
    let (a, b) = if a > b { (b, a) } else { (a, b) };
    let numerator1 = liquidity << 96;
    let numerator2 = &b - &a;
    if round_up {
        mul_div_rounding_up(
            &mul_div_rounding_up(&numerator1, &numerator2, &b),
            &BigInt::one(),
            &a,
        )
    } else {
        numerator1 * numerator2 / b / a
    }
}

fn amount1_delta(a: BigInt, b: BigInt, liquidity: &BigInt, round_up: bool) -> BigInt {
    let (a, b) = if a > b { (b, a) } else { (a, b) };
    let difference = b - a;
    let q96 = BigInt::one() << 96;
    if round_up {
        mul_div_rounding_up(liquidity, &difference, &q96)
    } else {
        liquidity * difference / q96
    }
}

pub fn liquidity_amounts(
    tick_lower: i32,
    tick_upper: i32,
    current_tick: i32,
    liquidity: &BigInt,
    current_sqrt_price_x96: &BigInt,
) -> (BigInt, BigInt) {
    let a = sqrt_ratio_at_tick(tick_lower);
    let b = sqrt_ratio_at_tick(tick_upper);
    let round_up = liquidity.is_positive();
    let amount0 = if current_tick < tick_lower {
        amount0_delta(a.clone(), b.clone(), liquidity, round_up)
    } else if current_tick < tick_upper {
        amount0_delta(
            current_sqrt_price_x96.clone(),
            b.clone(),
            liquidity,
            round_up,
        )
    } else {
        BigInt::zero()
    };
    let amount1 = if current_tick < tick_lower {
        BigInt::zero()
    } else if current_tick < tick_upper {
        amount1_delta(
            a.clone(),
            current_sqrt_price_x96.clone(),
            liquidity,
            round_up,
        )
    } else {
        amount1_delta(a, b, liquidity, round_up)
    };
    (amount0, amount1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_uniswap_tick_boundaries() {
        assert_eq!(sqrt_ratio_at_tick(-887_272).to_string(), "4295128739");
        assert_eq!(
            sqrt_ratio_at_tick(887_272).to_string(),
            "1461446703485210103287273052203988822378723970342"
        );
    }

    #[test]
    fn rounds_prices_after_each_graph_decimal_operation() {
        let sqrt: BigInt = "79228162514264337593543950336".parse().unwrap();
        let (price0, price1) = sqrt_price_x96_to_token_prices(&sqrt, &18.into(), &6.into());
        assert_eq!(price0.to_string(), "0.000000000001");
        assert_eq!(price1.to_string(), "1000000000000");
    }
}
