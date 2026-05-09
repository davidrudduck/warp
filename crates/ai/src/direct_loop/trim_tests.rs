use super::*;
use crate::provider::{ContentBlock, FinishReason, ToolCall};

fn sys(s: &str) -> ChatMessage {
    ChatMessage::System(s.into())
}

fn user(s: &str) -> ChatMessage {
    ChatMessage::User(vec![ContentBlock::Text(s.into())])
}

fn assistant(s: &str) -> ChatMessage {
    ChatMessage::Assistant {
        text: Some(s.into()),
        tool_calls: vec![],
    }
}

// --- trim_to_context_window tests ---

#[test]
fn trim_no_op_when_under_limit() {
    let msgs = vec![sys("system"), user("a"), assistant("b")];
    let result = trim_to_context_window(msgs.clone(), 10);
    assert_eq!(result.len(), 3);
}

#[test]
fn trim_no_op_when_at_limit() {
    let msgs = vec![user("a"), assistant("b"), user("c")];
    let result = trim_to_context_window(msgs, 3);
    assert_eq!(result.len(), 3);
}

#[test]
fn trim_drops_oldest_non_system_messages() {
    let msgs = vec![
        sys("sys"),
        user("old1"),
        assistant("old2"),
        user("recent1"),
        assistant("recent2"),
    ];
    // limit=3: keep 1 system + 2 most-recent non-system
    let result = trim_to_context_window(msgs, 3);
    assert_eq!(result.len(), 3);
    assert!(matches!(&result[0], ChatMessage::System(_)));
    // The two most-recent non-system messages should be kept.
    match &result[1] {
        ChatMessage::User(blocks) => {
            if let ContentBlock::Text(t) = &blocks[0] {
                assert_eq!(t, "recent1");
            } else {
                panic!("expected Text block");
            }
        }
        other => panic!("expected User, got {other:?}"),
    }
    match &result[2] {
        ChatMessage::Assistant { text, .. } => assert_eq!(text.as_deref(), Some("recent2")),
        other => panic!("expected Assistant, got {other:?}"),
    }
}

#[test]
fn trim_preserves_all_system_messages() {
    let msgs = vec![sys("sys1"), sys("sys2"), user("u1"), user("u2"), user("u3")];
    // limit=4: keep 2 system + 2 most-recent non-system
    let result = trim_to_context_window(msgs, 4);
    assert_eq!(result.len(), 4);
    let system_count = result
        .iter()
        .filter(|m| matches!(m, ChatMessage::System(_)))
        .count();
    assert_eq!(system_count, 2);
}

#[test]
fn trim_empty_messages_returns_empty() {
    let result = trim_to_context_window(vec![], 10);
    assert!(result.is_empty());
}

#[test]
fn trim_limit_zero_keeps_only_system_when_system_fills() {
    // system_count(0) >= limit(0) → only system messages (none here)
    let msgs = vec![user("a"), user("b")];
    let result = trim_to_context_window(msgs, 0);
    assert!(result.is_empty());
}

// --- tool_requires_confirmation tests (per-name dispatch) ---

#[test]
fn tool_requires_confirmation_read_only_tools_are_false() {
    assert!(!tool_requires_confirmation("ReadFiles"));
    assert!(!tool_requires_confirmation("SearchCodebase"));
    assert!(!tool_requires_confirmation("Grep"));
    assert!(!tool_requires_confirmation("FileGlob"));
    assert!(!tool_requires_confirmation("FileGlobV2"));
    assert!(!tool_requires_confirmation("ReadShellCommandOutput"));
    assert!(!tool_requires_confirmation("ReadDocuments"));
    assert!(!tool_requires_confirmation("ReadSkill"));
    assert!(!tool_requires_confirmation("ReadMCPResource"));
}

#[test]
fn tool_requires_confirmation_side_effecting_tools_are_true() {
    assert!(tool_requires_confirmation("RequestCommandOutput"));
    assert!(tool_requires_confirmation("RequestFileEdits"));
    assert!(tool_requires_confirmation("AskUserQuestion"));
    assert!(tool_requires_confirmation("WriteToLongRunningShellCommand"));
    assert!(tool_requires_confirmation("CreateDocuments"));
    assert!(tool_requires_confirmation("EditDocuments"));
    assert!(tool_requires_confirmation("CallMCPTool"));
}

#[test]
fn tool_requires_confirmation_unknown_tool_fail_safe() {
    assert!(tool_requires_confirmation("SomeUnknownFutureTool"));
    assert!(tool_requires_confirmation(""));
}

#[test]
fn batch_requires_confirmation_true_when_any_needs_confirm() {
    let batch = vec![
        ToolCall {
            id: "1".into(),
            name: "ReadFiles".into(),
            input: serde_json::json!({}),
        },
        ToolCall {
            id: "2".into(),
            name: "RequestCommandOutput".into(),
            input: serde_json::json!({}),
        },
    ];
    assert!(batch_requires_confirmation(&batch));
}

#[test]
fn batch_requires_confirmation_false_when_all_safe() {
    let batch = vec![
        ToolCall {
            id: "1".into(),
            name: "ReadFiles".into(),
            input: serde_json::json!({}),
        },
        ToolCall {
            id: "2".into(),
            name: "Grep".into(),
            input: serde_json::json!({}),
        },
    ];
    assert!(!batch_requires_confirmation(&batch));
}

// --- requires_confirmation tests ---

#[test]
fn requires_confirmation_true_for_tool_use_with_calls() {
    let tc = ToolCall {
        id: "c1".into(),
        name: "Grep".into(),
        input: serde_json::json!({}),
    };
    assert!(requires_confirmation(FinishReason::ToolUse, &[tc]));
}

#[test]
fn requires_confirmation_false_for_stop() {
    let tc = ToolCall {
        id: "c1".into(),
        name: "Grep".into(),
        input: serde_json::json!({}),
    };
    assert!(!requires_confirmation(FinishReason::Stop, &[tc]));
}

#[test]
fn requires_confirmation_false_when_no_tool_calls() {
    assert!(!requires_confirmation(FinishReason::ToolUse, &[]));
}
