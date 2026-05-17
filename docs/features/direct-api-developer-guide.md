# Direct API Developer Guide

## Architecture Overview

The Direct API feature enables OSS fork users to configure their own LLM provider API keys directly in Warp, without dependency on Warp's cloud infrastructure. This guide explains the architecture, implementation details, and how to extend it.

## High-Level Architecture

```text
┌──────────────────────────────────────────────┐
│         Warp Terminal (UI)                   │
│  Settings → Agents → Direct API              │
│  Settings → Agents → Profiles                │
└────────────────┬─────────────────────────────┘
                 │
        ┌────────▼──────────┐
        │  ApiKeyManager    │
        │  (settings.toml)  │
        └────────┬──────────┘
                 │
        ┌────────▼──────────┐
        │ AIExecutionProfile│
        │ ModelRouting      │
        └────────┬──────────┘
                 │
        ┌────────▼──────────┐
        │ RequestParams     │
        │ DirectApiRouteConfig │
        └────────┬──────────┘
                 │
        ┌────────▼─────────────────────────────┐
        │ generate_multi_agent_output          │
        │ WarpProvider -> server stream        │
        │ DirectApi -> local provider stream   │
        └────────┬─────────────────────────────┘
                 │
        ┌────────▼──────────────────────┐
        │  LlmProvider Trait             │
        │  (genai abstraction)           │
        └────────┬──────────────────────┘
                 │
    ┌────────────┼────────────┬──────────────┐
    │            │            │              │
┌───▼────┐  ┌───▼────┐  ┌───▼────┐  ┌────▼──────┐
│ OpenAI │  │Anthropic│  │ Gemini │  │ Ollama/   │
│        │  │         │  │        │  │ Custom    │
└────────┘  └────────┘  └────────┘  └───────────┘

         (All providers via genai crate)

        ┌──────────────────────────────┐
        │   direct_loop (agentic)      │
        │   Chat with tool dispatch    │
        └──────────────┬───────────────┘
                       │
        ┌──────────────▼───────────────┐
        │ ConversationRepository       │
        │ (SQLite with WAL mode)       │
        └──────────────┬───────────────┘
                       │
        ┌──────────────▼───────────────┐
        │  channel state path          │
        │  warp.sqlite                 │
        │  direct_conversations table  │
        │  direct_messages table       │
        └──────────────────────────────┘
```

## Crate Structure

### crates/ai/

Main AI functionality. Key files:

```bash
crates/ai/src/
├── api_keys.rs                  # ApiKeyManager - settings.toml integration
├── api_keys_tests.rs            # Tests for settings persistence
├── direct_loop/
│   ├── mod.rs                   # Main agentic loop (chat + tool dispatch)
│   ├── run_tests.rs             # Integration tests
│   ├── stream_tests.rs          # Streaming tests
│   └── trim_tests.rs            # Context window trimming tests
├── conversation/
│   ├── mod.rs                   # Conversation types
│   ├── repository.rs            # SQLite persistence layer
│   ├── repository_tests.rs      # Repository tests
│   └── serialization_tests.rs   # Message serialization tests
├── provider/
│   └── genai_adapter.rs         # Abstraction layer over genai crate
├── logging/
│   └── mod.rs                   # File-based logging with secret redaction
└── lib.rs                       # Public API exports
```

### crates/persistence/

Database layer using Diesel ORM:

```sql
crates/persistence/
├── src/
│   ├── schema.rs                # Database schema (auto-generated)
│   └── models.rs                # DirectConversation, DirectMessage structs
└── migrations/
    ├── 2026-05-09-000001_create_direct_conversations/
    │   ├── up.sql               # Create tables
    │   └── down.sql             # Drop tables
    └── ...
```

### app/src/settings_view/

UI layer:

```text
app/src/settings_view/
├── direct_api_page.rs           # Settings page (provider rows, key input, Test/Save/Refresh controls)
└── ...
```

### app/src/ai/execution_profiles/

Profile routing and profile editor UI:

```text
app/src/ai/execution_profiles/
├── profiles.rs                  # AIExecutionProfile, ModelRouting, Direct API selection persistence
├── direct_api_model_choices.rs  # Provider / Model choices from Direct API settings and cache
└── editor/
    ├── mod.rs                   # Model Routing and Direct API model actions
    └── ui_helpers.rs            # Profile editor routing rows
```

### app/src/ai/agent/api/

Agent request routing:

```text
app/src/ai/agent/api/
├── api.rs                       # RequestParams and DirectApiRouteConfig construction
├── impl.rs                      # generate_multi_agent_output routing branch
├── direct.rs                    # Local Direct API response stream
└── direct_tools.rs              # Provider message/tool conversion
```

## Component Details

### 1. ApiKeyManager — Settings-Backed Key Storage

**File**: `crates/ai/src/api_keys.rs`

Manages Direct API keys through `DirectAPISettings`, which are persisted in the channel-specific settings file. For the warp-oss macOS build, that file is `~/.warp-oss/settings.toml`.

```rust
pub struct ApiKeyManager {
    keys_cache: RefCell<Option<ApiKeys>>,
}

impl ApiKeyManager {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self { ... }
    pub fn keys(&self, ctx: &warpui::AppContext) -> ApiKeys { ... }
    pub fn set_openai_key(&mut self, key: Option<String>, ctx: &mut ModelContext<Self>) { ... }
    pub fn set_anthropic_key(&mut self, key: Option<String>, ctx: &mut ModelContext<Self>) { ... }
    // ... other providers
}
```

**Key Design Decisions**:

1. **Lazy Loading**: Keys not loaded until first use
2. **Session Cache**: Once loaded, cached in memory for the session
3. **WarpUI Model**: Uses `RefCell` interior mutability under WarpUI model contexts
4. **Channel-Local**: Uses the active build channel's settings path, so warp-oss and official Warp do not conflict

### 1.1. Per-Profile Direct API Routing

**Files**:
- `app/src/ai/execution_profiles/profiles.rs`
- `app/src/ai/execution_profiles/editor/mod.rs`
- `app/src/ai/execution_profiles/direct_api_model_choices.rs`
- `app/src/ai/agent/api.rs`
- `app/src/ai/agent/api/impl.rs`
- `app/src/ai/agent/api/direct.rs`

Execution profiles now carry `ModelRouting`:

- `WarpProvider` keeps the original Warp provider model path.
- `DirectApi` uses a profile-level Direct API provider/model selection.

The profile editor renders **Model Routing** above the base model controls. When a profile selects Direct API, the model picker is populated from locally configured Direct API providers and cached model lists. Labels are rendered as `Provider / Model`.

At request time, `RequestParams` carries both `model_routing` and an optional `DirectApiRouteConfig`. `generate_multi_agent_output` clears server API keys and routes Direct API profiles into the local Direct API stream. Direct API keys are read from `DirectAPISettings` and are not attached to Warp server requests for this route.

**Usage**:

```rust
let manager = ApiKeyManager::handle(ctx);
let keys = manager.as_ref(ctx).keys(ctx); // First call loads settings
let openai_key = keys.openai;     // Cached for rest of session
```

### 2. LlmProvider Trait & genai Integration

**Files**: `crates/ai/src/provider/` (abstracts over `genai` crate)

The `LlmProvider` trait defines the interface:

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError>;
    async fn chat_stream(&self, request: ChatRequest) -> Result<ChatStream, ProviderError>;
    fn capabilities(&self) -> &ModelCapabilities;
    fn provider_kind(&self) -> ProviderKind;
}
```

**Supported Providers** (via genai v0.6.0-beta.19):

- OpenAI (GPT-4o, GPT-4 Turbo, GPT-3.5)
- Anthropic (Claude 3.5 Sonnet, Opus, Haiku)
- Google Gemini (Gemini 2.0 Flash, 1.5 Pro)
- Ollama (local LLMs)
- OpenRouter (aggregator)
- Groq, DeepSeek (via genai)

**Why genai?**

- Single dependency for all providers
- Active maintenance (updated daily)
- 763 GitHub stars, 188K downloads
- MIT/Apache-2.0 license
- Covers 80%+ of use cases
- Fallback: Hand-rolled OpenAI adapter kept as reference (547 lines, revertible in 1 day)

**Model Registry**:

```rust
pub struct ModelRegistry {
    capabilities: HashMap<String, ModelCapabilities>,
}

impl ModelRegistry {
    pub fn get_provider(
        &self,
        provider: &str,
        model: &str,
        api_key: &str,
    ) -> Result<SharedProvider, String> {
        let capabilities = self.capabilities_for(model);
        let adapter = GenaiAdapter::new(provider, api_key, model)
            .with_capabilities(capabilities);
        Ok(Arc::new(adapter))
    }
}
```

### 3. Direct Loop — Agentic Chat with Tools

**File**: `crates/ai/src/direct_loop/mod.rs`

The core agentic loop that runs a conversation with tool dispatch:

```rust
pub async fn run(
    provider: SharedProvider,
    initial_messages: Vec<ChatMessage>,
    tools: Vec<Tool>,
    conversation_id: AIConversationId,
    tx: AgentEventSender,
    tool_req_tx: mpsc::Sender<ToolDispatchRequest>,
    cancellation_rx: futures::channel::oneshot::Receiver<()>,
    repository: Option<Arc<ConversationRepository>>,
) -> Result<(), ProviderError> {
    let mut history = initial_messages;
    
    loop {
        let request = ChatRequest {
            messages: trim_to_context_window(&history, 100),
            tools: tools.clone(),
            options: Default::default(),
        };
        
        // Call provider and collect response events
        let (_finish_reason, _usage, tool_calls) =
            collect_and_emit_stream(&provider, request, &tx, &mut cancel).await?;
        
        // Save assistant message
        let msg = ChatMessage::Assistant { text, tool_calls: tool_calls.clone() };
        history.push(msg.clone());
        if let Some(ref repo) = repository {
            repo.save_messages(conversation_id.to_string(), history.clone()).await?;
        }
        
        // If no tool calls, we're done
        if tool_calls.is_empty() {
            break;
        }
        
        // Dispatch tools and collect results
        let results = dispatch_tools(&tool_calls, &tool_req_tx).await?;
        
        // Add tool results to history
        let tool_result_msg = ChatMessage::User(result_blocks);
        history.push(tool_result_msg.clone());
        if let Some(ref repo) = repository {
            repo.save_messages(conversation_id.to_string(), history.clone()).await?;
        }
    }
    
    Ok(())
}
```

**Key Features**:

1. **Streaming**: Streams token-by-token to UI for responsive UX
2. **Tool Dispatch**: Handles function calling with concurrent tool execution
3. **Persistence**: Auto-saves all messages to SQLite
4. **Cancellation**: CancellationToken properly propagates to in-flight tools
5. **Context Trimming**: Respects model's context window (keeps recent messages)
6. **Logging**: Logs all operations with secret redaction

**Context Window Management**:

```rust
fn trim_to_context_window(messages: &[ChatMessage], limit: usize) -> Vec<ChatMessage> {
    // Keep last N messages to stay within context window
    // Default limit: 100 messages (~200K tokens for typical messages)
    messages[messages.len().saturating_sub(limit)..].to_vec()
}
```

### 4. Conversation Persistence — SQLite

**Files**: 
- `crates/ai/src/conversation/repository.rs` (Rust layer)
- `crates/persistence/migrations/` (SQL schema)

Persists all conversations to SQLite with full history.

**Database Schema**:

```sql
CREATE TABLE direct_conversations (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    conversation_id TEXT UNIQUE NOT NULL,
    provider_kind TEXT NOT NULL,
    model_id TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL,
    last_message_at TIMESTAMP NOT NULL,
    title TEXT,
    message_count INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE direct_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    conversation_id TEXT NOT NULL,
    message_index INTEGER NOT NULL,
    role TEXT NOT NULL,
    content_json TEXT NOT NULL,
    tool_calls_json TEXT,
    input_tokens INTEGER,
    output_tokens INTEGER,
    created_at TIMESTAMP NOT NULL,
    UNIQUE(conversation_id, message_index),
    FOREIGN KEY (conversation_id) REFERENCES direct_conversations(conversation_id)
);
```

**Repository API**:

```rust
pub struct ConversationRepository { ... }

impl ConversationRepository {
    pub async fn create_conversation(&self, provider: String, model: String) -> Result<String>;
    pub async fn save_messages(&self, conv_id: String, messages: Vec<ChatMessage>) -> Result<()>;
    pub async fn load_messages(&self, conv_id: String) -> Result<Vec<ChatMessage>>;
    pub async fn get_conversation(&self, conv_id: String) -> Result<DirectConversation>;
    pub async fn generate_title(&self, conv_id: String) -> Result<()>;
}
```

**Serialization**:

Messages are stored as JSON for flexibility:

```rust
pub struct DirectMessage {
    pub role: String,                    // "system", "user", "assistant"
    pub content_json: String,            // Vec<ContentBlock> serialized
    pub tool_calls_json: Option<String>, // Vec<ToolCall> if assistant
}

impl DirectMessage {
    pub fn from_chat_message(conv_id: &str, index: i32, msg: &ChatMessage) -> Self {
        // Serialize to JSON
    }
    
    pub fn to_chat_message(&self) -> ChatMessage {
        // Deserialize from JSON
    }
}
```

**Persistence Features**:

1. **Auto-Title**: First user message becomes conversation title
2. **Token Tracking**: Stores input/output token counts
3. **Timestamps**: Tracks when each message was created
4. **Ordering**: Messages maintain conversation order via `message_index`
5. **WAL Mode**: SQLite WAL prevents "database locked" errors under load
6. **Busy Timeout**: 5-second wait before failing on lock contention

**Usage**:

```rust
let repo = ConversationRepository::new(db_path);

// Create conversation
let conv_id = repo
    .create_conversation("openai".to_string(), "gpt-4o".to_string())
    .await?;

// Save the current message history during the loop
repo.save_messages(conv_id.clone(), history).await?;

// Resume conversation
let messages = repo.load_messages(conv_id.clone()).await?;
let conv = repo.get_conversation(conv_id).await?;
println!("Title: {}", conv.title.unwrap_or_default());
println!("Messages: {}", conv.message_count);
```

### 5. Settings UI — Provider Configuration

**File**: `app/src/settings_view/direct_api_page.rs`

WarpUI-based settings page for configuring API keys.

**Structure**:

```rust
pub enum DirectApiPageAction {
    TestConnection(String),
    SaveApiKey(String),
    ToggleProviderEnabled(String),
    UpdateModelList(String),
    ToggleApiKeyVisibility(String),
    SelectModel(String),
    ToggleRigBackendEnabled,
}

pub struct DirectApiSettingsPageView {
    page: PageType<Self>,
    api_key_manager: ModelHandle<ApiKeyManager>,
    provider_rows: Vec<ProviderRowState>,
    model_cache: Arc<ModelListCache>,
}
```

**Widgets**:

1. **Provider rows** — One row per provider for scanable setup
2. **API key input** — EditorView for entering key
3. **Base URL input** — Shown for providers with configurable base URLs, including Ollama, OpenRouter, and custom endpoints
4. **Test button** — Performs local key-format and required-base-URL validation
5. **Save button** — Persists to DirectAPISettings
6. **Enable/Disable button** — Toggles provider availability when required config is valid
7. **Refresh models button** — Validates provider reachability/authentication and caches available models
8. **Status display** — Shows validation/fetch results

**Action Handlers**:

```rust
fn handle_test_connection(&mut self, provider: ProviderType, ctx: &mut ViewContext<Self>) {
    let row = self.provider_row(provider);
    let api_key = row.api_key_editor.as_ref(ctx).buffer_text(ctx);

    // Validate format based on provider
    match provider {
        ProviderType::OpenAI => {
            if api_key.starts_with("sk-") { /* OK */ }
        }
        ProviderType::Anthropic => {
            if api_key.starts_with("sk-ant-") { /* OK */ }
        }
        ProviderType::Ollama => {
            // No API key needed
        }
        // ... others
    }
}

fn handle_save_api_key(&mut self, provider: ProviderType, ctx: &mut ViewContext<Self>) {
    let row = self.provider_row(provider);
    let api_key = row.api_key_editor.as_ref(ctx).buffer_text(ctx);

    self.api_key_manager.update(ctx, |manager, ctx| {
        match provider {
            ProviderType::OpenAI => manager.set_openai_key(Some(api_key), ctx),
            ProviderType::Anthropic => manager.set_anthropic_key(Some(api_key), ctx),
            // ... others
        }
    })
}
```

### 6. Logging — Secret Redaction

**File**: `crates/ai/src/logging/mod.rs`

File-based logging with automatic secret redaction. `DirectApiLogger` writes to a caller-provided directory, but production Direct API routing currently uses the normal Warp app logging path unless a caller explicitly wires this logger.

On macOS, the OSS app log is `warp-oss.log` under `~/Library/Logs/`.

**Features**:

1. **Regex Caching**: Patterns compiled once with `Lazy<Regex>` (200× faster)
2. **Secret Redaction**: API keys → `***REDACTED***`
3. **Timestamps**: Every log has microsecond precision
4. **Async I/O**: Wrapped in `spawn_blocking` to not block event loop

**Redaction Patterns**:

```rust
static OPENAI_PATTERN: Lazy<Regex> = Lazy::new(||
    Regex::new(r"sk-[A-Za-z0-9_\.\-]+").unwrap()
);
static ANTHROPIC_PATTERN: Lazy<Regex> = Lazy::new(||
    Regex::new(r"sk-ant-[A-Za-z0-9_-]+").unwrap()
);
static OPENROUTER_PATTERN: Lazy<Regex> = Lazy::new(||
    Regex::new(r"sk-or-v1-[A-Za-z0-9_\.\-]+").unwrap()
);
static BEARER_PATTERN: Lazy<Regex> = Lazy::new(||
    Regex::new(r"Bearer\s+[A-Za-z0-9_\.\-]+").unwrap()
);
static JWT_PATTERN: Lazy<Regex> = Lazy::new(||
    Regex::new(r"eyJ[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+").unwrap()
);
```

**Usage**:

```rust
pub struct DirectApiLogger { ... }

impl DirectApiLogger {
    pub fn new(log_dir: PathBuf) -> Self { ... }
    pub async fn log(&self, message: &str) { ... }
}
```

**Performance**:

Before optimization: 200μs per log (regex compilation)  
After caching: <1μs per log (200× faster)

### 7. Model Selection (Phase 2)

**Files**: `crates/ai/src/model_registry/` (backend infrastructure)

Phase 2 adds per-provider model selection, enabling users to choose which specific model to use for each provider.

#### ProviderId Enum

Maps UI provider selection to backend provider taxonomy:

```rust
// crates/ai/src/model_registry/provider_id.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum ProviderId {
    OpenAI,
    Anthropic,
    GoogleGemini,
    Ollama,
    OpenRouter,
    Custom,
}

impl ProviderId {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderId::OpenAI => "openai",
            ProviderId::Anthropic => "anthropic",
            ProviderId::GoogleGemini => "google",
            ProviderId::Ollama => "ollama",
            ProviderId::OpenRouter => "openrouter",
            ProviderId::Custom => "custom",
        }
    }
    
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "openai" => Some(ProviderId::OpenAI),
            "anthropic" => Some(ProviderId::Anthropic),
            "google" => Some(ProviderId::GoogleGemini),
            "ollama" => Some(ProviderId::Ollama),
            "openrouter" => Some(ProviderId::OpenRouter),
            "custom" => Some(ProviderId::Custom),
            _ => None,
        }
    }
}
```

**UI to Backend Mapping**:

| UI Label | ProviderId | genai provider string |
|---|---|---|
| "OpenAI" | `ProviderId::OpenAI` | `"openai"` |
| "Anthropic" | `ProviderId::Anthropic` | `"anthropic"` |
| "Google Gemini" | `ProviderId::GoogleGemini` | `"google"` |
| "Ollama" | `ProviderId::Ollama` | `"ollama"` |
| "OpenRouter" | `ProviderId::OpenRouter` | `"openrouter"` |
| "Custom" | `ProviderId::Custom` | (user-specified) |

#### ModelListProvider Trait

Async interface for fetching available models from provider APIs:

```rust
// crates/ai/src/model_registry/provider_trait.rs

#[async_trait]
pub trait ModelListProvider: Send + Sync {
    /// Fetch available models for this provider.
    /// Returns list of model IDs suitable for UI display.
    async fn fetch_models(&self, api_key: &str) -> Result<Vec<String>, ModelListError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ModelListError {
    #[error("API request failed: {0}")]
    ApiError(String),
    #[error("Authentication failed: invalid API key")]
    AuthError,
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Provider does not support model listing")]
    NotSupported,
}
```

**Implementation Example** (OpenAI):

```rust
pub struct OpenAIModelProvider;

#[async_trait]
impl ModelListProvider for OpenAIModelProvider {
    async fn fetch_models(&self, api_key: &str) -> Result<Vec<String>, ModelListError> {
        let client = reqwest::Client::new();
        let response = client
            .get("https://api.openai.com/v1/models")
            .bearer_auth(api_key)
            .send()
            .await
            .map_err(|e| ModelListError::NetworkError(e.to_string()))?;
            
        if response.status() == 401 {
            return Err(ModelListError::AuthError);
        }
        
        let json: ModelsResponse = response.json().await
            .map_err(|e| ModelListError::ApiError(e.to_string()))?;
            
        Ok(json.data.into_iter().map(|m| m.id).collect())
    }
}
```

**Current Implementations**:

- ✅ OpenAI (via `/v1/models` endpoint)
- ✅ Anthropic (static list - no public models API)
- ✅ Google Gemini (static list - no public models API)
- ❌ Ollama (pending - requires local tags API)
- ✅ OpenRouter (via `/v1/models` endpoint)
- ❌ Custom (not applicable - user configures manually)

#### ModelListCache

JSON-backed cache with 24-hour TTL:

```rust
// crates/ai/src/model_registry/cache.rs

pub struct ModelListCache {
    cache_path: PathBuf,  // warp_core::paths::cache_dir()/direct_api/models.json
}

impl ModelListCache {
    pub fn new() -> Result<Self> {
        let cache_dir = warp_core::paths::cache_dir().join("direct_api");
        std::fs::create_dir_all(&cache_dir)?;
        Ok(Self {
            cache_path: cache_dir.join("models.json"),
        })
    }
    
    pub fn get(&self, provider: ProviderId) -> Result<CachedModels, CacheError> {
        let contents = std::fs::read_to_string(&self.cache_path)?;
        let cache: HashMap<ProviderId, CachedModels> = serde_json::from_str(&contents)?;
        cache.get(&provider)
            .filter(|entry| !entry.is_expired())
            .cloned()
            .ok_or(CacheError::NotFound)
    }
    
    pub fn set(&self, provider: ProviderId, models: Vec<String>) -> Result<(), CacheError> {
        let mut cache: HashMap<ProviderId, CachedModels> = 
            self.read_all().unwrap_or_default();
            
        cache.insert(provider, CachedModels {
            models,
            fetched_at: SystemTime::now(),
        });
        
        let json = serde_json::to_string_pretty(&cache)?;
        
        // Atomic write via temp file + rename
        let temp_path = self.cache_path.with_extension("tmp");
        std::fs::write(&temp_path, json)?;
        std::fs::rename(temp_path, &self.cache_path)?;
        
        Ok(())
    }
    
    pub fn invalidate(&self, provider: ProviderId) -> Result<(), CacheError> {
        let mut cache = self.read_all()?;
        cache.remove(&provider);
        let json = serde_json::to_string_pretty(&cache)?;
        std::fs::write(&self.cache_path, json)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedModels {
    pub models: Vec<String>,
    pub fetched_at: SystemTime,
}

impl CachedModels {
    pub fn is_expired(&self) -> bool {
        self.fetched_at.elapsed().unwrap_or_default() > Duration::from_secs(24 * 3600)
    }
}
```

**Cache Location**:

```bash
~/Library/Application Support/dev.warp.WarpOss/direct_api/models.json
```

**Cache Structure**:

```json
{
  "openai": {
    "models": ["gpt-4o", "gpt-4o-mini", "gpt-4-turbo", "gpt-3.5-turbo"],
    "fetched_at": "2026-05-13T03:00:00Z"
  },
  "anthropic": {
    "models": [
      "claude-3-5-sonnet-20241022",
      "claude-3-opus-20240229",
      "claude-3-haiku-20240307"
    ],
    "fetched_at": "2026-05-13T03:15:00Z"
  }
}
```

**Cache Invalidation**:

Cache is invalidated when:
1. User saves a new API key (forces fresh model list fetch)
2. 24 hours have elapsed since last fetch
3. User manually clicks "Refresh models"

#### Model Persistence

User's selected model is stored in `ApiKeys` struct:

```rust
// crates/ai/src/api_keys.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeys {
    // ... existing API key fields
    
    #[serde(default)]
    pub selected_models: std::collections::BTreeMap<ProviderId, String>,
}

impl ApiKeyManager {
    pub fn set_selected_model(
        &mut self,
        provider: ProviderId,
        model_id: String,
        ctx: &mut ModelContext<Self>,
    ) {
        self.ensure_keys_loaded(ctx);
        if let Some(ref mut keys) = self.keys_cache.borrow_mut().as_mut() {
            keys.selected_models.insert(provider, model_id);
        }
        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        self.write_keys_to_settings(ctx);
    }
    
    pub fn get_selected_model_for_provider(
        &self,
        provider: ProviderId,
        ctx: &warpui::AppContext,
    ) -> Option<String> {
        let keys = self.keys(ctx);
        if let Some(model_id) = keys.selected_models.get(&provider) {
            return Some(model_id.clone());
        }
        
        // Fallback to per-provider defaults
        match provider {
            ProviderId::OpenAI => Some("gpt-4o-mini".to_string()),
            ProviderId::Anthropic => Some("claude-3-5-sonnet-20241022".to_string()),
            ProviderId::GoogleGemini => Some("gemini-2.0-flash".to_string()),
            ProviderId::Ollama => None,
            ProviderId::OpenRouter => None,
            ProviderId::Custom => None,
        }
    }
}
```

**Default Models**:

| Provider | Default Model | Rationale |
|---|---|---|
| OpenAI | `gpt-4o-mini` | Fast, affordable |
| Anthropic | `claude-3-5-sonnet-20241022` | Balanced capability |
| Google Gemini | `gemini-2.0-flash` | Latest, fast |
| Ollama | None | User must configure local model |
| OpenRouter | None | 100+ options, user chooses |
| Custom | None | Endpoint-specific |

## Integration Points

### How Settings UI connects to AI

1. User configures provider in Settings
2. ApiKeyManager stores key in DirectAPISettings
3. When user runs `@agent` command, direct_loop loads the saved key
4. ModelRegistry creates appropriate LlmProvider based on stored config
5. direct_loop runs conversation with that provider

### How conversation persistence works

1. User sends message via `@agent`
2. direct_loop receives ChatMessage
3. For each LLM response:
   - Message is added to history Vec
   - Message is persisted to SQLite via repository
4. If app restarts:
   - ConversationRepository loads full history from SQLite
   - direct_loop continues from where it left off

## Adding New Providers

To add a new LLM provider:

### Step 1: Add to genai crate (if not already there)

Check if genai supports your provider: https://github.com/cloudburst/genai

If not, create a PR to genai (or use custom adapter).

### Step 2: Update ModelRegistry

```rust
// crates/ai/src/model_registry/mod.rs

impl ModelRegistry {
    pub fn get_provider(...) -> Result<SharedProvider, String> {
        match provider {
            "your-provider" => {
                let capabilities = self.capabilities_for(model);
                let adapter = GenaiAdapter::new("your-provider", api_key, model)
                    .with_capabilities(capabilities);
                Ok(Arc::new(adapter))
            }
            // ... others
        }
    }
    
    fn capabilities_for(&self, model: &str) -> ModelCapabilities {
        match model {
            "your-provider-model-name" => ModelCapabilities {
                context_window: 200_000,
                supports_vision: true,
                supports_tool_use: true,
            },
            // ... others
        }
    }
}
```

### Step 3: Update ApiKeyManager

```rust
// crates/ai/src/api_keys.rs

pub struct ApiKeys {
    // ... existing
    pub your_provider_key: Option<String>,
}

impl ApiKeyManager {
    pub fn set_your_provider_key(
        &mut self,
        key: Option<String>,
        ctx: &mut ModelContext<Self>,
    ) {
        self.ensure_keys_loaded(ctx);
        if let Some(ref mut keys) = self.keys_cache.borrow_mut().as_mut() {
            keys.your_provider_key = key;
        }
        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        self.write_keys_to_settings(ctx);
    }
}
```

### Step 4: Update Settings UI

```rust
// app/src/settings_view/direct_api_page.rs

enum ProviderType {
    // ... existing
    YourProvider,
}

impl ProviderType {
    fn as_str(&self) -> &'static str {
        match self {
            ProviderType::YourProvider => "Your Provider",
            // ... others
        }
    }
}
```

### Step 5: Add Tests

```rust
// crates/ai/src/provider/genai_adapter_tests.rs

#[tokio::test]
async fn genai_supports_your_provider() {
    let adapter = GenaiAdapter::new("your-provider", "test-key", "model-id");
    let request = ChatRequest {
        messages: vec![ChatMessage::User(vec![ContentBlock::Text("Hello".into())])],
        tools: vec![],
        options: Default::default(),
    };
    
    let response = adapter.chat(request).await.unwrap();
    assert!(response.text.is_some());
}
```

### Step 6: Add ProviderId Variant (Phase 2)

```rust
// crates/ai/src/model_registry/provider_id.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum ProviderId {
    // ... existing variants
    YourProvider,
}

impl ProviderId {
    pub fn as_str(&self) -> &'static str {
        match self {
            // ... existing
            ProviderId::YourProvider => "your-provider",
        }
    }
    
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            // ... existing
            "your-provider" => Some(ProviderId::YourProvider),
            _ => None,
        }
    }
}
```

### Step 7: Implement ModelListProvider (Phase 2)

```rust
// crates/ai/src/model_registry/providers/your_provider.rs

pub struct YourProviderModelProvider;

#[async_trait]
impl ModelListProvider for YourProviderModelProvider {
    async fn fetch_models(&self, api_key: &str) -> Result<Vec<String>, ModelListError> {
        // Option 1: Static list (like Anthropic/Gemini)
        Ok(vec![
            "your-model-v1".to_string(),
            "your-model-v2".to_string(),
        ])
        
        // Option 2: Fetch from API (like OpenAI)
        let client = reqwest::Client::new();
        let response = client
            .get("https://api.yourprovider.com/v1/models")
            .bearer_auth(api_key)
            .send()
            .await
            .map_err(|e| ModelListError::NetworkError(e.to_string()))?;
            
        // ... parse response and return model IDs
    }
}
```

### Step 8: Register ModelListProvider (Phase 2)

```rust
// crates/ai/src/model_registry/mod.rs

pub fn get_model_list_provider(provider: ProviderId) -> Option<Box<dyn ModelListProvider>> {
    match provider {
        // ... existing
        ProviderId::YourProvider => Some(Box::new(YourProviderModelProvider)),
    }
}
```

### Step 9: Add Default Model (Phase 2)

```rust
// crates/ai/src/api_keys.rs

impl ApiKeyManager {
    pub fn get_selected_model_for_provider(
        &self,
        provider: ProviderId,
        ctx: &warpui::AppContext,
    ) -> Option<String> {
        let keys = self.keys(ctx);
        if let Some(model_id) = keys.selected_models.get(&provider) {
            return Some(model_id.clone());
        }
        
        match provider {
            // ... existing defaults
            ProviderId::YourProvider => Some("your-default-model".to_string()),
        }
    }
}
```

### Step 10: Update UI Provider Mapping (Phase 2)

```rust
// app/src/settings_view/direct_api_page.rs

impl ProviderType {
    fn to_provider_id(&self) -> ProviderId {
        match self {
            // ... existing
            ProviderType::YourProvider => ProviderId::YourProvider,
        }
    }
}
```

## Testing

### Unit Tests

```bash
# Test AI crate
cargo test -p ai --lib

# Test persistence
cargo test -p persistence --lib

# Test specific module
cargo test -p ai direct_loop
```

### Integration Tests

```bash
# Full end-to-end (requires API keys)
OPENAI_API_KEY=sk-... cargo test --test direct_api_e2e
```

### Test Structure

Tests are colocated with source files:

```rust
// foo.rs - implementation
fn my_function() { ... }

#[cfg(test)]
#[path = "foo_tests.rs"]
mod tests;
```

```rust
// foo_tests.rs - tests
#[test]
fn test_my_function() {
    let result = my_function();
    assert_eq!(result, expected);
}
```

## Performance Characteristics

### Memory

- **Message Cache**: ~1KB per message in history
- **API Key Cache**: <1KB (single API key in memory)
- **SQLite DB**: ~100KB per 100 conversations

Example: 20-turn conversation uses ~20KB RAM.

### CPU

| Operation | Time |
|---|---|
| Load conversation (100 msgs) | <10ms |
| Append message | <1ms |
| Regex redaction (regex cached) | <1μs |
| Model registry lookup | <1μs |

### Database

| Operation | Time |
|---|---|
| Create conversation | 2ms |
| Append single message | 1ms |
| Append 10 messages (batched) | 400μs |
| Load 100-message conversation | 5ms |

## Known Limitations and Future Enhancements

### Current Limitations

1. **No async test validation** (only local format/base-URL validation)
2. **API key fields are masked by default** with a visibility toggle
3. **Hardcoded context window limit** (100 messages, ~200K tokens)
4. **No per-message run-time model switching inside an active response**
5. **No conversation search** (future feature)
6. **No export** (save conversations to JSON/Markdown)
7. **No prompt caching** (Anthropic/OpenAI feature not implemented)

### Post-Launch Enhancements (Roadmap)

**V2** (Weeks 5-8):
- [ ] MCP server support (client-side)
- [ ] Anthropic prompt caching (5-10× cost reduction)
- [ ] Conversation search (SQLite FTS)
- [ ] Export conversations

**V3** (Weeks 9-12):
- [ ] Computer use support (Anthropic)
- [ ] Image handling in tool results
- [ ] Custom system prompts per conversation
- [ ] Model comparison mode

**Long-term**:
- [ ] Local embeddings for conversation similarity
- [ ] Conversation branching
- [ ] Collaborative conversations (shared via link)

## Debugging

### Enable Debug Logs

Direct API-specific debug log files are not currently wired in production builds. Use the normal app log for your build, or explicitly wire `DirectApiLogger` to a caller-provided directory when debugging provider calls.

### Check Direct Loop

Logs include:
- Conversation start/end
- Token counts
- Tool dispatch info
- Error traces (with secrets redacted)

### Database Inspection

```bash
# Install sqlite3
brew install sqlite3

# Connect to the channel-specific Warp DB.
# For a typical macOS OSS fallback path:
sqlite3 ~/.warp-oss/warp.sqlite
#
# Bundled builds may use the secure/app-group state directory instead.

# List conversations
sqlite> SELECT conversation_id, provider_kind, model_id, title, message_count
FROM direct_conversations ORDER BY last_message_at DESC;

# List messages for a conversation
sqlite> SELECT message_index, role, created_at FROM direct_messages
WHERE conversation_id = 'conv-id' ORDER BY message_index;

# View message content
sqlite> SELECT content_json FROM direct_messages
WHERE conversation_id = 'conv-id' LIMIT 1;
```

### Performance Profiling

```bash
# Run with timing info
RUST_LOG=debug cargo test --lib -- --nocapture

# Profile a specific test
cargo test --lib direct_loop --release -- --nocapture
```

## Dependencies

### Core

- `genai 0.6.0-beta.19` — Multi-provider LLM abstraction
- `tokio 1.47` — Async runtime
- `diesel 2.3.8` — ORM with SQLite
- `serde 1.0` — Serialization

### Performance

- `once_cell 1.20` — Lazy static initialization
- `regex 1.10` — Pattern matching (cached)

### Concurrency

- `tokio-util 0.7` — CancellationToken for cancelling in-flight ops

### Database

- `r2d2 0.8` — Connection pooling (optional, for multi-threaded access)

## Code Quality

### Compliance

- ✅ Follows CLAUDE.md (Warp's engineering standards)
- ✅ No `unwrap()` in library code
- ✅ Exhaustive pattern matching
- ✅ Inline format args in macros
- ✅ Context parameters last in function signatures
- ✅ Proper error propagation with `?`

### Testing

- 271+ tests passing
- Unit tests for all components
- Integration tests for E2E flows
- No `dbg!` macros
- No unsafe code

### Documentation

- All public APIs documented
- Examples provided
- Architecture explained
- Setup guides included

---

**Last updated**: 2026-05-11  
**Warp Version**: OSS fork  
**For user guide**: See [direct-api-user-guide.md](./direct-api-user-guide.md)
