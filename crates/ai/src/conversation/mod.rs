use crate::provider::{ChatMessage, ContentBlock};

/// Convert ChatMessage to database format (role, content_json, tool_calls_json)
pub fn serialize_chat_message(msg: &ChatMessage) -> (String, String, Option<String>) {
    match msg {
        ChatMessage::System(text) => (
            "system".into(),
            serde_json::to_string(&vec![ContentBlock::Text(text.clone())]).unwrap(),
            None,
        ),
        ChatMessage::User(blocks) => (
            "user".into(),
            serde_json::to_string(blocks).unwrap(),
            None,
        ),
        ChatMessage::Assistant { text, tool_calls } => {
            let content_blocks = if let Some(text) = text {
                vec![ContentBlock::Text(text.clone())]
            } else {
                vec![]
            };
            let tool_calls_json = if !tool_calls.is_empty() {
                Some(serde_json::to_string(tool_calls).unwrap())
            } else {
                None
            };
            (
                "assistant".into(),
                serde_json::to_string(&content_blocks).unwrap(),
                tool_calls_json,
            )
        }
    }
}

/// Convert database format back to ChatMessage
pub fn deserialize_chat_message(
    role: &str,
    content_json: &str,
    tool_calls_json: Option<&str>,
) -> ChatMessage {
    match role {
        "system" => {
            let blocks: Vec<ContentBlock> = serde_json::from_str(content_json).unwrap();
            if let Some(ContentBlock::Text(text)) = blocks.first() {
                ChatMessage::System(text.clone())
            } else {
                ChatMessage::System(String::new())
            }
        }
        "user" => {
            let blocks: Vec<ContentBlock> = serde_json::from_str(content_json).unwrap();
            ChatMessage::User(blocks)
        }
        "assistant" => {
            let blocks: Vec<ContentBlock> = serde_json::from_str(content_json).unwrap();
            let text = blocks.first().and_then(|b| match b {
                ContentBlock::Text(t) => Some(t.clone()),
                _ => None,
            });
            let tool_calls = tool_calls_json
                .map(|json| serde_json::from_str(json).unwrap())
                .unwrap_or_default();
            ChatMessage::Assistant { text, tool_calls }
        }
        _ => panic!("Unknown role: {}", role),
    }
}

#[cfg(test)]
#[path = "serialization_tests.rs"]
mod tests;
