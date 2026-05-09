use anyhow::Result;
use chrono::Utc;
use std::path::PathBuf;
use tokio::task::spawn_blocking;

use crate::provider::{ChatMessage, ContentBlock};
use persistence::model::{DirectConversation, NewDirectConversation, NewDirectMessage};
use persistence::schema::{direct_conversations, direct_messages};

pub struct ConversationRepository {
    db_path: PathBuf,
}

impl ConversationRepository {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    pub async fn create_conversation(
        &self,
        provider: &str,
        model: &str,
    ) -> Result<String> {
        let db_path = self.db_path.clone();
        let provider = provider.to_string();
        let model = model.to_string();

        spawn_blocking(move || {
            use diesel::prelude::*;

            let mut conn = diesel::SqliteConnection::establish(db_path.to_str().unwrap())?;

            let conv_id = uuid::Uuid::new_v4().to_string();
            let now = Utc::now().naive_utc();

            let new_conv = NewDirectConversation {
                conversation_id: conv_id.clone(),
                provider_kind: provider,
                model_id: model,
                created_at: now,
                last_message_at: now,
                title: None,
            };

            diesel::insert_into(direct_conversations::table)
                .values(&new_conv)
                .execute(&mut conn)?;

            Ok(conv_id)
        })
        .await?
    }

    pub async fn get_conversation(&self, conv_id: &str) -> Result<DirectConversation> {
        let db_path = self.db_path.clone();
        let conv_id = conv_id.to_string();

        spawn_blocking(move || {
            use diesel::prelude::*;

            let mut conn = diesel::SqliteConnection::establish(db_path.to_str().unwrap())?;

            let conv = direct_conversations::table
                .filter(direct_conversations::conversation_id.eq(&conv_id))
                .first(&mut conn)?;

            Ok(conv)
        })
        .await?
    }

    pub async fn save_messages(
        &self,
        conv_id: &str,
        messages: &[ChatMessage],
    ) -> Result<()> {
        let db_path = self.db_path.clone();
        let conv_id = conv_id.to_string();
        let messages = messages.to_vec();

        spawn_blocking(move || {
            use diesel::prelude::*;

            let mut conn = diesel::SqliteConnection::establish(db_path.to_str().unwrap())?;

            conn.transaction::<_, anyhow::Error, _>(|conn| {
                // Clear existing messages
                diesel::delete(direct_messages::table)
                    .filter(direct_messages::conversation_id.eq(&conv_id))
                    .execute(conn)?;

                // Insert new messages
                for (index, message) in messages.iter().enumerate() {
                    let (role, content_json, tool_calls_json) =
                        crate::conversation::serialize_chat_message(message);

                    let new_msg = NewDirectMessage {
                        conversation_id: conv_id.clone(),
                        message_index: index as i32,
                        role,
                        content_json,
                        tool_calls_json,
                        input_tokens: None,
                        output_tokens: None,
                        created_at: Utc::now().naive_utc(),
                    };

                    diesel::insert_into(direct_messages::table)
                        .values(&new_msg)
                        .execute(conn)?;
                }

                // Update conversation metadata
                diesel::update(direct_conversations::table)
                    .filter(direct_conversations::conversation_id.eq(&conv_id))
                    .set((
                        direct_conversations::message_count.eq(messages.len() as i32),
                        direct_conversations::last_message_at.eq(Utc::now().naive_utc()),
                    ))
                    .execute(conn)?;

                Ok(())
            })
        })
        .await?
    }

    pub async fn load_messages(&self, conv_id: &str) -> Result<Vec<ChatMessage>> {
        let db_path = self.db_path.clone();
        let conv_id = conv_id.to_string();

        spawn_blocking(move || {
            use diesel::prelude::*;

            let mut conn = diesel::SqliteConnection::establish(db_path.to_str().unwrap())?;

            let messages: Vec<persistence::model::DirectMessage> = direct_messages::table
                .filter(direct_messages::conversation_id.eq(&conv_id))
                .order(direct_messages::message_index.asc())
                .load(&mut conn)?;

            let chat_messages = messages
                .iter()
                .map(|msg| {
                    crate::conversation::deserialize_chat_message(
                        &msg.role,
                        &msg.content_json,
                        msg.tool_calls_json.as_deref(),
                    )
                })
                .collect();

            Ok(chat_messages)
        })
        .await?
    }

    pub async fn generate_title(&self, conv_id: &str) -> Result<()> {
        let db_path = self.db_path.clone();
        let conv_id = conv_id.to_string();

        spawn_blocking(move || {
            use diesel::prelude::*;

            let mut conn = diesel::SqliteConnection::establish(db_path.to_str().unwrap())?;

            // Get first user message
            let first_message: persistence::model::DirectMessage = direct_messages::table
                .filter(direct_messages::conversation_id.eq(&conv_id))
                .filter(direct_messages::role.eq("user"))
                .order(direct_messages::message_index.asc())
                .first(&mut conn)?;

            // Extract text from content
            let content_blocks: Vec<ContentBlock> =
                serde_json::from_str(&first_message.content_json)?;

            let text = content_blocks
                .iter()
                .find_map(|block| match block {
                    ContentBlock::Text(t) => Some(t.clone()),
                    _ => None,
                })
                .unwrap_or_default();

            // Generate title: take first 50 chars, truncate at word boundary
            let title = if text.len() <= 50 {
                text
            } else {
                let truncated = &text[..47];
                // Find last space to avoid cutting words
                if let Some(last_space) = truncated.rfind(' ') {
                    format!("{}...", &text[..last_space])
                } else {
                    format!("{}...", truncated)
                }
            };

            // Update conversation with title
            diesel::update(direct_conversations::table)
                .filter(direct_conversations::conversation_id.eq(&conv_id))
                .set(direct_conversations::title.eq(title))
                .execute(&mut conn)?;

            Ok(())
        })
        .await?
    }
}
