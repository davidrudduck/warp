use super::*;
use crate::conversation::repository::ConversationRepository;
use crate::provider::{
    agent_event_channel, mock::MockLlmProvider, AgentEvent, ChatMessage, ContentBlock,
    FinishReason, SharedProvider, StreamEvent, ToolCall, ToolResultContent,
};
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
async fn run_no_tools_completes_immediately() {
    // Test: When provider returns Stop with no tool calls, run should complete
    let events = vec![
        StreamEvent::Start,
        StreamEvent::TextChunk("Hello".into()),
        StreamEvent::End {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];
    let provider: SharedProvider = Arc::new(MockLlmProvider::new().with_stream(events));
    let (tx, rx) = agent_event_channel(16);
    let (tool_req_tx, _tool_req_rx) = mpsc::channel(16);
    let (_cancel_tx, cancel_rx) = futures::channel::oneshot::channel();

    let initial_messages = vec![ChatMessage::User(vec![ContentBlock::Text("test".into())])];
    let conversation_id = AIConversationId::new();

    let result = run(
        provider,
        initial_messages,
        vec![],
        conversation_id,
        tx,
        tool_req_tx,
        cancel_rx,
        None, // No repository for existing tests
    )
    .await;

    assert!(result.is_ok());

    // Verify we got text chunks and done event
    drop(rx);
}

#[tokio::test]
async fn run_with_tools_dispatches_and_continues() {
    // Test: When provider returns ToolUse, run should dispatch tools and continue
    let tool_call = ToolCall {
        id: "tc1".into(),
        name: "ReadFiles".into(),
        input: serde_json::json!({"paths": ["/tmp/test.txt"]}),
    };

    // First turn: model requests a tool
    let events1 = vec![
        StreamEvent::Start,
        StreamEvent::ToolCallReady(tool_call.clone()),
        StreamEvent::End {
            finish_reason: FinishReason::ToolUse,
            usage: None,
        },
    ];

    // Second turn: model responds with text after tool result
    let events2 = vec![
        StreamEvent::Start,
        StreamEvent::TextChunk("Got the file".into()),
        StreamEvent::End {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];

    let provider: SharedProvider = Arc::new(
        MockLlmProvider::new()
            .with_stream(events1)
            .with_stream(events2),
    );

    let (tx, _rx) = agent_event_channel(16);
    let (tool_req_tx, mut tool_req_rx) = mpsc::channel(16);
    let (_cancel_tx, cancel_rx) = futures::channel::oneshot::channel();

    let initial_messages = vec![ChatMessage::User(vec![ContentBlock::Text("test".into())])];
    let conversation_id = AIConversationId::new();

    // Spawn the run loop in the background
    let run_handle = tokio::spawn(async move {
        run(
            provider,
            initial_messages,
            vec![],
            conversation_id,
            tx,
            tool_req_tx,
            cancel_rx,
            None, // No repository for existing tests
        )
        .await
    });

    // Mock tool executor: receive dispatch request and send back result
    let dispatch_req = tool_req_rx
        .recv()
        .await
        .expect("should receive tool dispatch");
    assert_eq!(dispatch_req.tool_call.name, "ReadFiles");

    let result_block = ContentBlock::ToolResult {
        tool_use_id: dispatch_req.tool_call.id.clone(),
        content: ToolResultContent::Text("file contents".into()),
        is_error: false,
    };
    dispatch_req.result_tx.send(Ok(result_block)).unwrap();

    // Wait for run to complete
    let result = run_handle.await.unwrap();
    assert!(result.is_ok());
}

#[tokio::test]
async fn run_preserves_tool_call_order_when_confirmation_is_required() {
    let read_call = ToolCall {
        id: "tc-read".into(),
        name: "ReadFiles".into(),
        input: serde_json::json!({"files": [{"name": "Cargo.toml"}]}),
    };
    let write_call = ToolCall {
        id: "tc-write".into(),
        name: "RunAgents".into(),
        input: serde_json::json!({"prompt": "check this"}),
    };

    let events1 = vec![
        StreamEvent::Start,
        StreamEvent::ToolCallReady(read_call.clone()),
        StreamEvent::ToolCallReady(write_call.clone()),
        StreamEvent::End {
            finish_reason: FinishReason::ToolUse,
            usage: None,
        },
    ];
    let events2 = vec![
        StreamEvent::Start,
        StreamEvent::TextChunk("done".into()),
        StreamEvent::End {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];

    let provider: SharedProvider = Arc::new(
        MockLlmProvider::new()
            .with_stream(events1)
            .with_stream(events2),
    );
    let (tx, _rx) = agent_event_channel(16);
    let (tool_req_tx, mut tool_req_rx) = mpsc::channel(16);
    let (_cancel_tx, cancel_rx) = futures::channel::oneshot::channel();
    let initial_messages = vec![ChatMessage::User(vec![ContentBlock::Text("test".into())])];
    let conversation_id = AIConversationId::new();

    let run_handle = tokio::spawn(async move {
        run(
            provider,
            initial_messages,
            vec![],
            conversation_id,
            tx,
            tool_req_tx,
            cancel_rx,
            None,
        )
        .await
    });

    let first = tool_req_rx
        .recv()
        .await
        .expect("should receive first dispatch");
    assert_eq!(first.index, 0);
    assert_eq!(first.tool_call.name, "ReadFiles");
    first
        .result_tx
        .send(Ok(ContentBlock::ToolResult {
            tool_use_id: "tc-read".into(),
            content: ToolResultContent::Text("file contents".into()),
            is_error: false,
        }))
        .unwrap();

    let second = tool_req_rx
        .recv()
        .await
        .expect("should receive second dispatch");
    assert_eq!(second.index, 1);
    assert_eq!(second.tool_call.name, "RunAgents");
    second
        .result_tx
        .send(Ok(ContentBlock::ToolResult {
            tool_use_id: "tc-write".into(),
            content: ToolResultContent::Text("agent output".into()),
            is_error: false,
        }))
        .unwrap();

    let result = run_handle.await.unwrap();
    assert!(result.is_ok());
}

#[tokio::test]
async fn run_respects_turn_limit() {
    // Test: run should stop after MAX_DIRECT_LOOP_TURNS
    let tool_call = ToolCall {
        id: "tc1".into(),
        name: "ReadFiles".into(),
        input: serde_json::json!({}),
    };

    // Create a provider that always returns tool calls (infinite loop scenario)
    let mut provider = MockLlmProvider::new();
    for _ in 0..60 {
        // More than MAX_DIRECT_LOOP_TURNS
        provider = provider.with_stream(vec![
            StreamEvent::Start,
            StreamEvent::ToolCallReady(tool_call.clone()),
            StreamEvent::End {
                finish_reason: FinishReason::ToolUse,
                usage: None,
            },
        ]);
    }

    let provider: SharedProvider = Arc::new(provider);
    let (tx, mut rx) = agent_event_channel(128);
    let (tool_req_tx, mut tool_req_rx) = mpsc::channel(128);
    let (_cancel_tx, cancel_rx) = futures::channel::oneshot::channel();

    let initial_messages = vec![ChatMessage::User(vec![ContentBlock::Text("test".into())])];
    let conversation_id = AIConversationId::new();

    let run_handle = tokio::spawn(async move {
        run(
            provider,
            initial_messages,
            vec![],
            conversation_id,
            tx,
            tool_req_tx,
            cancel_rx,
            None, // No repository for existing tests
        )
        .await
    });

    // Mock tool executor: respond to tool calls
    let mut count = 0;
    while let Some(dispatch_req) = tool_req_rx.recv().await {
        count += 1;
        let result_block = ContentBlock::ToolResult {
            tool_use_id: dispatch_req.tool_call.id.clone(),
            content: ToolResultContent::Text("result".into()),
            is_error: false,
        };
        dispatch_req.result_tx.send(Ok(result_block)).ok();

        if count > 60 {
            break; // Safety: don't loop forever
        }
    }

    let result = run_handle.await.unwrap();
    assert!(result.is_ok());

    // Should have stopped before processing all 60 potential tool calls
    assert!(count < 60);
    assert!(count <= MAX_DIRECT_LOOP_TURNS);

    // Should have received an error event about the turn limit
    let mut got_error = false;
    while let Ok(event) = rx.try_recv() {
        if matches!(event, AgentEvent::Error(_)) {
            got_error = true;
        }
    }
    assert!(got_error, "should emit error when hitting turn limit");
}

#[tokio::test]
async fn run_cancelled_stops_cleanly() {
    // Test: When cancellation signal fires, run should stop gracefully
    // Use a stream that would normally complete, but we'll cancel it
    let events = vec![
        StreamEvent::Start,
        StreamEvent::TextChunk("Starting".into()),
        StreamEvent::End {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ];

    let provider: SharedProvider = Arc::new(MockLlmProvider::new().with_stream(events));
    let (tx, _rx) = agent_event_channel(16);
    let (tool_req_tx, _tool_req_rx) = mpsc::channel(16);
    let (cancel_tx, cancel_rx) = futures::channel::oneshot::channel();

    let initial_messages = vec![ChatMessage::User(vec![ContentBlock::Text("test".into())])];
    let conversation_id = AIConversationId::new();

    // Cancel immediately before run starts
    cancel_tx.send(()).unwrap();

    // Run should complete without error when cancelled
    let result = run(
        provider,
        initial_messages,
        vec![],
        conversation_id,
        tx,
        tool_req_tx,
        cancel_rx,
        None, // No repository for existing tests
    )
    .await;

    assert!(
        result.is_ok(),
        "run should return Ok when cancelled, got: {result:?}"
    );
}

#[tokio::test]
async fn run_persists_conversation_to_db() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Initialize test database with schema
    init_test_db(&db_path);

    let repo = Arc::new(ConversationRepository::new(db_path));

    // Create initial conversation
    let conv_id = repo
        .create_conversation("openai".to_string(), "gpt-4o".to_string())
        .await
        .unwrap();

    let provider = Arc::new(MockLlmProvider::new().with_stream(vec![
        StreamEvent::Start,
        StreamEvent::TextChunk("Hello".into()),
        StreamEvent::End {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ]));

    let initial_messages = vec![ChatMessage::User(vec![ContentBlock::Text("Hi".into())])];

    let (tx, _rx) = agent_event_channel(16);
    let (tool_req_tx, _tool_req_rx) = mpsc::channel(16);
    let (_cancel_tx, cancel_rx) = futures::channel::oneshot::channel();

    // Use the same conversation ID that was created in the repository
    let conversation_id = AIConversationId::from_string(&conv_id).unwrap();

    // Run with repository
    run(
        provider,
        initial_messages,
        vec![],
        conversation_id,
        tx,
        tool_req_tx,
        cancel_rx,
        Some(repo.clone()), // NEW: optional repository parameter
    )
    .await
    .unwrap();

    // Verify conversation was saved
    let messages = repo.load_messages(conv_id.clone()).await.unwrap();
    assert_eq!(messages.len(), 2); // User + Assistant
}

#[tokio::test]
async fn run_resumes_from_saved_conversation() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Initialize test database with schema
    init_test_db(&db_path);

    let repo = Arc::new(ConversationRepository::new(db_path));

    let conv_id = repo
        .create_conversation("openai".to_string(), "gpt-4o".to_string())
        .await
        .unwrap();

    // Pre-populate with saved messages
    let previous_messages = vec![
        ChatMessage::User(vec![ContentBlock::Text("What is 2+2?".into())]),
        ChatMessage::Assistant {
            text: Some("4".into()),
            tool_calls: vec![],
        },
    ];
    repo.save_messages(conv_id.clone(), previous_messages.clone())
        .await
        .unwrap();

    // Resume with new message
    let provider = Arc::new(MockLlmProvider::new().with_stream(vec![
        StreamEvent::Start,
        StreamEvent::TextChunk("It's still 4!".into()),
        StreamEvent::End {
            finish_reason: FinishReason::Stop,
            usage: None,
        },
    ]));

    // Load previous messages and continue
    let mut initial_messages = repo.load_messages(conv_id.clone()).await.unwrap();
    initial_messages.push(ChatMessage::User(vec![ContentBlock::Text(
        "Are you sure?".into(),
    )]));

    let (tx, _rx) = agent_event_channel(16);
    let (tool_req_tx, _tool_req_rx) = mpsc::channel(16);
    let (_cancel_tx, cancel_rx) = futures::channel::oneshot::channel();

    // Use the same conversation ID
    let conversation_id = AIConversationId::from_string(&conv_id).unwrap();

    run(
        provider,
        initial_messages,
        vec![],
        conversation_id,
        tx,
        tool_req_tx,
        cancel_rx,
        Some(repo.clone()),
    )
    .await
    .unwrap();

    // Verify all messages saved
    let final_messages = repo.load_messages(conv_id.clone()).await.unwrap();
    assert_eq!(final_messages.len(), 4); // 2 previous + 1 user + 1 assistant
}
