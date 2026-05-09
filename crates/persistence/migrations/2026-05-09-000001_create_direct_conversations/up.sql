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
    FOREIGN KEY (conversation_id) REFERENCES direct_conversations(conversation_id) ON DELETE CASCADE
);

CREATE INDEX idx_direct_messages_conversation ON direct_messages(conversation_id, message_index);
CREATE INDEX idx_direct_conversations_recent ON direct_conversations(last_message_at DESC);
