use ai::conversation::repository::ConversationRepository;
use ai::direct_loop::{self, AIConversationId, ToolDispatchRequest};
use ai::logging::DirectApiLogger;
use ai::provider::{
    agent_event_channel, AgentEvent, ChatMessage, ContentBlock, GenaiAdapter, SharedProvider,
};
use futures::channel::oneshot;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::mpsc;

/// Helper to initialize test database with schema
fn init_test_db(db_path: &std::path::Path) {
    use diesel::connection::SimpleConnection;
    use diesel::prelude::*;
    let mut conn = diesel::SqliteConnection::establish(db_path.to_str().unwrap()).unwrap();
    conn.batch_execute(
        r#"
        CREATE TABLE direct_conversations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id TEXT NOT NULL UNIQUE,
            provider_kind TEXT NOT NULL,
            model_id TEXT NOT NULL,
            created_at TIMESTAMP NOT NULL,
            last_message_at TIMESTAMP NOT NULL,
            title TEXT,
            message_count INTEGER NOT NULL DEFAULT 0,
            total_tokens INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE direct_messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id TEXT NOT NULL,
            message_index INTEGER NOT NULL,
            role TEXT NOT NULL,
            content_json TEXT NOT NULL,
            tool_calls_json TEXT,
            input_tokens INTEGER,
            output_tokens INTEGER,
            created_at TIMESTAMP NOT NULL,
            UNIQUE(conversation_id, message_index)
        );
        "#,
    )
    .unwrap();
}

#[tokio::test]
#[ignore = "requires OPENROUTER_API_KEY and network"]
async fn openrouter_key_endpoint_accepts_saved_bearer_key() {
    let key = std::env::var("OPENROUTER_API_KEY")
        .expect("OPENROUTER_API_KEY must be set for this ignored test");
    let status = reqwest::Client::new()
        .get("https://openrouter.ai/api/v1/key")
        .header("Authorization", format!("Bearer {key}"))
        .send()
        .await
        .expect("request should complete")
        .status();

    assert_eq!(status.as_u16(), 200);
}

#[tokio::test]
#[ignore] // Requires OPENAI_API_KEY env var
async fn e2e_openai_conversation_with_persistence() {
    // Setup
    let openai_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY required for E2E test");

    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let log_dir = temp_dir.path().join("logs");

    // Initialize database schema
    init_test_db(&db_path);

    // Initialize components
    let logger = DirectApiLogger::new(log_dir.clone());
    let repo = Arc::new(ConversationRepository::new(db_path.clone()));
    let provider: SharedProvider =
        Arc::new(GenaiAdapter::new("openai", &openai_key, "gpt-4o-mini"));

    // Create conversation
    let conv_id_str = repo
        .create_conversation("openai".to_string(), "gpt-4o-mini".to_string())
        .await
        .unwrap();
    let conv_id = AIConversationId::from_string(&conv_id_str).unwrap();

    // Prepare messages
    let initial_messages = vec![ChatMessage::User(vec![ContentBlock::Text(
        "What is 2+2? Answer with just the number.".into(),
    )])];

    // Setup channels
    let (event_tx, mut event_rx) = agent_event_channel(100);
    let (tool_tx, _tool_rx) = mpsc::channel::<ToolDispatchRequest>(100);
    let (_cancel_tx, cancel_rx) = oneshot::channel();

    // Log start
    logger
        .log(&format!("E2E Test: Starting conversation {}", conv_id_str))
        .await;

    // Run direct loop
    tokio::spawn(direct_loop::run(
        provider,
        initial_messages,
        vec![],
        conv_id,
        event_tx,
        tool_tx,
        cancel_rx,
        Some(repo.clone()),
    ));

    // Collect events
    let mut response_text = String::new();
    while let Some(event) = event_rx.recv().await {
        match event {
            AgentEvent::TextChunk(text) => {
                response_text.push_str(&text);
                logger.log(&format!("Received chunk: {}", text)).await;
            }
            AgentEvent::Done {
                finish_reason,
                usage,
            } => {
                logger
                    .log(&format!(
                        "Conversation done: {:?}, usage: {:?}",
                        finish_reason, usage
                    ))
                    .await;
                break;
            }
            AgentEvent::Error(err) => {
                panic!("Unexpected error: {}", err);
            }
            _ => {}
        }
    }

    // Verify response
    assert!(
        response_text.contains('4'),
        "Expected answer to contain '4', got: {}",
        response_text
    );
    logger
        .log(&format!("Response verified: {}", response_text))
        .await;

    // Verify persistence
    let saved_messages = repo.load_messages(conv_id_str.clone()).await.unwrap();
    assert_eq!(
        saved_messages.len(),
        2,
        "Expected user + assistant messages"
    );

    // Verify auto-title
    let conversation = repo.get_conversation(conv_id_str.clone()).await.unwrap();
    assert!(
        conversation.title.is_some(),
        "Auto-title should be generated"
    );
    logger
        .log(&format!("Auto-title: {:?}", conversation.title))
        .await;

    // Verify logging (check log file exists and has entries)
    let log_content = std::fs::read_to_string(log_dir.join("direct-api.log")).unwrap();
    assert!(log_content.contains("E2E Test"));
    assert!(log_content.contains("Conversation done"));

    // Verify API key redaction
    assert!(
        !log_content.contains(&openai_key),
        "API key should be redacted in logs"
    );

    logger.log("E2E Test: PASSED").await;
}

#[tokio::test]
async fn e2e_ollama_local_llm() {
    // Check if Ollama is running
    if tokio::net::TcpStream::connect("127.0.0.1:11434")
        .await
        .is_err()
    {
        eprintln!("Skipping Ollama E2E test: Ollama not running on localhost:11434");
        return;
    }

    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let log_dir = temp_dir.path().join("logs");

    // Initialize database schema
    init_test_db(&db_path);

    let logger = DirectApiLogger::new(log_dir.clone());
    let repo = Arc::new(ConversationRepository::new(db_path));
    let provider: SharedProvider = Arc::new(GenaiAdapter::new("ollama", "", "llama3.2"));

    let conv_id_str = repo
        .create_conversation("ollama".to_string(), "llama3.2".to_string())
        .await
        .unwrap();
    let conv_id = AIConversationId::from_string(&conv_id_str).unwrap();

    let initial_messages = vec![ChatMessage::User(vec![ContentBlock::Text(
        "Say 'hello' in one word.".into(),
    )])];

    let (event_tx, mut event_rx) = agent_event_channel(100);
    let (tool_tx, _tool_rx) = mpsc::channel::<ToolDispatchRequest>(100);
    let (_cancel_tx, cancel_rx) = oneshot::channel();

    logger.log("E2E Ollama Test: Starting").await;

    tokio::spawn(direct_loop::run(
        provider,
        initial_messages,
        vec![],
        conv_id,
        event_tx,
        tool_tx,
        cancel_rx,
        Some(repo.clone()),
    ));

    let mut done = false;
    while let Some(event) = event_rx.recv().await {
        if matches!(event, AgentEvent::Done { .. }) {
            done = true;
            logger.log("E2E Ollama Test: Received Done event").await;
            break;
        }
    }

    assert!(done, "Conversation should complete");

    // Verify persistence
    let saved_messages = repo.load_messages(conv_id_str.clone()).await.unwrap();
    assert_eq!(
        saved_messages.len(),
        2,
        "Expected user + assistant messages"
    );

    logger.log("E2E Ollama Test: PASSED").await;
}

#[tokio::test]
#[ignore] // Requires OPENAI_API_KEY
async fn e2e_resume_conversation() {
    let openai_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY required");

    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let log_dir = temp_dir.path().join("logs");

    // Initialize database schema
    init_test_db(&db_path);

    let logger = DirectApiLogger::new(log_dir);
    let repo = Arc::new(ConversationRepository::new(db_path));
    let provider: SharedProvider =
        Arc::new(GenaiAdapter::new("openai", &openai_key, "gpt-4o-mini"));

    // First conversation turn
    let conv_id_str = repo
        .create_conversation("openai".to_string(), "gpt-4o-mini".to_string())
        .await
        .unwrap();
    let conv_id = AIConversationId::from_string(&conv_id_str).unwrap();

    logger
        .log("E2E Resume Test: First turn - asking 'What is 2+2?'")
        .await;

    let (event_tx, mut event_rx) = agent_event_channel(100);
    let (tool_tx, _tool_rx) = mpsc::channel::<ToolDispatchRequest>(100);
    let (_cancel_tx, cancel_rx) = oneshot::channel();

    tokio::spawn(direct_loop::run(
        provider.clone(),
        vec![ChatMessage::User(vec![ContentBlock::Text(
            "What is 2+2?".into(),
        )])],
        vec![],
        conv_id,
        event_tx,
        tool_tx,
        cancel_rx,
        Some(repo.clone()),
    ));

    while let Some(event) = event_rx.recv().await {
        if matches!(event, AgentEvent::Done { .. }) {
            logger.log("E2E Resume Test: First turn completed").await;
            break;
        }
    }

    // Verify first turn
    let history = repo.load_messages(conv_id_str.clone()).await.unwrap();
    assert_eq!(history.len(), 2, "Expected user + assistant messages"); // User + Assistant

    logger
        .log("E2E Resume Test: Second turn - asking 'What is 3+3?'")
        .await;

    // Resume conversation
    let mut resumed_messages = history;
    resumed_messages.push(ChatMessage::User(vec![ContentBlock::Text(
        "What is 3+3?".into(),
    )]));

    let (event_tx2, mut event_rx2) = agent_event_channel(100);
    let (tool_tx2, _tool_rx2) = mpsc::channel::<ToolDispatchRequest>(100);
    let (_cancel_tx2, cancel_rx2) = oneshot::channel();

    tokio::spawn(direct_loop::run(
        provider,
        resumed_messages,
        vec![],
        conv_id,
        event_tx2,
        tool_tx2,
        cancel_rx2,
        Some(repo.clone()),
    ));

    while let Some(event) = event_rx2.recv().await {
        if matches!(event, AgentEvent::Done { .. }) {
            logger.log("E2E Resume Test: Second turn completed").await;
            break;
        }
    }

    // Verify full history saved
    let final_history = repo.load_messages(conv_id_str.clone()).await.unwrap();
    assert_eq!(
        final_history.len(),
        4,
        "Expected 2 user + 2 assistant messages"
    );

    logger.log("E2E Resume Test: PASSED").await;
}

#[tokio::test]
#[ignore] // Requires ANTHROPIC_API_KEY env var
async fn e2e_anthropic_conversation() {
    // Setup
    let anthropic_key =
        std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY required for E2E test");

    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let log_dir = temp_dir.path().join("logs");

    // Initialize database schema
    init_test_db(&db_path);

    // Initialize components
    let logger = DirectApiLogger::new(log_dir.clone());
    let repo = Arc::new(ConversationRepository::new(db_path.clone()));
    let provider: SharedProvider = Arc::new(GenaiAdapter::new(
        "anthropic",
        &anthropic_key,
        "claude-3-5-haiku-20241022",
    ));

    // Create conversation
    let conv_id_str = repo
        .create_conversation(
            "anthropic".to_string(),
            "claude-3-5-haiku-20241022".to_string(),
        )
        .await
        .unwrap();
    let conv_id = AIConversationId::from_string(&conv_id_str).unwrap();

    // Prepare messages
    let initial_messages = vec![ChatMessage::User(vec![ContentBlock::Text(
        "What is the capital of France? Answer with just the city name.".into(),
    )])];

    // Setup channels
    let (event_tx, mut event_rx) = agent_event_channel(100);
    let (tool_tx, _tool_rx) = mpsc::channel::<ToolDispatchRequest>(100);
    let (_cancel_tx, cancel_rx) = oneshot::channel();

    // Log start
    logger
        .log(&format!(
            "E2E Anthropic Test: Starting conversation {}",
            conv_id_str
        ))
        .await;

    // Run direct loop
    tokio::spawn(direct_loop::run(
        provider,
        initial_messages,
        vec![],
        conv_id,
        event_tx,
        tool_tx,
        cancel_rx,
        Some(repo.clone()),
    ));

    // Collect events
    let mut response_text = String::new();
    while let Some(event) = event_rx.recv().await {
        match event {
            AgentEvent::TextChunk(text) => {
                response_text.push_str(&text);
            }
            AgentEvent::Done { .. } => {
                break;
            }
            AgentEvent::Error(err) => {
                panic!("Unexpected error: {}", err);
            }
            _ => {}
        }
    }

    // Verify response contains "Paris"
    assert!(
        response_text.to_lowercase().contains("paris"),
        "Expected answer to contain 'Paris', got: {}",
        response_text
    );

    // Verify persistence
    let saved_messages = repo.load_messages(conv_id_str.clone()).await.unwrap();
    assert_eq!(
        saved_messages.len(),
        2,
        "Expected user + assistant messages"
    );

    // Verify API key redaction
    let log_content = std::fs::read_to_string(log_dir.join("direct-api.log")).unwrap();
    assert!(
        !log_content.contains(&anthropic_key),
        "Anthropic API key should be redacted in logs"
    );

    logger.log("E2E Anthropic Test: PASSED").await;
}

#[tokio::test]
async fn selected_model_propagates_to_conversation_repository() {
    use ai::api_keys::ApiKeyManager;
    use ai::model_registry::ProviderId;
    use warpui::App;
    use warpui_extras::secure_storage;

    // Demonstrates expected integration: ApiKeyManager → ConversationRepository.
    // Production integration blocked on conversation-starting UI (Phase 3).
    // Pattern: get_selected_model_for_provider → create_conversation → direct_loop::run
    App::test((), |mut app| async move {
        // Setup secure storage
        app.update(|ctx| {
            secure_storage::register_noop("test", ctx);
        });

        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        init_test_db(&db_path);

        let repo = Arc::new(ConversationRepository::new(db_path));
        let manager = app.add_singleton_model(ApiKeyManager::new);

        // Simulate user selecting a specific model for OpenAI
        manager.update(&mut app, |manager, ctx| {
            manager.set_selected_model(ProviderId::OpenAI, "gpt-4-turbo".to_string(), ctx);
        });

        // When creating a conversation, read the selected model
        let (provider_str, model_id) = manager.read(&app, |manager, ctx| {
            let provider = ProviderId::OpenAI;
            let model = manager
                .get_selected_model_for_provider(provider, ctx)
                .expect("OpenAI should have a selected or default model");
            ("openai".to_string(), model)
        });

        // Verify the selected model is used (not the default)
        assert_eq!(model_id, "gpt-4-turbo");

        // Create conversation with the selected model
        let conv_id_str = repo
            .create_conversation(provider_str, model_id.clone())
            .await
            .unwrap();

        // Verify conversation metadata has the correct model
        let conversation = repo.get_conversation(conv_id_str).await.unwrap();
        assert_eq!(conversation.model_id, "gpt-4-turbo");
    });
}

#[tokio::test]
async fn selecting_model_persists_to_api_key_manager() {
    use ai::api_keys::ApiKeyManager;
    use ai::model_registry::ProviderId;
    use warpui::App;
    use warpui_extras::secure_storage;

    App::test((), |mut app| async move {
        // Setup secure storage
        app.update(|ctx| {
            secure_storage::register_noop("test", ctx);
        });

        let manager = app.add_singleton_model(ApiKeyManager::new);

        // Set a model selection
        manager.update(&mut app, |manager, ctx| {
            manager.set_selected_model(
                ProviderId::Anthropic,
                "claude-3-opus-20240229".to_string(),
                ctx,
            );
        });

        // Verify it persists and can be read back
        manager.read(&app, |manager, ctx| {
            let selected = manager
                .get_selected_model_for_provider(ProviderId::Anthropic, ctx)
                .expect("Should return the selected model");
            assert_eq!(selected, "claude-3-opus-20240229");
        });

        // Verify default providers still return their defaults when not set
        manager.read(&app, |manager, ctx| {
            let openai_default = manager
                .get_selected_model_for_provider(ProviderId::OpenAI, ctx)
                .expect("OpenAI should have a default");
            assert_eq!(openai_default, "gpt-4o-mini");
        });
    });
}
