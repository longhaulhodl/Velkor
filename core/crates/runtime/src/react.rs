use crate::context::ConversationContext;
use crate::prompt::PromptBuilder;
use crate::RuntimeError;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tracing::{debug, info, warn};
use uuid::Uuid;
use velkor_audit::logger::AuditLogger;
use velkor_audit::{AuditEntryBuilder, AuditEvent};
use velkor_memory::service::MemoryService;
use velkor_memory::MemoryScope;
use velkor_models::router::ModelRouter;
use velkor_models::{
    ChatRequest, ContentBlock, LlmResponse, Message, MessageContent, Role, StopReason,
    StreamChunk, ToolCall, Usage,
};
use velkor_skills::store::SkillStore;
use velkor_tools::registry::ToolRegistry;
use velkor_tools::ToolContext;

/// Shared handle to the skill store for background skill review.
pub type SkillStoreHandle = std::sync::Arc<tokio::sync::RwLock<SkillStore>>;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Controls for the ReAct loop.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Model identifier (e.g. "anthropic/claude-sonnet-4-20250514").
    pub model: String,
    /// System prompt / persona instructions.
    pub system_prompt: String,
    /// Max ReAct iterations before we bail (prevents infinite tool loops).
    pub max_iterations: u32,
    /// How many memories to recall per turn.
    pub memory_recall_limit: usize,
    /// Temperature for model calls.
    pub temperature: Option<f32>,
    /// Max tokens per model response.
    pub max_tokens: Option<u32>,
    /// Memory scope for this agent's searches.
    pub memory_scope: MemoryScope,
    /// Enable background post-turn skill review (Hermes pattern).
    pub skill_self_improve: bool,
    /// Minimum ReAct iterations before triggering skill review.
    pub skill_review_threshold: u32,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            model: "anthropic/claude-sonnet-4-20250514".to_string(),
            system_prompt: "You are a helpful assistant.".to_string(),
            max_iterations: 25,
            memory_recall_limit: 10,
            temperature: Some(0.7),
            max_tokens: Some(4096),
            memory_scope: MemoryScope::Personal,
            skill_self_improve: true,
            skill_review_threshold: 10,
        }
    }
}

// ---------------------------------------------------------------------------
// AgentRuntime
// ---------------------------------------------------------------------------

/// The ReAct agent runtime per PRD Section 4.1.
///
/// Wires together model router, memory service, audit logger, and tool
/// registry. Two entry points:
///
/// - [`run()`](AgentRuntime::run) — non-streaming, returns final text
/// - [`run_stream()`](AgentRuntime::run_stream) — returns a
///   `Stream<Item = RuntimeEvent>` that yields text chunks in real-time,
///   tool status updates, and a completion signal
///
/// Both execute the full Reasoning + Acting loop:
/// 1. Recall relevant memories for the user message
/// 2. Build prompt with system instructions + memories + conversation history
/// 3. Stream/call the model
/// 4. If tool calls → execute tools, audit, append results, loop back to 3
/// 5. If no tool calls → return/signal completion
/// 6. Post-processing spawned as background task (memory extraction)
pub struct AgentRuntime {
    pub config: RuntimeConfig,
    pub model_router: Arc<ModelRouter>,
    pub memory: Arc<MemoryService>,
    pub audit: AuditLogger,
    pub tools: Arc<ToolRegistry>,
    pub skill_store: Option<SkillStoreHandle>,
}

impl AgentRuntime {
    pub fn new(
        config: RuntimeConfig,
        model_router: Arc<ModelRouter>,
        memory: Arc<MemoryService>,
        audit: AuditLogger,
        tools: Arc<ToolRegistry>,
    ) -> Self {
        Self {
            config,
            model_router,
            memory,
            audit,
            tools,
            skill_store: None,
        }
    }

    pub fn with_skill_store(mut self, store: SkillStoreHandle) -> Self {
        self.skill_store = Some(store);
        self
    }

    /// Non-streaming ReAct loop. Returns the final response after all tool
    /// calls are resolved.
    pub async fn run(
        &self,
        user_message: &str,
        context: &mut ConversationContext,
    ) -> Result<AgentResponse, RuntimeError> {
        let request_id = Uuid::new_v4();

        // 1. Recall relevant memories
        let memories = self
            .memory
            .search(
                user_message,
                self.config.memory_scope,
                context.user_id,
                self.config.memory_recall_limit,
            )
            .await
            .unwrap_or_else(|e| {
                warn!(error = %e, "Memory recall failed, proceeding without memories");
                Vec::new()
            });

        debug!(count = memories.len(), "Recalled memories for agent turn");

        // 2. Build system message + append user message
        let system_msg =
            PromptBuilder::build_system_prompt(&self.config.system_prompt, &memories);
        context.push(PromptBuilder::user_message(user_message));

        self.audit.log_async(
            AuditEntryBuilder::new(AuditEvent::AgentMessageReceived)
                .user_id(context.user_id)
                .agent_id(&context.agent_id)
                .conversation_id(context.conversation_id)
                .request_id(request_id)
                .details(serde_json::json!({ "content_length": user_message.len() }))
                .build(),
        );

        // 3. ReAct loop
        let mut iteration = 0u32;
        let mut total_usage = Usage::default();

        let final_text = loop {
            iteration += 1;
            if iteration > self.config.max_iterations {
                return Err(RuntimeError::MaxIterations(self.config.max_iterations));
            }

            debug!(iteration, "ReAct loop iteration");

            let mut messages = vec![system_msg.clone()];
            messages.extend(context.messages.clone());

            let tool_schemas = self.tools.schemas();
            let tools_ref = if tool_schemas.is_empty() {
                None
            } else {
                Some(tool_schemas.as_slice())
            };

            let request = ChatRequest {
                model: &self.config.model,
                messages: &messages,
                tools: tools_ref,
                temperature: self.config.temperature,
                max_tokens: self.config.max_tokens,
                stream: false,
            };

            let response: LlmResponse = self.model_router.chat(&request).await?;

            total_usage.input_tokens += response.usage.input_tokens;
            total_usage.output_tokens += response.usage.output_tokens;

            // Audit model response
            let (ip, op) = self.model_router.cost_per_token(&self.config.model);
            let call_cost = response.usage.cost(ip, op);
            self.audit.log_async(
                AuditEntryBuilder::new(AuditEvent::AgentModelResponse)
                    .user_id(context.user_id)
                    .agent_id(&context.agent_id)
                    .conversation_id(context.conversation_id)
                    .request_id(request_id)
                    .model_used(&response.model)
                    .tokens(
                        response.usage.input_tokens as i32,
                        response.usage.output_tokens as i32,
                    )
                    .cost_usd(call_cost)
                    .details(serde_json::json!({
                        "stop_reason": format!("{:?}", response.stop_reason),
                        "iteration": iteration,
                    }))
                    .build(),
            );

            // 4. Tool calls?
            if response.stop_reason == StopReason::ToolUse && !response.tool_calls.is_empty() {
                self.handle_tool_calls_non_streaming(
                    &response, context, request_id,
                )
                .await?;
                continue;
            }

            // 5. No tool calls — done
            context.push(PromptBuilder::assistant_message(&response.content));
            break response.content;
        };

        // Audit outgoing response
        self.audit.log_async(
            AuditEntryBuilder::new(AuditEvent::AgentMessageSent)
                .user_id(context.user_id)
                .agent_id(&context.agent_id)
                .conversation_id(context.conversation_id)
                .request_id(request_id)
                .details(serde_json::json!({
                    "content_length": final_text.len(),
                    "iterations": iteration,
                    "total_input_tokens": total_usage.input_tokens,
                    "total_output_tokens": total_usage.output_tokens,
                }))
                .build(),
        );

        info!(
            agent = %context.agent_id,
            iterations = iteration,
            input_tokens = total_usage.input_tokens,
            output_tokens = total_usage.output_tokens,
            "ReAct loop completed"
        );

        // 6. Post-processing in background
        self.spawn_post_processing(user_message, &final_text, context, request_id);

        Ok(AgentResponse {
            content: final_text,
            iterations: iteration,
            usage: total_usage,
            request_id,
        })
    }

    /// Streaming ReAct loop. Returns a `Stream<Item = RuntimeEvent>`.
    ///
    /// The caller (e.g. WebSocket handler) consumes the stream to get:
    /// - `RuntimeEvent::Text(chunk)` — real-time text from the model
    /// - `RuntimeEvent::ToolStatus { .. }` — tool execution notifications
    /// - `RuntimeEvent::Done { .. }` — loop complete with final metadata
    /// - `RuntimeEvent::Error(msg)` — if the loop fails
    ///
    /// The actual ReAct loop runs in a spawned task that feeds the stream.
    /// Text flows to the caller as fast as the model produces it. When tool
    /// calls arrive, tools execute silently (with status events), then the
    /// model is called again and streaming resumes.
    pub fn run_stream(
        self: Arc<Self>,
        user_message: String,
        mut context: ConversationContext,
    ) -> ReceiverStream<RuntimeEvent> {
        let (tx, rx) = mpsc::channel::<RuntimeEvent>(64);

        tokio::spawn(async move {
            let result = Self::streaming_react_loop(&self, &user_message, &mut context, &tx).await;

            match result {
                Ok(response) => {
                    let _ = tx
                        .send(RuntimeEvent::Done {
                            request_id: response.request_id,
                            iterations: response.iterations,
                            usage: response.usage,
                        })
                        .await;

                    // Post-processing in background (doesn't hold the stream open)
                    self.spawn_post_processing(
                        &user_message,
                        &response.content,
                        &context,
                        response.request_id,
                    );
                }
                Err(e) => {
                    let _ = tx.send(RuntimeEvent::Error(format!("{e}"))).await;
                }
            }
            // tx drops here, closing the stream
        });

        ReceiverStream::new(rx)
    }

    // -----------------------------------------------------------------------
    // Internal: the streaming ReAct loop (called inside the spawned task)
    // -----------------------------------------------------------------------

    async fn streaming_react_loop(
        &self,
        user_message: &str,
        context: &mut ConversationContext,
        tx: &mpsc::Sender<RuntimeEvent>,
    ) -> Result<AgentResponse, RuntimeError> {
        let request_id = Uuid::new_v4();

        // Recall memories
        let memories = self
            .memory
            .search(
                user_message,
                self.config.memory_scope,
                context.user_id,
                self.config.memory_recall_limit,
            )
            .await
            .unwrap_or_else(|e| {
                warn!(error = %e, "Memory recall failed, proceeding without memories");
                Vec::new()
            });

        let system_msg =
            PromptBuilder::build_system_prompt(&self.config.system_prompt, &memories);
        context.push(PromptBuilder::user_message(user_message));

        self.audit.log_async(
            AuditEntryBuilder::new(AuditEvent::AgentMessageReceived)
                .user_id(context.user_id)
                .agent_id(&context.agent_id)
                .conversation_id(context.conversation_id)
                .request_id(request_id)
                .details(serde_json::json!({ "content_length": user_message.len() }))
                .build(),
        );

        let mut iteration = 0u32;
        let mut total_usage = Usage::default();
        let accumulated_text;

        loop {
            iteration += 1;
            if iteration > self.config.max_iterations {
                return Err(RuntimeError::MaxIterations(self.config.max_iterations));
            }

            debug!(iteration, "Streaming ReAct iteration");

            let mut messages = vec![system_msg.clone()];
            messages.extend(context.messages.clone());

            let tool_schemas = self.tools.schemas();
            let tools_ref = if tool_schemas.is_empty() {
                None
            } else {
                Some(tool_schemas.as_slice())
            };

            let request = ChatRequest {
                model: &self.config.model,
                messages: &messages,
                tools: tools_ref,
                temperature: self.config.temperature,
                max_tokens: self.config.max_tokens,
                stream: true,
            };

            // Stream from the model
            let stream = self.model_router.chat_stream(&request).await?;
            let consumed = self.consume_stream(stream, tx).await?;

            total_usage.input_tokens += consumed.usage.input_tokens;
            total_usage.output_tokens += consumed.usage.output_tokens;

            // Audit model response
            let (ip, op) = self.model_router.cost_per_token(&self.config.model);
            let call_cost = consumed.usage.cost(ip, op);
            self.audit.log_async(
                AuditEntryBuilder::new(AuditEvent::AgentModelResponse)
                    .user_id(context.user_id)
                    .agent_id(&context.agent_id)
                    .conversation_id(context.conversation_id)
                    .request_id(request_id)
                    .model_used(&self.config.model)
                    .tokens(
                        consumed.usage.input_tokens as i32,
                        consumed.usage.output_tokens as i32,
                    )
                    .cost_usd(call_cost)
                    .details(serde_json::json!({
                        "stop_reason": format!("{:?}", consumed.stop_reason),
                        "iteration": iteration,
                    }))
                    .build(),
            );

            // Tool calls? Execute them, then loop back
            if consumed.stop_reason == StopReason::ToolUse && !consumed.tool_calls.is_empty() {
                // Add assistant message with tool use blocks
                let mut blocks = Vec::new();
                if !consumed.text.is_empty() {
                    blocks.push(ContentBlock::Text {
                        text: consumed.text.clone(),
                    });
                }
                for tc in &consumed.tool_calls {
                    blocks.push(ContentBlock::ToolUse {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        input: tc.input.clone(),
                    });
                }
                context.push(Message {
                    role: Role::Assistant,
                    content: MessageContent::Blocks(blocks),
                });

                // Execute tools with status events
                let tool_ctx = ToolContext {
                    user_id: context.user_id,
                    conversation_id: context.conversation_id,
                    agent_id: context.agent_id.clone(),
                };

                for tc in &consumed.tool_calls {
                    let _ = tx
                        .send(RuntimeEvent::ToolStatus {
                            tool: tc.name.clone(),
                            status: ToolStatusKind::Started,
                        })
                        .await;

                    self.audit.log_async(
                        AuditEntryBuilder::new(AuditEvent::AgentToolCalled)
                            .user_id(context.user_id)
                            .agent_id(&context.agent_id)
                            .conversation_id(context.conversation_id)
                            .request_id(request_id)
                            .details(serde_json::json!({
                                "tool": tc.name,
                                "input": tc.input,
                            }))
                            .build(),
                    );

                    let result = match self
                        .tools
                        .execute(&tc.name, tc.input.clone(), &tool_ctx)
                        .await
                    {
                        Ok(r) => r,
                        Err(e) => {
                            warn!(tool = %tc.name, error = %e, "Tool execution failed");
                            velkor_tools::ToolResult::error(format!("Tool error: {e}"))
                        }
                    };

                    let _ = tx
                        .send(RuntimeEvent::ToolStatus {
                            tool: tc.name.clone(),
                            status: if result.is_error {
                                ToolStatusKind::Failed
                            } else {
                                ToolStatusKind::Completed
                            },
                        })
                        .await;

                    self.audit.log_async(
                        AuditEntryBuilder::new(AuditEvent::AgentToolResult)
                            .user_id(context.user_id)
                            .agent_id(&context.agent_id)
                            .conversation_id(context.conversation_id)
                            .request_id(request_id)
                            .details(serde_json::json!({
                                "tool": tc.name,
                                "is_error": result.is_error,
                                "output_summary": result.summary(200),
                            }))
                            .build(),
                    );

                    context.push(PromptBuilder::tool_result_message(
                        &tc.id,
                        &result.content,
                        result.is_error,
                    ));
                }

                continue; // Loop back → call model again with tool results
            }

            // No tool calls — response complete
            accumulated_text = consumed.text;
            context.push(PromptBuilder::assistant_message(&accumulated_text));
            break;
        }

        // Audit completion
        self.audit.log_async(
            AuditEntryBuilder::new(AuditEvent::AgentMessageSent)
                .user_id(context.user_id)
                .agent_id(&context.agent_id)
                .conversation_id(context.conversation_id)
                .request_id(request_id)
                .details(serde_json::json!({
                    "content_length": accumulated_text.len(),
                    "iterations": iteration,
                    "total_input_tokens": total_usage.input_tokens,
                    "total_output_tokens": total_usage.output_tokens,
                }))
                .build(),
        );

        info!(
            agent = %context.agent_id,
            iterations = iteration,
            input_tokens = total_usage.input_tokens,
            output_tokens = total_usage.output_tokens,
            "Streaming ReAct loop completed"
        );

        Ok(AgentResponse {
            content: accumulated_text,
            iterations: iteration,
            usage: total_usage,
            request_id,
        })
    }

    // -----------------------------------------------------------------------
    // Internal: consume a provider stream, forward text, accumulate tools
    // -----------------------------------------------------------------------

    async fn consume_stream(
        &self,
        stream: velkor_models::StreamResult,
        tx: &mpsc::Sender<RuntimeEvent>,
    ) -> Result<StreamConsumeResult, RuntimeError> {
        let mut stream = stream;
        let mut text = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut tool_index: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut usage = Usage::default();
        let mut stop_reason = StopReason::EndTurn;

        while let Some(chunk) = stream.next().await {
            match chunk {
                StreamChunk::Text(t) => {
                    text.push_str(&t);
                    let _ = tx.send(RuntimeEvent::Text(t)).await;
                }
                StreamChunk::ToolCallStart { id, name } => {
                    let idx = tool_calls.len();
                    tool_calls.push(ToolCall {
                        id: id.clone(),
                        name,
                        input: serde_json::Value::Null,
                    });
                    tool_index.insert(id, idx);
                }
                StreamChunk::ToolCallDelta { id, json_delta } => {
                    if let Some(&idx) = tool_index.get(&id) {
                        let tc = &mut tool_calls[idx];
                        if tc.input.is_null() {
                            tc.input = serde_json::Value::String(json_delta);
                        } else if let serde_json::Value::String(ref mut s) = tc.input {
                            s.push_str(&json_delta);
                        }
                    }
                }
                StreamChunk::Done {
                    usage: chunk_usage,
                    stop_reason: sr,
                } => {
                    usage = chunk_usage;
                    stop_reason = sr;
                }
                StreamChunk::Error(e) => {
                    warn!(error = %e, "Stream error from provider");
                }
            }
        }

        // Parse accumulated JSON string fragments into proper values
        for tc in &mut tool_calls {
            if let serde_json::Value::String(ref raw) = tc.input {
                match serde_json::from_str(raw) {
                    Ok(parsed) => tc.input = parsed,
                    Err(e) => {
                        warn!(tool = %tc.name, error = %e, "Failed to parse tool input JSON");
                        tc.input = serde_json::json!({});
                    }
                }
            }
        }

        Ok(StreamConsumeResult {
            text,
            tool_calls,
            usage,
            stop_reason,
        })
    }

    // -----------------------------------------------------------------------
    // Internal: execute tool calls for the non-streaming path
    // -----------------------------------------------------------------------

    async fn handle_tool_calls_non_streaming(
        &self,
        response: &LlmResponse,
        context: &mut ConversationContext,
        request_id: Uuid,
    ) -> Result<(), RuntimeError> {
        // Add assistant message with tool use blocks
        let mut blocks = Vec::new();
        if !response.content.is_empty() {
            blocks.push(ContentBlock::Text {
                text: response.content.clone(),
            });
        }
        for tc in &response.tool_calls {
            blocks.push(ContentBlock::ToolUse {
                id: tc.id.clone(),
                name: tc.name.clone(),
                input: tc.input.clone(),
            });
        }
        context.push(Message {
            role: Role::Assistant,
            content: MessageContent::Blocks(blocks),
        });

        let tool_ctx = ToolContext {
            user_id: context.user_id,
            conversation_id: context.conversation_id,
            agent_id: context.agent_id.clone(),
        };

        for tc in &response.tool_calls {
            self.audit.log_async(
                AuditEntryBuilder::new(AuditEvent::AgentToolCalled)
                    .user_id(context.user_id)
                    .agent_id(&context.agent_id)
                    .conversation_id(context.conversation_id)
                    .request_id(request_id)
                    .details(serde_json::json!({
                        "tool": tc.name,
                        "input": tc.input,
                    }))
                    .build(),
            );

            let result = match self.tools.execute(&tc.name, tc.input.clone(), &tool_ctx).await {
                Ok(r) => r,
                Err(e) => {
                    warn!(tool = %tc.name, error = %e, "Tool execution failed");
                    velkor_tools::ToolResult::error(format!("Tool error: {e}"))
                }
            };

            self.audit.log_async(
                AuditEntryBuilder::new(AuditEvent::AgentToolResult)
                    .user_id(context.user_id)
                    .agent_id(&context.agent_id)
                    .conversation_id(context.conversation_id)
                    .request_id(request_id)
                    .details(serde_json::json!({
                        "tool": tc.name,
                        "is_error": result.is_error,
                        "output_summary": result.summary(200),
                    }))
                    .build(),
            );

            context.push(PromptBuilder::tool_result_message(
                &tc.id,
                &result.content,
                result.is_error,
            ));
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Post-processing: background tasks after response is returned
    // -----------------------------------------------------------------------

    fn spawn_post_processing(
        &self,
        user_message: &str,
        response_text: &str,
        context: &ConversationContext,
        request_id: Uuid,
    ) {
        self.spawn_memory_extraction(user_message, response_text, context, request_id);
        self.spawn_skill_review(context, request_id);
    }

    /// Background memory extraction (auto-store substantive conversation facts).
    fn spawn_memory_extraction(
        &self,
        user_message: &str,
        response_text: &str,
        context: &ConversationContext,
        request_id: Uuid,
    ) {
        let memory = Arc::clone(&self.memory);
        let audit = self.audit.clone();
        let user_id = context.user_id;
        let conversation_id = context.conversation_id;
        let agent_id = context.agent_id.clone();
        let scope = self.config.memory_scope;
        let user_msg = user_message.to_string();
        let resp_text = response_text.to_string();

        tokio::spawn(async move {
            if resp_text.len() > 100 {
                let summary = if resp_text.len() > 500 {
                    format!(
                        "User asked: {}... Agent responded about: {}...",
                        &user_msg[..user_msg.len().min(100)],
                        &resp_text[..200]
                    )
                } else {
                    format!(
                        "User: {}... Response: {}",
                        &user_msg[..user_msg.len().min(100)],
                        &resp_text[..resp_text.len().min(200)]
                    )
                };

                match memory
                    .store(user_id, &summary, scope, None, Some(conversation_id))
                    .await
                {
                    Ok(id) => {
                        debug!(memory_id = %id, "Auto-extracted memory from conversation");
                        audit.log_async(
                            AuditEntryBuilder::new(AuditEvent::AgentMemoryStored)
                                .user_id(user_id)
                                .agent_id(&agent_id)
                                .conversation_id(conversation_id)
                                .request_id(request_id)
                                .details(serde_json::json!({
                                    "memory_id": id.to_string(),
                                    "auto_extracted": true,
                                }))
                                .build(),
                        );
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to auto-extract memory");
                    }
                }
            }
        });
    }

    /// Background skill review (Hermes pattern): after complex tasks with many
    /// iterations, spawn a review that analyzes the conversation and considers
    /// creating or updating a learned skill.
    ///
    /// Trigger: iterations >= skill_review_threshold AND skill_self_improve is on.
    /// The review uses the model router to make a single LLM call with the
    /// conversation summary + review prompt, then parses the response for
    /// skill create/patch actions.
    fn spawn_skill_review(
        &self,
        context: &ConversationContext,
        request_id: Uuid,
    ) {
        if !self.config.skill_self_improve {
            return;
        }
        let skill_store = match &self.skill_store {
            Some(s) => Arc::clone(s),
            None => return,
        };

        // Build a summary of the conversation for the review agent
        let mut conversation_summary = String::new();
        let mut iteration_count = 0u32;
        for msg in &context.messages {
            let role_str = match msg.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::System | Role::Tool => continue,
            };
            let content_str = match &msg.content {
                MessageContent::Text(t) => t.clone(),
                MessageContent::Blocks(blocks) => {
                    let mut parts = Vec::new();
                    for block in blocks {
                        match block {
                            ContentBlock::Text { text } => parts.push(text.clone()),
                            ContentBlock::ToolUse { name, .. } => {
                                parts.push(format!("[tool_use: {name}]"));
                                iteration_count += 1;
                            }
                            ContentBlock::ToolResult { content, .. } => {
                                let truncated: String =
                                    content.chars().take(200).collect();
                                parts.push(format!("[tool_result: {truncated}...]"));
                            }
                        }
                    }
                    parts.join("\n")
                }
            };
            conversation_summary.push_str(&format!("{role_str}: {content_str}\n\n"));
        }

        // Only trigger review if we hit the iteration threshold
        if iteration_count < self.config.skill_review_threshold {
            return;
        }

        let model_router = Arc::clone(&self.model_router);
        let model = self.config.model.clone();
        let audit = self.audit.clone();
        let user_id = context.user_id;
        let conversation_id = context.conversation_id;
        let agent_id = context.agent_id.clone();

        info!(
            iterations = iteration_count,
            threshold = self.config.skill_review_threshold,
            "Triggering background skill review"
        );

        tokio::spawn(async move {
            let review_prompt = velkor_skills::index::build_review_prompt();

            // Summarize existing learned skills for context
            let existing_skills = {
                let store = skill_store.read().await;
                let summaries = store.all_skill_summaries().await;
                if summaries.is_empty() {
                    "No existing skills.".to_string()
                } else {
                    summaries
                        .iter()
                        .map(|(name, desc, source)| format!("- {name} [{source}]: {desc}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            };

            let review_system = format!(
                "{review_prompt}\n\n## Existing Skills\n{existing_skills}"
            );

            let messages = vec![
                Message {
                    role: Role::System,
                    content: MessageContent::Text(review_system),
                },
                Message {
                    role: Role::User,
                    content: MessageContent::Text(format!(
                        "Review this conversation and decide if a skill should be created or updated:\n\n{conversation_summary}"
                    )),
                },
            ];

            let request = ChatRequest {
                model: &model,
                messages: &messages,
                tools: None,
                temperature: Some(0.3),
                max_tokens: Some(2048),
                stream: false,
            };

            match model_router.chat(&request).await {
                Ok(response) => {
                    let text = response.content.trim().to_string();

                    // If the model says nothing to save, we're done
                    if text.to_lowercase().contains("nothing to save") {
                        debug!("Skill review: nothing to save");
                        return;
                    }

                    // Parse the response for skill actions.
                    // The model should respond with structured instructions:
                    //   ACTION: create|patch
                    //   NAME: skill-name
                    //   DESCRIPTION: ...
                    //   CONTENT: ...
                    let action = if text.contains("ACTION: create") {
                        "create"
                    } else if text.contains("ACTION: patch") {
                        "patch"
                    } else {
                        debug!("Skill review response did not contain a clear action");
                        return;
                    };

                    let name = extract_field(&text, "NAME:");
                    let description = extract_field(&text, "DESCRIPTION:");
                    let content = extract_field(&text, "CONTENT:");

                    let (Some(name), Some(content)) = (name, content) else {
                        debug!("Skill review: missing NAME or CONTENT in response");
                        return;
                    };

                    let store = skill_store.write().await;
                    match action {
                        "create" => {
                            match store
                                .create_learned(
                                    &name,
                                    description.as_deref(),
                                    &content,
                                    None,
                                    "skill-review",
                                    Some(conversation_id),
                                )
                                .await
                            {
                                Ok(skill) => {
                                    info!(
                                        name = %skill.name,
                                        id = %skill.id,
                                        "Background skill review created new skill"
                                    );
                                    audit.log_async(
                                        AuditEntryBuilder::new(AuditEvent::AgentToolResult)
                                            .user_id(user_id)
                                            .agent_id(&agent_id)
                                            .conversation_id(conversation_id)
                                            .request_id(request_id)
                                            .details(serde_json::json!({
                                                "skill_review": "created",
                                                "skill_name": skill.name,
                                                "skill_id": skill.id.to_string(),
                                            }))
                                            .build(),
                                    );
                                }
                                Err(e) => {
                                    warn!(error = %e, name = %name, "Failed to create skill from review");
                                }
                            }
                        }
                        "patch" => {
                            match store.get_learned_by_name(&name).await {
                                Ok(Some(existing)) => {
                                    if let Err(e) = store
                                        .patch_learned(existing.id, &content, description.as_deref())
                                        .await
                                    {
                                        warn!(error = %e, name = %name, "Failed to patch skill from review");
                                    } else {
                                        info!(name = %name, "Background skill review patched skill");
                                        audit.log_async(
                                            AuditEntryBuilder::new(AuditEvent::AgentToolResult)
                                                .user_id(user_id)
                                                .agent_id(&agent_id)
                                                .conversation_id(conversation_id)
                                                .request_id(request_id)
                                                .details(serde_json::json!({
                                                    "skill_review": "patched",
                                                    "skill_name": name,
                                                }))
                                                .build(),
                                        );
                                    }
                                }
                                _ => {
                                    warn!(name = %name, "Skill review tried to patch non-existent skill, creating instead");
                                    let _ = store
                                        .create_learned(
                                            &name,
                                            description.as_deref(),
                                            &content,
                                            None,
                                            "skill-review",
                                            Some(conversation_id),
                                        )
                                        .await;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Background skill review model call failed");
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Internal: stream consume result
// ---------------------------------------------------------------------------

struct StreamConsumeResult {
    text: String,
    tool_calls: Vec<ToolCall>,
    usage: Usage,
    stop_reason: StopReason,
}

// ---------------------------------------------------------------------------
// Runtime events — the stream type returned by run_stream()
// ---------------------------------------------------------------------------

/// Events yielded by the streaming ReAct loop.
///
/// The WebSocket/API handler maps these to wire format for the client:
/// - `Text` → stream to chat UI immediately
/// - `ToolStatus` → show "Searching web..." / "Reading document..." indicators
/// - `Done` → close the stream, return metadata
/// - `Error` → surface error to client
#[derive(Debug, Clone)]
pub enum RuntimeEvent {
    /// A chunk of text from the model, forwarded in real-time.
    Text(String),
    /// Status update about a tool being executed.
    ToolStatus {
        tool: String,
        status: ToolStatusKind,
    },
    /// The ReAct loop completed successfully.
    Done {
        request_id: Uuid,
        iterations: u32,
        usage: Usage,
    },
    /// The ReAct loop encountered a fatal error.
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolStatusKind {
    Started,
    Completed,
    Failed,
}

// ---------------------------------------------------------------------------
// Response type (returned by non-streaming run())
// ---------------------------------------------------------------------------

/// The result of a complete ReAct loop execution.
#[derive(Debug)]
pub struct AgentResponse {
    /// The final text response from the agent.
    pub content: String,
    /// How many ReAct iterations were needed (1 = no tool calls).
    pub iterations: u32,
    /// Cumulative token usage across all model calls in the loop.
    pub usage: Usage,
    /// Correlation ID for audit trail.
    pub request_id: Uuid,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract a field value from a structured text response.
/// Looks for "PREFIX value" and returns everything from the value to the next
/// known field marker or end of text.
fn extract_field(text: &str, prefix: &str) -> Option<String> {
    let start = text.find(prefix)?;
    let after = &text[start + prefix.len()..];

    // Find the end: next field marker or end of text
    let markers = ["ACTION:", "NAME:", "DESCRIPTION:", "CONTENT:"];
    let end = markers
        .iter()
        .filter(|&&m| m != prefix)
        .filter_map(|m| after.find(m))
        .min()
        .unwrap_or(after.len());

    let value = after[..end].trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}
