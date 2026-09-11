use std::sync::Arc;

use fabro_llm::catalog;
use fabro_llm::lithos_catalog::{Catalog, Offering};
use fabro_types::AgentProfileKind;
use lithos_llm::catalog::ProviderId;
use lithos_llm::types::ToolDefinition;

use crate::profiles::EnvContext;
use crate::sandbox::Sandbox;
use crate::skills::Skill;
use crate::subagent::{
    SessionFactory, SubAgentSupervisor, make_close_agent_tool, make_send_input_tool,
    make_spawn_agent_tool, make_wait_tool,
};
use crate::tool_registry::ToolRegistry;

/// Context window assumed for a model the catalog does not describe.
pub const DEFAULT_CONTEXT_WINDOW_TOKENS: usize = 200_000;

pub trait AgentProfile: Send + Sync {
    fn profile_kind(&self) -> AgentProfileKind;
    fn provider_id(&self) -> ProviderId;
    fn model(&self) -> &str;
    fn catalog(&self) -> Option<&Arc<Catalog>> {
        None
    }
    fn tool_registry(&self) -> &ToolRegistry;
    fn tool_registry_mut(&mut self) -> &mut ToolRegistry;
    fn build_system_prompt(
        &self,
        env: &dyn Sandbox,
        env_context: &EnvContext,
        memory: &[String],
        user_instructions: Option<&str>,
        skills: &[Skill],
    ) -> String;

    fn tools(&self) -> Vec<ToolDefinition> {
        self.tool_registry().definitions()
    }

    fn knowledge_cutoff(&self) -> Option<String> {
        self.catalog_model()
            .and_then(|entry| entry.model.knowledge_cutoff().map(str::to_string))
    }

    /// The catalog row for this profile's route, when the catalog knows it.
    fn catalog_model(&self) -> Option<Offering<'_>> {
        self.catalog()?
            .enabled_provider(self.provider_id().as_str())?
            .offering(self.model())
    }

    fn context_window_size(&self) -> usize {
        self.catalog_model()
            .and_then(|entry| entry.model.limits())
            .map_or(DEFAULT_CONTEXT_WINDOW_TOKENS, |limits| {
                usize::try_from(limits.context_tokens).unwrap_or(usize::MAX)
            })
    }

    fn max_output_tokens(&self) -> Option<u32> {
        self.catalog_model()
            .and_then(|entry| entry.model.limits())
            .map(|limits| u32::try_from(limits.max_output_tokens).unwrap_or(u32::MAX))
    }

    fn reasons_by_default(&self) -> bool {
        self.catalog_model()
            .is_some_and(|entry| catalog::reasons_by_default(&entry))
    }

    fn register_subagent_tools(
        &mut self,
        supervisor: SubAgentSupervisor,
        session_factory: SessionFactory,
        current_depth: usize,
    ) {
        self.tool_registry_mut().register(make_spawn_agent_tool(
            supervisor.clone(),
            session_factory,
            current_depth,
        ));
        self.tool_registry_mut()
            .register(make_send_input_tool(supervisor.clone()));
        self.tool_registry_mut()
            .register(make_wait_tool(supervisor.clone()));
        self.tool_registry_mut()
            .register(make_close_agent_tool(supervisor));
    }
}

#[cfg(test)]
mod tests {
    use fabro_types::AgentProfileKind;
    use lithos_llm::catalog::builtin;

    use super::*;
    use crate::test_support::{MockSandbox, TestProfile};

    #[test]
    fn profile_provider_and_model() {
        let profile = TestProfile::new();
        assert_eq!(profile.profile_kind(), AgentProfileKind::Anthropic);
        assert_eq!(profile.provider_id(), builtin::anthropic());
        assert_eq!(profile.model(), "mock-model");
    }

    #[test]
    fn profile_context_window_defaults() {
        let profile = TestProfile::new();
        assert_eq!(profile.context_window_size(), 200_000);
    }

    #[test]
    fn profile_build_system_prompt() {
        let profile = TestProfile::new();
        let env = MockSandbox::linux();
        let ctx = EnvContext::default();
        let docs = vec!["README.md contents".into()];
        let prompt = profile.build_system_prompt(&env, &ctx, &docs, None, &[]);
        assert!(prompt.contains("test assistant"));
    }

    #[test]
    fn profile_build_system_prompt_with_user_instructions() {
        let profile = TestProfile::new();
        let env = MockSandbox::default();
        let ctx = EnvContext::default();
        let prompt = profile.build_system_prompt(&env, &ctx, &[], Some("Always use TDD"), &[]);
        assert!(prompt.contains("Always use TDD"));
    }

    #[test]
    fn profile_tools_empty_registry() {
        let profile = TestProfile::new();
        assert!(profile.tools().is_empty());
    }
}
