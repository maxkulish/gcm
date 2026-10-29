//! Prompt budgets (CLO-797): how many tokens a provider call may spend on the
//! diff, derived from the selected model's context window. Binary-only: the
//! `Provider` trait is part of the locked library surface, so the window table
//! and the per-provider output facts live here rather than on the trait.
//!
//! Two kinds of call carry a diff and get separate budgets. The grouping call
//! sends the changed-file inventory and must leave room for the model to echo
//! every path back; a message call sends only the stat and the diff.

use crate::error::GcmError;
use crate::provider::ProviderId;

/// The window assumed for a model the table does not list. Deliberately
/// pessimistic; every use is announced (D6) and `GCM_CONTEXT_WINDOW_TOKENS`
/// overrides it.
pub const DEFAULT_WINDOW: usize = 32_768;
pub const WINDOW_ENV: &str = "GCM_CONTEXT_WINDOW_TOKENS";
/// Upper bound on `GCM_CONTEXT_WINDOW_TOKENS`; larger values are typos.
const MAX_WINDOW_OVERRIDE: usize = 10_000_000;

/// Room for the request envelope around the system prompt and user content
/// (roles, JSON framing, tool wrappers).
const ENVELOPE_SLACK: usize = 1_024;
/// Output a reply needs beyond the path echo: summaries and one commit message.
const FIXED_OUTPUT_OVERHEAD: usize = 1_024;
/// The plan echoes every path, plus JSON quoting and separators.
const PATH_ECHO_FACTOR: f64 = 1.15;
/// Output reserved for reasoning a request does not disable. No provider
/// publishes a bound for hidden or minimal reasoning; this is gcm's reserve.
const REASONING_ALLOWANCE: usize = 4_096;

/// Estimate the tokens `text` costs. Errs high: 2 bytes per token for
/// mostly-ASCII text, 1 character per token when more than a tenth of the
/// bytes are non-ASCII (CJK, Cyrillic and emoji tokenize far worse).
pub fn estimate_tokens(text: &str) -> usize {
    let non_ascii = text.bytes().filter(|b| !b.is_ascii()).count();
    if non_ascii * 10 > text.len() {
        text.chars().count()
    } else {
        text.len().div_ceil(2)
    }
}

/// The provider and model a run sends to.
#[derive(Debug, Clone)]
pub struct Target {
    pub provider: ProviderId,
    pub model: String,
}

/// Where a context window came from, so a default can be announced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowOrigin {
    Table,
    Default,
    Env,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub tokens: usize,
    pub origin: WindowOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Match {
    Exact,
    Prefix,
}

struct WindowEntry {
    provider: ProviderId,
    model: &'static str,
    by: Match,
    tokens: usize,
}

const WINDOWS: &[WindowEntry] = &[
    // docs/investigations/2026-09-16-clo-797-grouping-prompt-context-overflow.md
    // (Groq's 400 names the 131,072-token limit).
    WindowEntry {
        provider: ProviderId::Groq,
        model: "openai/gpt-oss-120b",
        by: Match::Exact,
        tokens: 131_072,
    },
    // platform.claude.com/docs/en/models/haiku-4-5/overview: 200K context.
    // The prefix covers the dated snapshot the alias resolves to.
    WindowEntry {
        provider: ProviderId::Anthropic,
        model: "claude-haiku-4-5",
        by: Match::Prefix,
        tokens: 200_000,
    },
    // openrouter.ai/google/gemini-3.5-flash-lite: 1,048,576-token context.
    WindowEntry {
        provider: ProviderId::Google,
        model: "gemini-3.5-flash-lite",
        by: Match::Prefix,
        tokens: 1_048_576,
    },
    WindowEntry {
        provider: ProviderId::Vertex,
        model: "gemini-3.5-flash-lite",
        by: Match::Prefix,
        tokens: 1_048_576,
    },
    // getdeploying.com/llms/gpt-5.6-terra and pricepertoken.com: 1,050,000-token
    // context for the GPT-5.6 family.
    WindowEntry {
        provider: ProviderId::Openai,
        model: "gpt-5.6",
        by: Match::Prefix,
        tokens: 1_050_000,
    },
];

/// Exact `(provider, model)` first, then the longest matching family prefix,
/// so a shorter prefix cannot shadow a larger-context sibling.
fn lookup(table: &[WindowEntry], provider: ProviderId, model: &str) -> Option<usize> {
    let mine = || table.iter().filter(move |e| e.provider == provider);
    if let Some(e) = mine().find(|e| e.by == Match::Exact && e.model == model) {
        return Some(e.tokens);
    }
    mine()
        .filter(|e| e.by == Match::Prefix && model.starts_with(e.model))
        .max_by_key(|e| e.model.len())
        .map(|e| e.tokens)
}

/// The context window for `target`: `GCM_CONTEXT_WINDOW_TOKENS` when set, else
/// the table, else [`DEFAULT_WINDOW`].
pub fn context_window(target: &Target) -> Result<Window, GcmError> {
    window_from(std::env::var(WINDOW_ENV).ok().as_deref(), target)
}

fn window_from(env: Option<&str>, target: &Target) -> Result<Window, GcmError> {
    if let Some(raw) = env {
        return parse_window_override(raw).map(|tokens| Window {
            tokens,
            origin: WindowOrigin::Env,
        });
    }
    Ok(match lookup(WINDOWS, target.provider, &target.model) {
        Some(tokens) => Window {
            tokens,
            origin: WindowOrigin::Table,
        },
        None => Window {
            tokens: DEFAULT_WINDOW,
            origin: WindowOrigin::Default,
        },
    })
}

/// A set override must be a positive integer no larger than
/// [`MAX_WINDOW_OVERRIDE`]. Unlike the diff-byte overrides, a bad value is an
/// error, not a silent fallback: a wrong window decides what the model sees.
fn parse_window_override(raw: &str) -> Result<usize, GcmError> {
    let invalid = |why: &str| {
        GcmError::Config(format!(
            "{WINDOW_ENV}='{raw}' {why}; set it to the model's context window in tokens, \
             e.g. {WINDOW_ENV}=131072, or unset it"
        ))
    };
    let t = raw.trim();
    if t.is_empty() {
        return Err(invalid("is empty"));
    }
    let n: usize = t.parse().map_err(|_| invalid("is not a whole number"))?;
    if n == 0 {
        return Err(invalid("must be greater than zero"));
    }
    if n > MAX_WINDOW_OVERRIDE {
        return Err(invalid("is larger than any known context window"));
    }
    Ok(n)
}

/// What a provider's payload fixes about the reply: the `max_tokens` it sends
/// for each call, and the reasoning the request does not disable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputFacts {
    pub plan_output_cap: Option<usize>,
    pub message_output_cap: Option<usize>,
    pub reasoning_allowance: usize,
}

pub fn output_facts(target: &Target) -> OutputFacts {
    let (plan_output_cap, message_output_cap) = match target.provider {
        ProviderId::Anthropic => (
            Some(crate::provider::ANTHROPIC_PLAN_MAX_TOKENS),
            Some(crate::provider::ANTHROPIC_MESSAGE_MAX_TOKENS),
        ),
        _ => (None, None),
    };
    let reasoning_allowance = match target.provider {
        // No extended thinking is requested.
        ProviderId::Anthropic => 0,
        // `reasoning_effort: "none"` (openai.rs).
        ProviderId::Openai => 0,
        // qwen gets `reasoning_effort: "none"`; gpt-oss only gets
        // `include_reasoning: false`, which hides the reasoning but still runs it.
        ProviderId::Groq if target.model.contains("qwen") => 0,
        // gpt-oss on Groq; Gemini's `thinkingLevel: MINIMAL` still thinks; Ollama
        // thinking models emit a `thinking` field.
        ProviderId::Groq | ProviderId::Google | ProviderId::Vertex | ProviderId::Ollama => {
            REASONING_ALLOWANCE
        }
    };
    OutputFacts {
        plan_output_cap,
        message_output_cap,
        reasoning_allowance,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetKind {
    Grouping,
    Message,
}

/// The token budget for one provider call. `available()` is what the user
/// content (inventory, stat and diff) may spend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptBudget {
    pub kind: BudgetKind,
    pub window_tokens: usize,
    pub envelope_allowance: usize,
    pub completion_reserve: usize,
}

/// The grouping call cannot be made at all, whatever the diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupingRejected {
    /// Echoing the paths back would exceed the provider's plan output cap.
    OutputCap { echo_tokens: usize, cap: usize },
}

impl PromptBudget {
    /// The grouping budget. The reply echoes every path in `files_json`, so
    /// the echo is reserved, and a provider whose plan output cap cannot hold
    /// it is rejected before any diff body is gathered.
    pub fn grouping(
        window: usize,
        target: &Target,
        files_json: &str,
    ) -> Result<Self, GroupingRejected> {
        let facts = output_facts(target);
        let echo_tokens = (estimate_tokens(files_json) as f64 * PATH_ECHO_FACTOR).ceil() as usize;
        if let Some(cap) = facts.plan_output_cap {
            if echo_tokens > cap {
                return Err(GroupingRejected::OutputCap { echo_tokens, cap });
            }
        }
        let echo_reserve = facts
            .plan_output_cap
            .map_or(echo_tokens, |cap| cap.min(echo_tokens));
        let schema = crate::plan::schema().to_string();
        Ok(PromptBudget {
            kind: BudgetKind::Grouping,
            window_tokens: window,
            envelope_allowance: estimate_tokens(crate::provider::GROUPING_SYSTEM_PROMPT)
                + estimate_tokens(&schema)
                + ENVELOPE_SLACK,
            completion_reserve: FIXED_OUTPUT_OVERHEAD + facts.reasoning_allowance + echo_reserve,
        })
    }

    /// The message budget (single-commit, fallback and per-group messages). A
    /// message echoes no paths, so there is no echo reserve and no rejection.
    pub fn message(window: usize, target: &Target) -> Self {
        let facts = output_facts(target);
        PromptBudget {
            kind: BudgetKind::Message,
            window_tokens: window,
            envelope_allowance: estimate_tokens(crate::provider::SYSTEM_PROMPT) + ENVELOPE_SLACK,
            completion_reserve: facts.message_output_cap.unwrap_or(FIXED_OUTPUT_OVERHEAD)
                + facts.reasoning_allowance,
        }
    }

    /// Tokens left for the user content.
    pub fn available(&self) -> usize {
        self.window_tokens
            .saturating_sub(self.envelope_allowance)
            .saturating_sub(self.completion_reserve)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(provider: ProviderId, model: &str) -> Target {
        Target {
            provider,
            model: model.to_string(),
        }
    }

    #[test]
    fn estimate_ascii_is_two_bytes_per_token() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 2);
        assert_eq!(estimate_tokens("abcde"), 3);
    }

    #[test]
    fn estimate_non_ascii_is_one_char_per_token() {
        let text = "файл привет";
        assert_eq!(estimate_tokens(text), text.chars().count());
        assert!(estimate_tokens(text) > text.len() / 2 / 2);
    }

    const COLLISION: &[WindowEntry] = &[
        WindowEntry {
            provider: ProviderId::Groq,
            model: "fam",
            by: Match::Prefix,
            tokens: 1_000,
        },
        WindowEntry {
            provider: ProviderId::Groq,
            model: "fam-large",
            by: Match::Prefix,
            tokens: 9_000,
        },
        WindowEntry {
            provider: ProviderId::Groq,
            model: "fam-large-x",
            by: Match::Exact,
            tokens: 5_000,
        },
    ];

    #[test]
    fn context_window_exact_beats_prefix() {
        assert_eq!(
            lookup(COLLISION, ProviderId::Groq, "fam-large-x"),
            Some(5_000)
        );
    }

    #[test]
    fn context_window_longest_prefix_wins() {
        assert_eq!(
            lookup(COLLISION, ProviderId::Groq, "fam-large-y"),
            Some(9_000)
        );
        assert_eq!(
            lookup(COLLISION, ProviderId::Groq, "fam-small"),
            Some(1_000)
        );
        assert_eq!(lookup(COLLISION, ProviderId::Openai, "fam-large"), None);
    }

    #[test]
    fn context_window_table_and_default() {
        let groq = window_from(None, &target(ProviderId::Groq, "openai/gpt-oss-120b")).unwrap();
        assert_eq!(groq.tokens, 131_072);
        assert_eq!(groq.origin, WindowOrigin::Table);
        let unknown = window_from(None, &target(ProviderId::Ollama, "gemma4:e4b-mlx")).unwrap();
        assert_eq!(unknown.tokens, DEFAULT_WINDOW);
        assert_eq!(unknown.origin, WindowOrigin::Default);
    }

    #[test]
    fn context_window_env_override_wins() {
        let w = window_from(
            Some(" 4096 "),
            &target(ProviderId::Groq, "openai/gpt-oss-120b"),
        )
        .unwrap();
        assert_eq!(w.tokens, 4_096);
        assert_eq!(w.origin, WindowOrigin::Env);
    }

    #[test]
    fn context_window_invalid_env_is_a_config_error() {
        let t = target(ProviderId::Groq, "openai/gpt-oss-120b");
        for raw in ["", "  ", "abc", "12k", "-5", "0", "99999999999"] {
            match window_from(Some(raw), &t) {
                Err(GcmError::Config(msg)) => assert!(msg.contains(WINDOW_ENV), "{raw}: {msg}"),
                other => panic!("{raw:?} must be a config error, got {other:?}"),
            }
        }
    }

    #[test]
    fn kinds_grouping_and_message_differ() {
        let t = target(ProviderId::Groq, "openai/gpt-oss-120b");
        let files = r#"[{"path":"a.rs","xy":" M"}]"#;
        let g = PromptBudget::grouping(131_072, &t, files).unwrap();
        let m = PromptBudget::message(131_072, &t);
        assert_eq!(g.kind, BudgetKind::Grouping);
        assert_eq!(m.kind, BudgetKind::Message);
        assert!(
            g.envelope_allowance > m.envelope_allowance,
            "schema is grouping-only"
        );
        assert_ne!(g.completion_reserve, m.completion_reserve);
        assert_eq!(
            g.available(),
            131_072 - g.envelope_allowance - g.completion_reserve
        );
    }

    #[test]
    fn kinds_path_echo_reserve_grows_with_the_inventory() {
        let t = target(ProviderId::Groq, "openai/gpt-oss-120b");
        let small = PromptBudget::grouping(131_072, &t, "[]").unwrap();
        let big = PromptBudget::grouping(131_072, &t, &"x".repeat(20_000)).unwrap();
        assert!(big.completion_reserve >= small.completion_reserve + 10_000);
    }

    #[test]
    fn kinds_anthropic_output_cap_rejects_grouping_not_message() {
        let t = target(ProviderId::Anthropic, "claude-haiku-4-5");
        // ~8,000 estimated tokens of paths: the echo cannot fit in 4096.
        let files = "p".repeat(16_000);
        match PromptBudget::grouping(200_000, &t, &files) {
            Err(GroupingRejected::OutputCap { echo_tokens, cap }) => {
                assert_eq!(cap, crate::provider::ANTHROPIC_PLAN_MAX_TOKENS);
                assert!(echo_tokens > cap);
            }
            other => panic!("expected an output-cap rejection, got {other:?}"),
        }
        let m = PromptBudget::message(200_000, &t);
        assert_eq!(
            m.completion_reserve,
            crate::provider::ANTHROPIC_MESSAGE_MAX_TOKENS
        );
        assert!(m.available() > 190_000);
    }

    #[test]
    fn kinds_reasoning_allowance_follows_suppression() {
        assert_eq!(
            output_facts(&target(ProviderId::Groq, "qwen/qwen3.6-27b")).reasoning_allowance,
            0
        );
        assert_eq!(
            output_facts(&target(ProviderId::Openai, "gpt-5.6-terra")).reasoning_allowance,
            0
        );
        assert_eq!(
            output_facts(&target(ProviderId::Groq, "openai/gpt-oss-120b")).reasoning_allowance,
            REASONING_ALLOWANCE
        );
    }

    #[test]
    fn kinds_available_saturates() {
        let t = target(ProviderId::Groq, "openai/gpt-oss-120b");
        assert_eq!(PromptBudget::message(100, &t).available(), 0);
    }
}
