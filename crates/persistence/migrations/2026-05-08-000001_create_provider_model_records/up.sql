CREATE TABLE provider_model_records (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    provider_kind TEXT NOT NULL,
    model_id TEXT NOT NULL,
    context_window INTEGER NOT NULL,
    supports_tools BOOLEAN NOT NULL DEFAULT 1,
    supports_vision BOOLEAN NOT NULL DEFAULT 0,
    supports_streaming BOOLEAN NOT NULL DEFAULT 1,
    last_fetched_at TIMESTAMP NOT NULL,
    UNIQUE (provider_kind, model_id)
);
