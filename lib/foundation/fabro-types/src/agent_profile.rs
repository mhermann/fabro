//! Agent profile vocabulary shared by the catalog and the agent.
//!
//! The catalog records which profile a model should run under in its
//! `metadata.agent.profile` entry, a namespace lithos-llm ships and Pebble
//! reads too. This enum is the Rust spelling of that value.

use serde::{Deserialize, Serialize};
use strum::{Display, EnumString, IntoStaticStr, VariantArray};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Display,
    EnumString,
    IntoStaticStr,
    VariantArray,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum AgentProfileKind {
    Anthropic,
    /// Claude 5 models trained against Anthropic's current coding-agent
    /// harness. This remains model-scoped so older Claude models keep the
    /// established Anthropic profile.
    #[serde(rename = "claude-5")]
    #[strum(to_string = "claude-5")]
    Claude5,
    #[serde(rename = "openai")]
    #[strum(to_string = "openai")]
    OpenAi,
    Gemini,
    /// Kimi (Moonshot) models, wherever they are served from. Selected per
    /// model rather than per provider, so a Kimi model reached through a
    /// gateway such as OpenRouter gets the same profile as one reached
    /// directly at `api.moonshot.ai`.
    Kimi,
    /// GPT-5.6 models (Sol, Terra, Luna), which Codex drives with a narrower
    /// core tool set than earlier GPT models: a shell, a file editor, and
    /// `update_plan`, plus optional web search. The profile omits dedicated
    /// file-read, discovery, and fetch tools. Selected per model rather than
    /// per provider, so other models on the `openai` provider keep
    /// [`Self::OpenAi`].
    Gpt56,
    /// GPT-6 models (Astra), which Codex drives with the same narrow tool
    /// contract as GPT-5.6. Fabro runs them on the GPT-5.6 harness.
    Gpt6,
}

impl AgentProfileKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        self.into()
    }

    /// Whether the profile runs Codex's narrow core tool set (a shell, a file
    /// editor, and `update_plan`) instead of Fabro's dedicated read,
    /// discovery, and fetch tools.
    #[must_use]
    pub fn uses_codex_core_tools(self) -> bool {
        matches!(self, Self::Gpt56 | Self::Gpt6)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_profile_kind_round_trips_as_settings_strings() {
        for kind in AgentProfileKind::VARIANTS {
            let expected = kind.to_string();
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, format!("\"{expected}\""));
            let parsed: AgentProfileKind = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, *kind);
            assert_eq!(expected.parse::<AgentProfileKind>().unwrap(), *kind);
        }
    }

    #[test]
    fn claude5_and_gpt56_use_their_catalog_spellings() {
        assert_eq!(AgentProfileKind::Claude5.as_str(), "claude-5");
        assert_eq!(AgentProfileKind::Gpt56.as_str(), "gpt56");
        assert_eq!(AgentProfileKind::Gpt6.as_str(), "gpt6");
        assert!(AgentProfileKind::Gpt6.uses_codex_core_tools());
        assert!(!AgentProfileKind::OpenAi.uses_codex_core_tools());
        assert_eq!(AgentProfileKind::OpenAi.as_str(), "openai");
    }
}
