//! Charset-aware Shannon entropy for the generic secret detector (CLO-514, FR-60).
//!
//! Shannon entropy of a length-`L` string is bounded by `log2(L)`, so a raw
//! threshold (e.g. base64 ≈ 4.5) is mathematically unreachable for the short
//! (16-22 char) tokens FR-60 must catch. We therefore gate primarily on
//! length-normalized entropy `H / log2(L)`, with the raw value as a secondary
//! floor only where it is reachable.

/// Character class of a candidate secret value, ordered by superset:
/// `Hex` ⊂ `Alnum` ⊂ `Base64`. Classified hex-first so a hex token does not
/// inherit the stricter base64 threshold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Charset {
    /// `[0-9a-fA-F]` only.
    Hex,
    /// Letters + digits (base36-ish), no `+`/`/`/`-`/`_`.
    Alnum,
    /// Base64/base64url alphabet (`A-Za-z0-9+/=-_`).
    Base64,
}

impl Charset {
    /// Classify a value by its widest required character class, hex first.
    pub fn classify(value: &str) -> Self {
        let mut all_hex = true;
        let mut all_alnum = true;
        for c in value.chars() {
            if !c.is_ascii_hexdigit() {
                all_hex = false;
            }
            if !c.is_ascii_alphanumeric() {
                all_alnum = false;
            }
        }
        if all_hex {
            Charset::Hex
        } else if all_alnum {
            Charset::Alnum
        } else {
            Charset::Base64
        }
    }

    /// Normalized-entropy floor for this class: the ratio `H / log2(L)` a value
    /// must clear. Wider alphabets demand a higher ratio (more disorder) before
    /// we believe the value is a random secret rather than prose/identifiers.
    pub fn normalized_floor(self) -> f64 {
        match self {
            Charset::Hex => 0.60,
            Charset::Alnum => 0.72,
            Charset::Base64 => 0.78,
        }
    }
}

/// Realized Shannon entropy (bits/symbol) over the string's own characters.
///
/// Computed over `chars()`, never bytes: a multi-byte UTF-8 sequence must count
/// as one symbol, not inflate the score as several high-value bytes.
pub fn shannon_entropy(value: &str) -> f64 {
    let mut counts = std::collections::HashMap::new();
    let mut total = 0usize;
    for c in value.chars() {
        *counts.entry(c).or_insert(0usize) += 1;
        total += 1;
    }
    if total == 0 {
        return 0.0;
    }
    let total = total as f64;
    counts
        .values()
        .map(|&n| {
            let p = n as f64 / total;
            -p * p.log2()
        })
        .sum()
}

/// Length-normalized entropy `H / log2(L)` in `[0, 1]` (1.0 when every char is
/// distinct). Length-invariant, so it works for short and long values alike.
/// Returns 0.0 for values too short to normalize (`len < 2`).
pub fn normalized_entropy(value: &str) -> f64 {
    let len = value.chars().count();
    if len < 2 {
        return 0.0;
    }
    shannon_entropy(value) / (len as f64).log2()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ac1_calibration_value_clears_its_charset_floor() {
        // The binding acid-test from the spec: the AC1 GitLab token must be
        // detectable. Measured H ≈ 3.33, normalized ≈ 0.83 (verified offline).
        let v = "3cjcjg988jrskbxx";
        let h = shannon_entropy(v);
        let norm = normalized_entropy(v);
        assert!((h - 3.328).abs() < 0.01, "H was {h}");
        assert!((norm - 0.832).abs() < 0.01, "norm was {norm}");
        assert_eq!(Charset::classify(v), Charset::Alnum);
        assert!(
            norm >= Charset::Alnum.normalized_floor(),
            "AC1 value must clear the alnum floor"
        );
    }

    #[test]
    fn benign_low_entropy_value_stays_below_floor() {
        let v = "aaaaaaaaaaaaaaaa"; // 16 identical chars: H = 0
        assert_eq!(shannon_entropy(v), 0.0);
        assert!(normalized_entropy(v) < Charset::Alnum.normalized_floor());
    }

    #[test]
    fn classifier_is_hex_before_base64() {
        assert_eq!(Charset::classify("deadbeef0123"), Charset::Hex);
        assert_eq!(Charset::classify("abcXYZ789"), Charset::Alnum);
        assert_eq!(Charset::classify("ab+/cd=="), Charset::Base64);
    }

    #[test]
    fn entropy_counts_chars_not_bytes() {
        // Multi-byte chars must not inflate the score beyond log2(len).
        let v = "токентокен"; // 10 Cyrillic chars, 5 distinct
        assert!(shannon_entropy(v) <= (10f64).log2() + 1e-9);
    }
}
