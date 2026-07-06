//! Hunk classification for `gcm resolve` (CLO-531, ST3).
//!
//! Deterministic resolutions keep the LLM off the critical path for the
//! majority of conflict hunks.

use super::markers::Hunk;

/// The resolution strategy applied to a hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkResolution {
    /// Deterministic: identical sides, one side unchanged, or one side empty.
    Auto { text: String, reason: AutoReason },
    /// Both sides diverge; send to the provider.
    Complex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoReason {
    IdenticalSides,
    OursUnchanged,
    TheirsUnchanged,
    OneSideEmpty,
}

/// Classify a hunk for resolution strategy. Returns `Trivial(...)` when the
/// outcome is unambiguous without an LLM, else `Complex`.
pub fn classify(hunk: &Hunk) -> HunkResolution {
    if hunk.ours == hunk.theirs {
        // If both sides are empty and we have no base, the hunk may be malformed
        // (e.g. plain diff3 without base) - escalate rather than silently keep empty.
        if hunk.ours.is_empty() && hunk.base.is_none() {
            return HunkResolution::Complex;
        }
        return HunkResolution::Auto {
            text: hunk.ours.clone(),
            reason: AutoReason::IdenticalSides,
        };
    }
    if let Some(base) = &hunk.base {
        if base == &hunk.ours {
            return HunkResolution::Auto {
                text: hunk.theirs.clone(),
                reason: AutoReason::OursUnchanged,
            };
        }
        if base == &hunk.theirs {
            return HunkResolution::Auto {
                text: hunk.ours.clone(),
                reason: AutoReason::TheirsUnchanged,
            };
        }
    }
    if hunk.ours.is_empty() || hunk.theirs.is_empty() {
        let text = if hunk.ours.is_empty() {
            hunk.theirs.clone()
        } else {
            hunk.ours.clone()
        };
        return HunkResolution::Auto {
            text,
            reason: AutoReason::OneSideEmpty,
        };
    }
    HunkResolution::Complex
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(base: Option<&str>, ours: &str, theirs: &str) -> Hunk {
        Hunk {
            start_line: 1,
            end_line: 1,
            base: base.map(|s| s.to_string()),
            ours: ours.to_string(),
            theirs: theirs.to_string(),
        }
    }

    #[test]
    fn identical_sides() {
        let r = classify(&h(Some("base\n"), "same\n", "same\n"));
        assert!(matches!(
            r,
            HunkResolution::Auto {
                reason: AutoReason::IdenticalSides,
                ..
            }
        ));
        assert_eq!(r.auto_text(), "same\n");
    }

    #[test]
    fn ours_unchanged() {
        let r = classify(&h(Some("base\n"), "base\n", "theirs\n"));
        assert!(matches!(
            r,
            HunkResolution::Auto {
                reason: AutoReason::OursUnchanged,
                ..
            }
        ));
        assert_eq!(r.auto_text(), "theirs\n");
    }

    #[test]
    fn theirs_unchanged() {
        let r = classify(&h(Some("base\n"), "ours\n", "base\n"));
        assert!(matches!(
            r,
            HunkResolution::Auto {
                reason: AutoReason::TheirsUnchanged,
                ..
            }
        ));
        assert_eq!(r.auto_text(), "ours\n");
    }

    #[test]
    fn one_side_empty() {
        let r = classify(&h(Some("base\n"), "", "theirs\n"));
        assert!(matches!(
            r,
            HunkResolution::Auto {
                reason: AutoReason::OneSideEmpty,
                ..
            }
        ));
        assert_eq!(r.auto_text(), "theirs\n");
    }

    #[test]
    fn complex() {
        let r = classify(&h(Some("base\n"), "ours\n", "theirs\n"));
        assert_eq!(r, HunkResolution::Complex);
    }

    #[test]
    fn both_empty_with_base_is_identical() {
        // If both sides deleted the same content, resolving to empty is correct.
        let r = classify(&h(Some("base\n"), "", ""));
        assert!(matches!(
            r,
            HunkResolution::Auto {
                reason: AutoReason::IdenticalSides,
                ..
            }
        ));
        assert_eq!(r.auto_text(), "");
    }

    #[test]
    fn both_empty_without_base_is_complex() {
        // Without a base marker we cannot tell whether both sides deleted or the
        // hunk was malformed; escalate to the provider.
        let r = classify(&h(None, "", ""));
        assert_eq!(r, HunkResolution::Complex);
    }
}

impl HunkResolution {
    #[cfg(test)]
    fn auto_text(&self) -> String {
        match self {
            HunkResolution::Auto { text, .. } => text.clone(),
            HunkResolution::Complex => panic!("expected Auto"),
        }
    }
}
