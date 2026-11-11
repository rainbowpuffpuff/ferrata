//! Amounts in the token's units with its decimals shown, and the base units beside them
//! (ethskills frontend-ux: never show a raw base-unit integer alone, never round silently).

use anyhow::{bail, ensure, Result};

/// `amount` base units of a token with `decimals` decimals: "0.001 ETH (1000000000000000 base units,
/// 18 decimals)". Exact: no rounding, trailing zeros of the fraction dropped.
pub fn show(amount: u128, decimals: u8, symbol: &str) -> String {
    format!("{} {symbol} ({amount} base units, {decimals} decimals)", plain(amount, decimals))
}

/// `amount` in whole units, exact: 1000000000000000 with 18 decimals is "0.001".
pub fn plain(amount: u128, decimals: u8) -> String {
    if decimals == 0 {
        return amount.to_string();
    }
    let scale = 10u128.checked_pow(u32::from(decimals));
    let Some(scale) = scale else {
        // More than 38 decimals: print the digits with the point placed by hand.
        let digits = format!("{amount:0>width$}", width = usize::from(decimals) + 1);
        let (whole, frac) = digits.split_at(digits.len() - usize::from(decimals));
        let frac = frac.trim_end_matches('0');
        return if frac.is_empty() { whole.to_string() } else { format!("{whole}.{frac}") };
    };
    let whole = amount / scale;
    let frac = amount % scale;
    if frac == 0 {
        return whole.to_string();
    }
    let frac = format!("{frac:0>width$}", width = usize::from(decimals));
    format!("{whole}.{}", frac.trim_end_matches('0'))
}

/// "0.001" with 18 decimals is 1000000000000000 base units; more fractional digits than
/// `decimals` is an error, never a rounding.
pub fn parse(text: &str, decimals: u8) -> Result<u128> {
    let text = text.trim();
    ensure!(!text.is_empty(), "an empty amount");
    let (whole, frac) = text.split_once('.').unwrap_or((text, ""));
    ensure!(
        whole.chars().all(|c| c.is_ascii_digit()) && frac.chars().all(|c| c.is_ascii_digit()),
        "not an amount: {text}"
    );
    if frac.len() > usize::from(decimals) {
        bail!("{text} has more than {decimals} decimals");
    }
    let scale = 10u128.checked_pow(u32::from(decimals)).ok_or_else(|| anyhow::anyhow!("too many decimals"))?;
    let whole: u128 = if whole.is_empty() { 0 } else { whole.parse()? };
    let frac_value: u128 =
        if frac.is_empty() { 0 } else { frac.parse::<u128>()? * 10u128.pow(u32::from(decimals) - frac.len() as u32) };
    whole
        .checked_mul(scale)
        .and_then(|w| w.checked_add(frac_value))
        .ok_or_else(|| anyhow::anyhow!("{text} does not fit in 128 bits of base units"))
}

/// A 32-byte big-endian word as `u128` (the bridge's amounts all fit).
pub fn word_to_u128(word: &[u8; 32]) -> Result<u128> {
    ensure!(word[..16].iter().all(|b| *b == 0), "an amount above 2^128");
    Ok(u128::from_be_bytes(word[16..].try_into().expect("16 bytes")))
}

/// `u128` as a 32-byte big-endian word.
pub fn u128_to_word(n: u128) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[16..].copy_from_slice(&n.to_be_bytes());
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_and_parses_exactly() {
        assert_eq!(plain(1_000_000_000_000_000, 18), "0.001");
        assert_eq!(plain(1_500_000_000_000_000_000, 18), "1.5");
        assert_eq!(plain(100_000_000, 6), "100");
        assert_eq!(plain(1, 6), "0.000001");
        assert_eq!(plain(7, 0), "7");
        assert_eq!(show(1_000_000_000_000_000, 18, "ETH"), "0.001 ETH (1000000000000000 base units, 18 decimals)");
        assert_eq!(parse("0.001", 18).unwrap(), 1_000_000_000_000_000);
        assert_eq!(parse("100", 6).unwrap(), 100_000_000);
        assert_eq!(parse(".5", 1).unwrap(), 5);
        assert!(parse("0.0000001", 6).is_err(), "no silent rounding");
        assert!(parse("1e3", 6).is_err());
        assert!(parse("-1", 6).is_err());
        for (a, d) in [(0u128, 18u8), (123_456_789, 6), (u128::MAX, 18), (10, 1)] {
            assert_eq!(parse(&plain(a, d), d).unwrap(), a);
        }
        assert_eq!(word_to_u128(&u128_to_word(42)).unwrap(), 42);
    }
}
