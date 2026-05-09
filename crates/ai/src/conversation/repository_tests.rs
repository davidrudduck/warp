use super::repository::ConversationRepository;
use tempfile::tempdir;

#[tokio::test]
async fn repository_creates_conversation() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Initialize test database with schema
    {
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
            "#,
        )
        .unwrap();
    }

    let repo = ConversationRepository::new(db_path);

    let conv_id: String = repo.create_conversation("openai", "gpt-4o").await.unwrap();

    // Verify in DB
    let conv = repo.get_conversation(&conv_id).await.unwrap();
    assert_eq!(conv.provider_kind, "openai");
    assert_eq!(conv.model_id, "gpt-4o");
    assert_eq!(conv.message_count, 0);
}

#[tokio::test]
async fn repository_saves_messages() {
    use crate::provider::{ChatMessage, ContentBlock};

    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Initialize test database with schema
    {
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

    let repo = ConversationRepository::new(db_path);
    let conv_id: String = repo.create_conversation("openai", "gpt-4o").await.unwrap();

    let messages = vec![
        ChatMessage::User(vec![ContentBlock::Text("Hello".into())]),
        ChatMessage::Assistant {
            text: Some("Hi there!".into()),
            tool_calls: vec![],
        },
    ];

    repo.save_messages(&conv_id, &messages).await.unwrap();

    // Verify
    let loaded = repo.load_messages(&conv_id).await.unwrap();
    assert_eq!(loaded.len(), 2);

    // Verify conversation updated
    let conv = repo.get_conversation(&conv_id).await.unwrap();
    assert_eq!(conv.message_count, 2);
}

#[tokio::test]
async fn repository_generates_auto_title() {
    use crate::provider::{ChatMessage, ContentBlock};

    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Initialize test database with schema
    {
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

    let repo = ConversationRepository::new(db_path);
    let conv_id: String = repo.create_conversation("openai", "gpt-4o").await.unwrap();

    // Save messages with a user message
    let messages = vec![
        ChatMessage::User(vec![ContentBlock::Text(
            "How do I create a Rust project?".into(),
        )]),
        ChatMessage::Assistant {
            text: Some("Use cargo new <name>".into()),
            tool_calls: vec![],
        },
    ];

    repo.save_messages(&conv_id, &messages).await.unwrap();

    // Generate auto-title based on first user message
    repo.generate_title(&conv_id).await.unwrap();

    // Verify title was set
    let conv = repo.get_conversation(&conv_id).await.unwrap();
    assert!(conv.title.is_some());
    let title = conv.title.unwrap();
    assert!(!title.is_empty());
    assert!(title.len() <= 50); // Title should be concise
}

#[tokio::test]
async fn repository_auto_title_truncates_long_messages() {
    use crate::provider::{ChatMessage, ContentBlock};

    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Initialize test database with schema
    {
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

    let repo = ConversationRepository::new(db_path);
    let conv_id: String = repo.create_conversation("openai", "gpt-4o").await.unwrap();

    // Save messages with a very long user message
    let long_text = "This is a very long message that should be truncated to fit within the title length limit. It contains many words and characters that exceed what would be reasonable for a conversation title.";
    let messages = vec![ChatMessage::User(vec![ContentBlock::Text(
        long_text.into(),
    )])];

    repo.save_messages(&conv_id, &messages).await.unwrap();
    repo.generate_title(&conv_id).await.unwrap();

    // Verify title was truncated
    let conv = repo.get_conversation(&conv_id).await.unwrap();
    let title = conv.title.unwrap();
    assert!(title.len() <= 50);
    assert!(title.ends_with("..."));
}
