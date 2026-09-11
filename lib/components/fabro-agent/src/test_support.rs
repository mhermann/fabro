use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use fabro_llm::adapter::{ProviderAdapter, ResolvedCall};
use fabro_llm::lithos_catalog::AdapterId;
use fabro_llm::test_support::client_with_adapters;
pub use fabro_llm::test_support::{response_to_stream, test_retry_policy};
use fabro_llm::{
    Client, ClientOptions, Error as LlmError, FinishReason, Request, Response, ResponseStream,
};
pub use fabro_sandbox::test_support::{MockSandbox, MutableMockSandbox};
use fabro_types::AgentProfileKind;
use lithos_llm::catalog::{ModelId, ProviderId, builtin};
use lithos_llm::types::{ContentPart, TokenCounts, ToolCall};

use crate::agent_profile::AgentProfile;
use crate::config::SessionOptions;
use crate::native_tool::ToolVocabulary;
use crate::profiles::EnvContext;
use crate::sandbox::*;
use crate::session::Session;
use crate::skills::{Skill, format_skills_prompt_section};
use crate::tool_registry::{RegisteredTool, ToolRegistry, ToolSource};

/// The provider every test profile routes to.
pub const TEST_PROVIDER: &str = builtin::ids::ANTHROPIC;
/// The model every test profile requests. It is not in the catalog, so the
/// provider's passthrough route serves it.
pub const TEST_MODEL: &str = "mock-model";

// --- TestProfile ---

pub struct TestProfile {
    pub registry:       ToolRegistry,
    pub context_window: usize,
}

impl TestProfile {
    pub fn new() -> Self {
        Self {
            registry:       ToolRegistry::new(),
            context_window: 200_000,
        }
    }

    pub fn with_tools(registry: ToolRegistry) -> Self {
        Self {
            registry,
            context_window: 200_000,
        }
    }

    pub fn with_context_window(registry: ToolRegistry, context_window: usize) -> Self {
        Self {
            registry,
            context_window,
        }
    }
}

impl AgentProfile for TestProfile {
    fn profile_kind(&self) -> AgentProfileKind {
        AgentProfileKind::Anthropic
    }

    fn provider_id(&self) -> ProviderId {
        builtin::anthropic()
    }

    fn model(&self) -> &'static str {
        TEST_MODEL
    }

    fn tool_registry(&self) -> &ToolRegistry {
        &self.registry
    }

    fn tool_registry_mut(&mut self) -> &mut ToolRegistry {
        &mut self.registry
    }

    fn build_system_prompt(
        &self,
        _env: &dyn Sandbox,
        _env_context: &EnvContext,
        _memory: &[String],
        user_instructions: Option<&str>,
        skills: &[Skill],
    ) -> String {
        let skills_section = format_skills_prompt_section(skills, ToolVocabulary::Fabro);
        let skills_part = if skills_section.is_empty() {
            String::new()
        } else {
            format!("\n\n{skills_section}")
        };
        match user_instructions {
            Some(instructions) => format!(
                "You are a test assistant.{skills_part}\n\n# User Instructions\n{instructions}"
            ),
            None => format!("You are a test assistant.{skills_part}"),
        }
    }

    fn context_window_size(&self) -> usize {
        self.context_window
    }
}

// --- MockLlmProvider ---

/// Answers from a script of responses, repeating the last one.
pub struct MockLlmProvider {
    pub responses:  Vec<Response>,
    pub call_index: AtomicUsize,
    id:             AdapterId,
}

impl MockLlmProvider {
    pub fn new(responses: Vec<Response>) -> Self {
        Self {
            responses,
            call_index: AtomicUsize::new(0),
            id: AdapterId::new("mock"),
        }
    }

    fn next_response(&self) -> Response {
        let idx = self.call_index.fetch_add(1, Ordering::SeqCst);
        self.responses[idx.min(self.responses.len() - 1)].clone()
    }
}

#[async_trait]
impl ProviderAdapter for MockLlmProvider {
    fn id(&self) -> &AdapterId {
        &self.id
    }

    async fn complete(&self, _call: &ResolvedCall) -> Result<Response, LlmError> {
        Ok(self.next_response())
    }

    async fn stream(&self, _call: &ResolvedCall) -> Result<ResponseStream, LlmError> {
        Ok(response_to_stream(self.next_response()))
    }
}

// --- Helper functions ---

/// A response attributed to the test route with the given content parts.
pub fn response_with_parts(id: &str, parts: Vec<ContentPart>) -> Response {
    let has_tool_calls = parts
        .iter()
        .any(|part| matches!(part, ContentPart::ToolCall(_)));
    let mut response = Response::new(
        ProviderId::new(TEST_PROVIDER),
        ModelId::new(TEST_MODEL),
        parts,
    );
    response.id = Some(id.to_string());
    response.finish_reason = if has_tool_calls {
        FinishReason::ToolCall
    } else {
        FinishReason::Stop
    };
    response.usage = TokenCounts {
        input: 10,
        output: 5,
        ..TokenCounts::default()
    };
    response
}

pub fn text_response(text: &str) -> Response {
    response_with_parts(&format!("resp_{text}"), vec![ContentPart::Text {
        text: text.to_string(),
    }])
}

pub fn tool_call_response(
    tool_name: &str,
    tool_call_id: &str,
    args: serde_json::Value,
) -> Response {
    response_with_parts(&format!("resp_{tool_call_id}"), vec![
        ContentPart::Text {
            text: "Let me use a tool.".to_string(),
        },
        ContentPart::ToolCall(ToolCall::function(tool_call_id, tool_name, args)),
    ])
}

pub fn multi_tool_call_response(calls: Vec<(&str, &str, serde_json::Value)>) -> Response {
    let mut content = vec![ContentPart::Text {
        text: "Let me use multiple tools.".to_string(),
    }];
    for (tool_name, tool_call_id, args) in calls {
        content.push(ContentPart::ToolCall(ToolCall::function(
            tool_call_id,
            tool_name,
            args,
        )));
    }
    response_with_parts("resp_multi", content)
}

/// A client over the Fabro test catalog that routes the test provider to
/// `provider`, with client-side retries but no delay between attempts.
pub async fn make_client(provider: Arc<dyn ProviderAdapter>) -> Client {
    make_client_with_options(
        provider,
        ClientOptions::default().with_retry(Some(test_retry_policy())),
    )
}

/// A client over the Fabro test catalog with no client-side retries. Tests
/// that count provider calls made by the agent's own replay loop use this.
pub fn make_client_without_retries(provider: Arc<dyn ProviderAdapter>) -> Client {
    make_client_with_options(provider, ClientOptions::default())
}

pub fn make_client_with_options(
    provider: Arc<dyn ProviderAdapter>,
    options: ClientOptions,
) -> Client {
    client_with_adapters(vec![(TEST_PROVIDER, provider)], options)
}

pub async fn make_session(responses: Vec<Response>) -> Session {
    let provider = Arc::new(MockLlmProvider::new(responses));
    let client = make_client(provider).await;
    let profile = Arc::new(TestProfile::new());
    let env = Arc::new(MockSandbox::default());
    Session::new(client, profile, env, SessionOptions::default(), None)
}

pub async fn make_session_with_tools(responses: Vec<Response>, registry: ToolRegistry) -> Session {
    let provider = Arc::new(MockLlmProvider::new(responses));
    make_session_with_provider_and_tools(provider, registry).await
}

pub async fn make_session_with_provider_and_tools(
    provider: Arc<dyn ProviderAdapter>,
    registry: ToolRegistry,
) -> Session {
    let client = make_client(provider).await;
    let profile = Arc::new(TestProfile::with_tools(registry));
    let env = Arc::new(MockSandbox::default());
    Session::new(client, profile, env, SessionOptions::default(), None)
}

pub async fn make_session_with_config(responses: Vec<Response>, config: SessionOptions) -> Session {
    let provider = Arc::new(MockLlmProvider::new(responses));
    let client = make_client(provider).await;
    let profile = Arc::new(TestProfile::new());
    let env = Arc::new(MockSandbox::default());
    Session::new(client, profile, env, config, None)
}

pub async fn make_session_with_tools_and_config(
    responses: Vec<Response>,
    registry: ToolRegistry,
    config: SessionOptions,
) -> Session {
    let provider = Arc::new(MockLlmProvider::new(responses));
    let client = make_client(provider).await;
    let profile = Arc::new(TestProfile::with_tools(registry));
    let env = Arc::new(MockSandbox::default());
    Session::new(client, profile, env, config, None)
}

pub fn make_echo_tool() -> RegisteredTool {
    use lithos_llm::types::ToolDefinition;
    RegisteredTool {
        definition: ToolDefinition::function(
            "echo",
            "Echoes the input",
            serde_json::json!({"type": "object", "properties": {"text": {"type": "string"}}}),
        ),
        executor:   Arc::new(|args, _ctx| {
            Box::pin(async move {
                let text = args
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("no text");
                Ok(format!("echo: {text}"))
            })
        }),
        source:     ToolSource::Native,
    }
}

pub fn make_error_tool() -> RegisteredTool {
    use lithos_llm::types::ToolDefinition;
    RegisteredTool {
        definition: ToolDefinition::function(
            "fail_tool",
            "Always fails",
            serde_json::json!({"type": "object"}),
        ),
        executor:   Arc::new(|_args, _ctx| {
            Box::pin(async move { Err("tool execution failed".to_string()) })
        }),
        source:     ToolSource::Native,
    }
}

// --- MockErrorProvider ---

/// Fails every call with a fresh error from `factory`.
pub struct MockErrorProvider {
    factory: Box<dyn Fn() -> LlmError + Send + Sync>,
    calls:   AtomicUsize,
    id:      AdapterId,
}

impl MockErrorProvider {
    pub fn new(factory: impl Fn() -> LlmError + Send + Sync + 'static) -> Self {
        Self {
            factory: Box::new(factory),
            calls:   AtomicUsize::new(0),
            id:      AdapterId::new("mock"),
        }
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ProviderAdapter for MockErrorProvider {
    fn id(&self) -> &AdapterId {
        &self.id
    }

    async fn complete(&self, _call: &ResolvedCall) -> Result<Response, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err((self.factory)())
    }

    async fn stream(&self, _call: &ResolvedCall) -> Result<ResponseStream, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err((self.factory)())
    }
}

// --- CapturingLlmProvider ---

/// A mock LLM provider that captures the full Request for test assertions.
pub struct CapturingLlmProvider {
    pub captured_request: Mutex<Option<Request>>,
    id:                   AdapterId,
}

impl CapturingLlmProvider {
    pub fn new() -> Self {
        Self {
            captured_request: Mutex::new(None),
            id:               AdapterId::new("mock"),
        }
    }
}

#[async_trait]
impl ProviderAdapter for CapturingLlmProvider {
    fn id(&self) -> &AdapterId {
        &self.id
    }

    async fn complete(&self, call: &ResolvedCall) -> Result<Response, LlmError> {
        *self
            .captured_request
            .lock()
            .expect("captured_request lock poisoned") = Some(call.request().clone());
        Ok(text_response("captured"))
    }

    async fn stream(&self, call: &ResolvedCall) -> Result<ResponseStream, LlmError> {
        *self
            .captured_request
            .lock()
            .expect("captured_request lock poisoned") = Some(call.request().clone());
        Ok(response_to_stream(text_response("captured")))
    }
}
