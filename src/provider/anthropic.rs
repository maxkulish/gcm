//! Anthropic provider (CLO-494): forced tool-use for structured output.
//! Stub — full implementation in ST4.

use crate::diff::{DiffBudget, GatheredDiff, GroupingContext};
use crate::plan::Plan;
use crate::provider::{Provider, ProviderError};

pub struct Anthropic {
    model: String,
}

impl Anthropic {
    pub fn new(model: String) -> Self {
        Anthropic { model }
    }
}

impl Provider for Anthropic {
    fn name(&self) -> &'static str {
        "Anthropic"
    }

    fn generate_plan(&self, _ctx: &GroupingContext) -> Result<Plan, ProviderError> {
        unimplemented!("ST4")
    }

    fn generate_message(&self, _diff: &GatheredDiff) -> Result<String, ProviderError> {
        unimplemented!("ST4")
    }

    fn cache_model_id(&self) -> String {
        format!("anthropic:{}", self.model)
    }

    fn diff_budget(&self) -> DiffBudget {
        DiffBudget::standard()
    }
}
