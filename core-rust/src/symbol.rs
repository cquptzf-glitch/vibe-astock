use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Market {
    SH,
    SZ,
    BJ,
}

impl Market {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SH => "SH",
            Self::SZ => "SZ",
            Self::BJ => "BJ",
        }
    }

    pub const fn eastmoney_market_id(self) -> &'static str {
        match self {
            Self::SH => "1",
            Self::SZ | Self::BJ => "0",
        }
    }

    pub const fn tencent_prefix(self) -> &'static str {
        match self {
            Self::SH => "sh",
            Self::SZ => "sz",
            Self::BJ => "bj",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolKey {
    pub digits: String,
    pub market: Market,
    pub normalized: String,
    pub tencent: String,
    pub eastmoney: String,
}

pub fn natural_market(digits: &str) -> Option<Market> {
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if digits.starts_with("92") || matches!(&digits[..2], "43" | "83" | "87") {
        return Some(Market::BJ);
    }
    if matches!(digits.as_bytes()[0], b'5' | b'6' | b'9') {
        return Some(Market::SH);
    }
    Some(Market::SZ)
}

fn market_from_tag(tag: &str) -> Option<Market> {
    match tag {
        "SH" => Some(Market::SH),
        "SZ" => Some(Market::SZ),
        "BJ" => Some(Market::BJ),
        _ => None,
    }
}

/// Preserve the legacy stock symbol contract while giving the mobile Rust core
/// one canonical market-routing implementation.
///
/// Accepted spellings:
/// - `600519`
/// - `SH600519`
/// - `600519.SH`
///
/// Explicit market prefixes/suffixes are validated against the natural code
/// segment. Contradictory spellings are rejected instead of silently rerouted.
pub fn normalize_stock_symbol(input: &str) -> Result<SymbolKey> {
    let raw = input.trim().to_ascii_uppercase();
    if !raw.is_ascii() {
        return Err(anyhow!("invalid symbol: {input}"));
    }
    let (digits, explicit) = if raw.len() == 6 && raw.bytes().all(|b| b.is_ascii_digit()) {
        (raw.as_str(), None)
    } else if raw.len() == 8 {
        let tag = &raw[..2];
        let digits = &raw[2..];
        let market = market_from_tag(tag).ok_or_else(|| anyhow!("invalid symbol: {input}"))?;
        if !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(anyhow!("invalid symbol: {input}"));
        }
        (digits, Some(market))
    } else if raw.len() == 9 && raw.as_bytes()[6] == b'.' {
        let digits = &raw[..6];
        let tag = &raw[7..];
        let market = market_from_tag(tag).ok_or_else(|| anyhow!("invalid symbol: {input}"))?;
        if !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(anyhow!("invalid symbol: {input}"));
        }
        (digits, Some(market))
    } else {
        return Err(anyhow!("invalid symbol: {input}"));
    };

    let natural = natural_market(digits).ok_or_else(|| anyhow!("invalid symbol: {input}"))?;

    if digits.starts_with("000") {
        match explicit {
            Some(Market::BJ) => {
                return Err(anyhow!("invalid symbol: {input}: 000xxx does not belong to BSE"));
            }
            Some(Market::SH) => {
                return Err(anyhow!(
                    "invalid symbol: {input}: SH000xxx is an index spelling, not an individual stock"
                ));
            }
            _ => {}
        }
    } else if let Some(explicit) = explicit {
        if explicit != natural {
            return Err(anyhow!(
                "invalid symbol: {input}: explicit market {} contradicts natural market {}",
                explicit.as_str(),
                natural.as_str()
            ));
        }
    }

    let market = explicit.unwrap_or(natural);
    Ok(SymbolKey {
        digits: digits.to_owned(),
        market,
        normalized: format!("{}{}", market.as_str(), digits),
        tencent: format!("{}{}", market.tencent_prefix(), digits),
        eastmoney: format!("{}.{}", market.eastmoney_market_id(), digits),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_legacy_market_segments() {
        assert_eq!(natural_market("600519"), Some(Market::SH));
        assert_eq!(natural_market("000001"), Some(Market::SZ));
        assert_eq!(natural_market("300750"), Some(Market::SZ));
        assert_eq!(natural_market("430047"), Some(Market::BJ));
        assert_eq!(natural_market("830799"), Some(Market::BJ));
        assert_eq!(natural_market("870299"), Some(Market::BJ));
        assert_eq!(natural_market("920002"), Some(Market::BJ));
        assert_eq!(natural_market("900901"), Some(Market::SH));
    }

    #[test]
    fn accepts_legacy_spellings() {
        let sh = normalize_stock_symbol("600519").unwrap();
        assert_eq!(sh.normalized, "SH600519");
        assert_eq!(sh.tencent, "sh600519");
        assert_eq!(sh.eastmoney, "1.600519");

        let bj = normalize_stock_symbol("430047.BJ").unwrap();
        assert_eq!(bj.normalized, "BJ430047");
        assert_eq!(bj.tencent, "bj430047");
        assert_eq!(bj.eastmoney, "0.430047");

        let sz = normalize_stock_symbol("sz000001").unwrap();
        assert_eq!(sz.normalized, "SZ000001");
    }

    #[test]
    fn rejects_market_contradictions_and_non_legacy_spelling() {
        assert!(normalize_stock_symbol("SZ600519").is_err());
        assert!(normalize_stock_symbol("BJ000001").is_err());
        assert!(normalize_stock_symbol("SH000001").is_err());
        assert!(normalize_stock_symbol("SH.600519").is_err());
        assert!(normalize_stock_symbol("AAPL").is_err());
    }
}
