# iTerm2-Style tmux Clipboard Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an opt-in experimental tmux clipboard sync feature that mirrors tmux paste-buffer updates into the operating-system clipboard.

**Architecture:** Keep Warp's existing OSC 52 path unchanged. Add a second tmux-control-mode path that parses `%paste-buffer-changed`, validates the buffer name, runs `show-buffer -b <buffer>`, and routes the fetched content through Warp's existing `ClipboardStore` event. Gate all side effects behind a new `terminal.experimental_tmux_clipboard_sync` setting that defaults to disabled.

**Tech Stack:** Rust, WarpUI settings, Warp terminal ANSI/tmux control-mode parser, tmux control mode, existing Warp clipboard events.

---

## Source References

- iTerm2 parses `%paste-buffer-changed buffer1` and validates names with `buffer[0-9]+`: `/Users/david/Code/iTerm2/sources/tmux/TmuxGateway.m:451`.
- iTerm2 gates the mirror behind `kPreferenceKeyTmuxSyncClipboard`: `/Users/david/Code/iTerm2/sources/PTYSession/PTYSession.m:10025`.
- iTerm2 fetches content with `show-buffer -b <buffer>` and writes to `NSPasteboard`: `/Users/david/Code/iTerm2/sources/tmux/TmuxController.m:3644`.
- Warp already parses OSC 52 clipboard writes: `/Users/david/Code/warp/app/src/terminal/model/ansi/mod.rs:958`.
- Warp already emits `Event::ClipboardStore`: `/Users/david/Code/warp/app/src/terminal/model/grid/ansi_handler.rs:1153`.
- Warp already writes `ClipboardStore` content to the OS clipboard: `/Users/david/Code/warp/app/src/terminal/view.rs:10445`.

## File Structure

- Modify: `app/src/terminal/settings.rs`
  - Owns the new persistent setting.
- Modify: `app/src/settings_view/features_page.rs`
  - Owns the Features page toggle and command-palette action.
- Modify: `app/src/terminal/model/tmux/mod.rs`
  - Owns the safe tmux paste-buffer name type and tmux control-mode event.
- Modify: `app/src/terminal/model/tmux/parser.rs`
  - Parses `%paste-buffer-changed`.
- Modify: `app/src/terminal/model/tmux/parser_tests.rs`
  - Covers valid and invalid paste-buffer notifications.
- Modify: `app/src/terminal/model/tmux/commands.rs`
  - Formats `show-buffer -b <buffer>`.
- Modify: `app/src/terminal/model/tmux/mod_tests.rs`
  - Covers paste-buffer name validation.
- Modify: `app/src/terminal/model/ansi/mod.rs`
  - Forwards paste-buffer notifications and unmatched command output.
- Modify: `app/src/terminal/model/terminal_model.rs`
  - Applies the setting gate, tracks pending paste-buffer reads, and emits clipboard events.
- Modify: `app/src/terminal/terminal_manager.rs`
  - Seeds the terminal model with the current setting value when a terminal is created.
- Modify: `app/src/terminal/view.rs`
  - Keeps the terminal model in sync when the setting changes at runtime.

## Task 1: Add the Experimental Setting

**Files:**
- Modify: `app/src/terminal/settings.rs`

- [x] **Step 1: Add a failing settings compile target**

Run:

```bash
cargo check -p warp --bin warp-oss
```

Expected before code change: PASS. This establishes the baseline before adding the new setting.

- [x] **Step 2: Add the setting to `TerminalSettings`**

In `app/src/terminal/settings.rs`, add this entry inside `define_settings_group!(TerminalSettings, settings: [...])` after `show_terminal_zero_state_block`:

```rust
    experimental_tmux_clipboard_sync: ExperimentalTmuxClipboardSync {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        toml_path: "terminal.experimental_tmux_clipboard_sync",
        description: "Whether to mirror tmux paste buffer changes to the OS clipboard.",
    },
```

This setting is local-only because tmux clipboard behavior is terminal-host and security-context specific.

- [x] **Step 3: Run the setting compile check**

Run:

```bash
cargo check -p warp --bin warp-oss
```

Expected: PASS.

- [x] **Step 4: Commit**

```bash
git add app/src/terminal/settings.rs
git commit -m "settings: add experimental tmux clipboard sync flag"
```

## Task 2: Add the Features Page Toggle

**Files:**
- Modify: `app/src/settings_view/features_page.rs`

- [x] **Step 1: Add imports**

Update the existing terminal settings import:

```rust
use crate::terminal::settings::{
    ExperimentalTmuxClipboardSync, MaximumGridSize, ShowTerminalZeroStateBlock, TerminalSettings,
    UseAudibleBell,
};
```

- [x] **Step 2: Add the action variant**

In `FeaturesPageAction`, add:

```rust
    ToggleExperimentalTmuxClipboardSync,
```

Place it near `ToggleUseAudibleBell` and `ToggleShowTerminalZeroStateBlock`.

- [x] **Step 3: Add command-palette toggle binding**

In `init_actions_from_parent_view`, add a `ToggleSettingActionPair` near the audible bell binding:

```rust
        ToggleSettingActionPair::new(
            "experimental tmux clipboard sync",
            builder(SettingsAction::FeaturesPageToggle(
                FeaturesPageAction::ToggleExperimentalTmuxClipboardSync,
            )),
            context,
            flags::EXPERIMENTAL_TMUX_CLIPBOARD_SYNC_FLAG,
        )
        .is_supported_on_current_platform(
            TerminalSettings::as_ref(app)
                .experimental_tmux_clipboard_sync
                .is_supported_on_current_platform(),
        ),
```

In `app/src/settings_view/mod.rs`, add this flag constant inside `pub mod flags`:

```rust
    pub const EXPERIMENTAL_TMUX_CLIPBOARD_SYNC_FLAG: &str =
        "ExperimentalTmuxClipboardSyncEnabled";
```

- [x] **Step 4: Add telemetry mapping**

In `impl FeaturesPageAction`, add the telemetry arm:

```rust
            Self::ToggleExperimentalTmuxClipboardSync => {
                let terminal_settings = TerminalSettings::as_ref(ctx);
                TelemetryEvent::FeaturesPageAction {
                    action: "ToggleExperimentalTmuxClipboardSync".to_string(),
                    value: to_string(*terminal_settings.experimental_tmux_clipboard_sync),
                }
            }
```

- [x] **Step 5: Add action handling**

In the `match action` block for feature actions, add:

```rust
            ToggleExperimentalTmuxClipboardSync => {
                TerminalSettings::handle(ctx).update(ctx, |terminal_settings, ctx| {
                    report_if_error!(terminal_settings
                        .experimental_tmux_clipboard_sync
                        .toggle_and_save_value(ctx));
                })
            }
```

- [x] **Step 6: Add the widget to the Terminal section**

In the Terminal widgets list, after `AudibleBellWidget`, add:

```rust
        if terminal_settings
            .experimental_tmux_clipboard_sync
            .is_supported_on_current_platform()
        {
            terminal_widgets.push(Box::new(ExperimentalTmuxClipboardSyncWidget::default()));
        }
```

- [x] **Step 7: Add the widget type**

Add this widget near `AudibleBellWidget`:

```rust
#[derive(Default)]
struct ExperimentalTmuxClipboardSyncWidget {
    additional_info_link: MouseStateHandle,
    switch_state: SwitchStateHandle,
}

impl SettingsWidget for ExperimentalTmuxClipboardSyncWidget {
    type View = FeaturesPageView;

    fn search_terms(&self) -> &str {
        "experimental tmux clipboard sync paste buffer copy"
    }

    fn render(
        &self,
        view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        let ui_builder = appearance.ui_builder();
        let terminal_settings = TerminalSettings::as_ref(app);
        render_body_item::<FeaturesPageAction>(
            "Experimental tmux clipboard sync".into(),
            Some(AdditionalInfo {
                mouse_state: self.additional_info_link.clone(),
                on_click_action: None,
                secondary_text: None,
                tooltip_override_text: Some(
                    "Mirror tmux paste buffer changes to the system clipboard.".into(),
                ),
            }),
            LocalOnlyIconState::for_setting(
                ExperimentalTmuxClipboardSync::storage_key(),
                ExperimentalTmuxClipboardSync::sync_to_cloud(),
                &mut view
                    .button_mouse_states
                    .local_only_icon_tooltip_states
                    .borrow_mut(),
                app,
            ),
            ToggleState::Enabled,
            appearance,
            ui_builder
                .switch(self.switch_state.clone())
                .check(*terminal_settings.experimental_tmux_clipboard_sync)
                .build()
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(
                        FeaturesPageAction::ToggleExperimentalTmuxClipboardSync,
                    )
                })
                .finish(),
            None,
        )
    }
}
```

- [x] **Step 8: Run the UI compile check**

Run:

```bash
cargo check -p warp --bin warp-oss
```

Expected: PASS.

- [x] **Step 9: Commit**

```bash
git add app/src/settings_view/features_page.rs app/src/settings_view/mod.rs
git commit -m "ui: add experimental tmux clipboard sync toggle"
```

## Task 3: Add a Safe tmux Paste-Buffer Name Type

**Files:**
- Modify: `app/src/terminal/model/tmux/mod.rs`
- Modify: `app/src/terminal/model/tmux/mod_tests.rs`

- [x] **Step 1: Write the failing tests**

Add to `app/src/terminal/model/tmux/mod_tests.rs`:

```rust
#[test]
fn paste_buffer_name_accepts_tmux_auto_buffer_names() {
    let name = PasteBufferName::parse(b"buffer123").expect("buffer name should parse");
    assert_eq!(name.as_str(), "buffer123");
}

#[test]
fn paste_buffer_name_rejects_empty_suffix() {
    assert_eq!(PasteBufferName::parse(b"buffer"), None);
}

#[test]
fn paste_buffer_name_rejects_non_buffer_prefix() {
    assert_eq!(PasteBufferName::parse(b"foo123"), None);
}

#[test]
fn paste_buffer_name_rejects_shell_metacharacters() {
    assert_eq!(PasteBufferName::parse(b"buffer1;display-message owned"), None);
}
```

- [x] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test -p warp terminal::model::tmux::mod_tests::paste_buffer_name --lib
```

Expected: FAIL with `use of undeclared type PasteBufferName`.

- [x] **Step 3: Add the type**

Add to `app/src/terminal/model/tmux/mod.rs` after the imports:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteBufferName(String);

impl PasteBufferName {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let suffix = bytes.strip_prefix(b"buffer")?;
        if suffix.is_empty() || !suffix.iter().all(u8::is_ascii_digit) {
            return None;
        }

        let name = std::str::from_utf8(bytes).ok()?.to_string();
        Some(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
```

- [x] **Step 4: Run tests**

Run:

```bash
cargo test -p warp terminal::model::tmux::mod_tests::paste_buffer_name --lib
```

Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add app/src/terminal/model/tmux/mod.rs app/src/terminal/model/tmux/mod_tests.rs
git commit -m "tmux: add safe paste buffer name type"
```

## Task 4: Parse `%paste-buffer-changed`

**Files:**
- Modify: `app/src/terminal/model/tmux/parser.rs`
- Modify: `app/src/terminal/model/tmux/parser_tests.rs`

- [x] **Step 1: Write parser tests**

Add to `app/src/terminal/model/tmux/parser_tests.rs`:

```rust
#[test]
fn test_paste_buffer_changed_message() {
    let mut parser = TmuxControlModeParser::new();
    let mut handler = TestHandler::new();

    let input = b"%paste-buffer-changed buffer123\n";
    for &byte in input {
        parser.advance(&mut handler, byte);
    }

    assert_eq!(handler.messages.len(), 1);
    assert_eq!(
        &handler.messages[0],
        &TmuxMessage::PasteBufferChanged {
            buffer_name: PasteBufferName::parse(b"buffer123").expect("valid buffer name"),
        }
    );
}

#[test]
fn test_paste_buffer_changed_rejects_invalid_buffer_name() {
    let mut parser = TmuxControlModeParser::new();
    let mut handler = TestHandler::new();

    let input = b"%paste-buffer-changed buffer1;display-message owned\n";
    for &byte in input {
        parser.advance(&mut handler, byte);
    }

    assert_eq!(handler.messages.len(), 1);
    assert!(matches!(
        handler.messages[0],
        TmuxMessage::ParseError { .. }
    ));
}
```

- [x] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test -p warp terminal::model::tmux::parser_tests::test_paste_buffer_changed --lib
```

Expected: FAIL because `PasteBufferChanged` does not exist.

- [x] **Step 3: Add enum variant and parser state**

In `app/src/terminal/model/tmux/parser.rs`, import `PasteBufferName`:

```rust
use super::PasteBufferName;
```

Add to `TmuxMessage`:

```rust
    PasteBufferChanged {
        buffer_name: PasteBufferName,
    },
```

Add to the `Debug` impl match:

```rust
            TmuxMessage::PasteBufferChanged { buffer_name } => f
                .debug_struct("PasteBufferChanged")
                .field("buffer_name", buffer_name)
                .finish(),
```

Add to `ParserState`:

```rust
    TagPasteBufferChanged {
        buffer_name: Vec<u8>,
        saw_name: bool,
    },
```

- [x] **Step 4: Recognize the tag**

In `ParserState::ReadingTag`, add:

```rust
                        b"paste-buffer-changed" => {
                            self.state = ParserState::TagPasteBufferChanged {
                                buffer_name: Vec::new(),
                                saw_name: false,
                            };
                        }
```

- [x] **Step 5: Parse the buffer name**

Add a match arm in `TmuxControlModeParser::advance`:

```rust
            ParserState::TagPasteBufferChanged {
                buffer_name,
                saw_name,
            } => match byte {
                b'\n' => {
                    let Some(name) = PasteBufferName::parse(buffer_name) else {
                        report_parse_error(
                            handler,
                            "Invalid %paste-buffer-changed buffer name",
                            b'\n',
                        );
                        self.state = ParserState::BeginningOfLine;
                        return;
                    };

                    handler.tmux_control_mode_message(TmuxMessage::PasteBufferChanged {
                        buffer_name: name,
                    });
                    self.state = ParserState::BeginningOfLine;
                }
                b' ' if !*saw_name => {}
                b' ' => {
                    report_parse_error(
                        handler,
                        "Unexpected space in %paste-buffer-changed buffer name",
                        byte,
                    );
                    self.state = ParserState::Error;
                }
                byte => {
                    *saw_name = true;
                    buffer_name.push(byte);
                }
            },
```

- [x] **Step 6: Run parser tests**

Run:

```bash
cargo test -p warp terminal::model::tmux::parser_tests::test_paste_buffer_changed --lib
```

Expected: PASS.

- [x] **Step 7: Commit**

```bash
git add app/src/terminal/model/tmux/parser.rs app/src/terminal/model/tmux/parser_tests.rs
git commit -m "tmux: parse paste buffer change notifications"
```

## Task 5: Add tmux Command Formatting for `show-buffer`

**Files:**
- Modify: `app/src/terminal/model/tmux/commands.rs`
- Modify: `app/src/terminal/model/tmux/mod_tests.rs`

- [x] **Step 1: Write the command formatting test**

Add to `app/src/terminal/model/tmux/mod_tests.rs`:

```rust
#[test]
fn show_paste_buffer_command_formats_show_buffer() {
    let buffer_name = PasteBufferName::parse(b"buffer7").expect("valid buffer name");
    let command = commands::TmuxCommand::ShowPasteBuffer { buffer_name };

    assert_eq!(command.get_command_string(), "show-buffer -b buffer7\n");
}
```

- [x] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test -p warp terminal::model::tmux::mod_tests::show_paste_buffer_command_formats_show_buffer --lib
```

Expected: FAIL because `ShowPasteBuffer` does not exist.

- [x] **Step 3: Add the command**

In `app/src/terminal/model/tmux/commands.rs`, import the type:

```rust
use super::PasteBufferName;
```

Add to `TmuxCommand`:

```rust
    /// Fetches the contents of a validated tmux paste buffer.
    ShowPasteBuffer {
        buffer_name: PasteBufferName,
    },
```

Add to `get_command_string`:

```rust
            TmuxCommand::ShowPasteBuffer { buffer_name } => {
                format!("show-buffer -b {}\n", buffer_name.as_str())
            }
```

- [x] **Step 4: Run the command test**

Run:

```bash
cargo test -p warp terminal::model::tmux::mod_tests::show_paste_buffer_command_formats_show_buffer --lib
```

Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add app/src/terminal/model/tmux/commands.rs app/src/terminal/model/tmux/mod_tests.rs
git commit -m "tmux: format show-buffer command"
```

## Task 6: Forward tmux Paste-Buffer Events and Command Output

**Files:**
- Modify: `app/src/terminal/model/tmux/mod.rs`
- Modify: `app/src/terminal/model/ansi/mod.rs`

- [x] **Step 1: Add control-mode events**

In `app/src/terminal/model/tmux/mod.rs`, add variants to `ControlModeEvent`:

```rust
    /// A tmux paste buffer changed. The buffer name has already been validated.
    PasteBufferChanged {
        buffer_name: PasteBufferName,
    },
    /// Output from a tmux control-mode command that was not consumed by internal tmux setup parsing.
    CommandOutput {
        output_lines: Result<Vec<Vec<u8>>, Vec<Vec<u8>>>,
    },
```

- [x] **Step 2: Forward paste-buffer messages**

In `app/src/terminal/model/ansi/mod.rs`, update `TmuxPerformer::tmux_control_mode_message`:

```rust
            TmuxMessage::PasteBufferChanged { buffer_name } => {
                self.handler
                    .tmux_control_mode_event(ControlModeEvent::PasteBufferChanged {
                        buffer_name,
                    });
            }
```

- [x] **Step 3: Forward unmatched command output**

Replace the existing `TmuxMessage::CommandOutput` arm with:

```rust
            TmuxMessage::CommandOutput { output_lines } => {
                match &output_lines {
                    Ok(lines) => {
                        let mut handled_internal_command = false;
                        for line in lines {
                            let Some(command) = parse_command(line.clone()) else {
                                continue;
                            };
                            handled_internal_command = true;
                            match command {
                                TmuxCommandResponse::SetPrimaryWindowPane { window_id, pane_id } => {
                                    self.state.pane_for_window.insert(window_id, pane_id);
                                    self.init_primary_pane(window_id, pane_id);
                                }
                                TmuxCommandResponse::BackgroundWindow { window_id, pane_id } => {
                                    self.state.pane_for_window.insert(window_id, pane_id);
                                }
                            }
                        }

                        if !handled_internal_command {
                            self.handler
                                .tmux_control_mode_event(ControlModeEvent::CommandOutput {
                                    output_lines,
                                });
                        }
                    }
                    Err(_) => {
                        self.handler
                            .tmux_control_mode_event(ControlModeEvent::CommandOutput {
                                output_lines,
                            });
                    }
                }
            }
```

- [x] **Step 4: Update exhaustive matches**

Update any `match ControlModeEvent` call sites to include the two new variants. The main one is `TerminalModel::tmux_control_mode_event` in `app/src/terminal/model/terminal_model.rs`.

- [x] **Step 5: Run compile check**

Run:

```bash
cargo check -p warp --bin warp-oss
```

Expected: PASS after all exhaustive matches are updated.

- [x] **Step 6: Commit**

```bash
git add app/src/terminal/model/tmux/mod.rs app/src/terminal/model/ansi/mod.rs app/src/terminal/model/terminal_model.rs
git commit -m "tmux: forward paste buffer control mode events"
```

## Task 7: Gate and Mirror tmux Paste Buffers in TerminalModel

**Files:**
- Modify: `app/src/terminal/model/terminal_model.rs`
- Modify: `app/src/terminal/terminal_manager.rs`
- Modify: `app/src/terminal/view.rs`

- [x] **Step 1: Add state**

In `app/src/terminal/model/terminal_model.rs`, merge `VecDeque` into the existing `std::collections` import:

```rust
use std::collections::{HashMap, VecDeque};
```

Add these fields to `TerminalModel`:

```rust
    pending_tmux_paste_buffer_reads: VecDeque<tmux::PasteBufferName>,
    experimental_tmux_clipboard_sync_enabled: bool,
```

Initialize them in `TerminalModel::new_internal`:

```rust
            pending_tmux_paste_buffer_reads: VecDeque::new(),
            experimental_tmux_clipboard_sync_enabled: false,
```

- [x] **Step 2: Add a runtime setting setter**

Add this method to `impl TerminalModel`:

```rust
    pub fn set_experimental_tmux_clipboard_sync_enabled(&mut self, enabled: bool) {
        self.experimental_tmux_clipboard_sync_enabled = enabled;
        if !enabled {
            self.pending_tmux_paste_buffer_reads.clear();
        }
    }
```

- [x] **Step 3: Seed the model from settings at creation time**

In `app/src/terminal/terminal_manager.rs`, replace the direct `TerminalModel::new(...)` return with a local variable:

```rust
    let mut model = TerminalModel::new(
        restored_blocks.map(|v| v.as_slice()),
        sizes,
        terminal_colors_list(ctx),
        channel_event_proxy,
        ctx.background_executor().clone(),
        should_show_bootstrap_block,
        should_show_in_band_command_blocks,
        show_memory_stats,
        honor_ps1,
        is_inverted,
        obfuscate_secrets,
        is_ai_ugc_telemetry_enabled,
        startup_directory,
        shell_state,
    );
    model.set_experimental_tmux_clipboard_sync_enabled(
        *TerminalSettings::as_ref(ctx).experimental_tmux_clipboard_sync,
    );
    model
```

- [x] **Step 4: Keep the model synced after settings changes**

In `app/src/terminal/view.rs`, extend the existing `TerminalSettingsChangedEvent` subscription with:

```rust
                TerminalSettingsChangedEvent::ExperimentalTmuxClipboardSync { .. } => {
                    me.model
                        .lock()
                        .set_experimental_tmux_clipboard_sync_enabled(
                            *terminal_settings
                                .as_ref(ctx)
                                .experimental_tmux_clipboard_sync,
                        );
                }
```

- [x] **Step 5: Add output conversion helper**

Near other small helpers in `terminal_model.rs`, add:

```rust
fn tmux_command_output_lines_to_text(output_lines: Vec<Vec<u8>>) -> Option<String> {
    let mut bytes = Vec::new();
    for (index, line) in output_lines.into_iter().enumerate() {
        if index > 0 {
            bytes.push(b'\n');
        }
        bytes.extend(line);
    }

    String::from_utf8(bytes).ok()
}
```

- [x] **Step 6: Handle paste-buffer changed events**

In `TerminalModel::tmux_control_mode_event`, add:

```rust
            tmux::ControlModeEvent::PasteBufferChanged { buffer_name } => {
                if !self.experimental_tmux_clipboard_sync_enabled {
                    return;
                }

                self.pending_tmux_paste_buffer_reads
                    .push_back(buffer_name.clone());
                self.emit_handler_event(HandlerEvent::RunTmuxCommand(
                    TmuxCommand::ShowPasteBuffer { buffer_name },
                ));
            }
```

- [x] **Step 7: Handle command output**

In the same match, add:

```rust
            tmux::ControlModeEvent::CommandOutput { output_lines } => {
                if self.pending_tmux_paste_buffer_reads.pop_front().is_none() {
                    return;
                }

                match output_lines {
                    Ok(lines) => {
                        let Some(contents) = tmux_command_output_lines_to_text(lines) else {
                            log::warn!("tmux paste buffer contained non-UTF-8 content");
                            return;
                        };

                        self.event_proxy.send_terminal_event(Event::ClipboardStore(
                            ClipboardType::Clipboard,
                            contents,
                        ));
                    }
                    Err(lines) => {
                        log::warn!(
                            "Failed to read tmux paste buffer: {:?}",
                            AsciiDebug(&lines.concat())
                        );
                    }
                }
            }
```

Add `ClipboardType` to the existing terminal imports in `app/src/terminal/model/terminal_model.rs`:

```rust
use crate::terminal::{block_filter::BlockFilterQuery, model::ansi::Handler, ClipboardType};
```

- [x] **Step 8: Clear pending reads on tmux exit**

In the existing `ControlModeEvent::Exited` arm, add:

```rust
                self.pending_tmux_paste_buffer_reads.clear();
```

- [x] **Step 9: Run compile check**

Run:

```bash
cargo check -p warp --bin warp-oss
```

Expected: PASS.

- [x] **Step 10: Commit**

```bash
git add app/src/terminal/model/terminal_model.rs app/src/terminal/terminal_manager.rs app/src/terminal/view.rs
git commit -m "terminal: mirror tmux paste buffers when enabled"
```

## Task 8: Add Focused Unit Tests for TerminalModel Behavior

**Files:**
- Modify: `app/src/terminal/model/terminal_model.rs`
- Modify: `app/src/terminal/model/terminal_model_tests.rs`

- [x] **Step 1: Add helper tests for output conversion**

Add these tests to `app/src/terminal/model/terminal_model_tests.rs`:

```rust
#[test]
fn tmux_command_output_lines_to_text_preserves_multiline_text() {
    let text = tmux_command_output_lines_to_text(vec![
        b"first line".to_vec(),
        b"second line".to_vec(),
    ]);

    assert_eq!(text.as_deref(), Some("first line\nsecond line"));
}

#[test]
fn tmux_command_output_lines_to_text_rejects_non_utf8() {
    let text = tmux_command_output_lines_to_text(vec![vec![0xff]]);

    assert_eq!(text, None);
}
```

- [x] **Step 2: Add disabled-setting behavior test**

Add:

```rust
#[test]
fn tmux_paste_buffer_change_is_ignored_when_setting_disabled() {
    let (event_tx, event_rx) = async_channel::unbounded();
    let event_proxy = ChannelEventListener::builder_for_test()
        .with_terminal_events_tx(event_tx)
        .build();
    let mut terminal = TerminalModel::mock(None, Some(event_proxy));

    terminal.tmux_control_mode_event(tmux::ControlModeEvent::PasteBufferChanged {
        buffer_name: tmux::PasteBufferName::parse(b"buffer1").expect("valid buffer name"),
    });

    assert!(terminal.pending_tmux_paste_buffer_reads.is_empty());
    assert!(event_rx.is_empty());
}
```

- [x] **Step 3: Add enabled-setting command dispatch test**

Add:

```rust
#[test]
fn tmux_paste_buffer_change_queues_show_buffer_when_setting_enabled() {
    let (event_tx, event_rx) = async_channel::unbounded();
    let event_proxy = ChannelEventListener::builder_for_test()
        .with_terminal_events_tx(event_tx)
        .build();
    let mut terminal = TerminalModel::mock(None, Some(event_proxy));
    terminal.set_experimental_tmux_clipboard_sync_enabled(true);

    terminal.tmux_control_mode_event(tmux::ControlModeEvent::PasteBufferChanged {
        buffer_name: tmux::PasteBufferName::parse(b"buffer1").expect("valid buffer name"),
    });

    assert_eq!(terminal.pending_tmux_paste_buffer_reads.len(), 1);
    match event_rx.try_recv().expect("expected tmux command event") {
        Event::Handler(HandlerEvent::RunTmuxCommand(TmuxCommand::ShowPasteBuffer {
            buffer_name,
        })) => assert_eq!(buffer_name.as_str(), "buffer1"),
        event => panic!("expected show-buffer handler event, got {event:?}"),
    }
}
```

- [x] **Step 4: Add command output clipboard event test**

Add:

```rust
#[test]
fn tmux_command_output_with_pending_paste_buffer_emits_clipboard_store() {
    let (event_tx, event_rx) = async_channel::unbounded();
    let event_proxy = ChannelEventListener::builder_for_test()
        .with_terminal_events_tx(event_tx)
        .build();
    let mut terminal = TerminalModel::mock(None, Some(event_proxy));
    terminal.set_experimental_tmux_clipboard_sync_enabled(true);

    terminal.tmux_control_mode_event(tmux::ControlModeEvent::PasteBufferChanged {
        buffer_name: tmux::PasteBufferName::parse(b"buffer1").expect("valid buffer name"),
    });
    let _ = event_rx.try_recv().expect("discard show-buffer command event");

    terminal.tmux_control_mode_event(tmux::ControlModeEvent::CommandOutput {
        output_lines: Ok(vec![b"copied text".to_vec()]),
    });

    match event_rx.try_recv().expect("expected clipboard event") {
        Event::ClipboardStore(ClipboardType::Clipboard, contents) => {
            assert_eq!(contents, "copied text");
        }
        event => panic!("expected clipboard store event, got {event:?}"),
    }
    assert!(terminal.pending_tmux_paste_buffer_reads.is_empty());
}
```

- [x] **Step 5: Run focused tests**

Run:

```bash
cargo test -p warp terminal::model::terminal_model --lib
```

Expected: PASS.

- [x] **Step 6: Commit**

```bash
git add app/src/terminal/model/terminal_model.rs app/src/terminal/model/terminal_model_tests.rs
git commit -m "terminal: test tmux paste buffer clipboard sync"
```

## Task 9: End-to-End Verification

**Files:**
- No planned source modifications.

- [x] **Step 1: Run tmux parser and command tests**

Run:

```bash
cargo test -p warp terminal::model::tmux --lib
```

Expected: PASS.

- [x] **Step 2: Run terminal model tests**

Run:

```bash
cargo test -p warp terminal::model::terminal_model --lib
```

Expected: PASS.

- [x] **Step 3: Run compile check**

Run:

```bash
cargo check -p warp --bin warp-oss
```

Expected: PASS.

- [x] **Step 4: Format**

Run:

```bash
cargo fmt
```

Expected: exits 0 and leaves only intended formatting changes.

- [ ] **Step 5: Manual macOS validation**

Not run in this agent pass: this requires launching the Warp GUI and performing a real tmux copy-mode gesture plus native app paste check.

Run Warp with the feature disabled first:

```bash
cargo run -p warp --bin warp-oss
```

Inside Warp:

```bash
tmux -CC
```

Copy text using tmux copy-mode. Expected: OS clipboard is unchanged while `terminal.experimental_tmux_clipboard_sync` is false.

Enable the setting in Settings → Features → Terminal. Repeat the tmux copy-mode copy. Expected: OS clipboard contains the tmux paste-buffer text and can be pasted into a native macOS app.

- [ ] **Step 6: Verify normal OSC 52 still works**

Not run in this agent pass: this requires exercising the running Warp GUI clipboard integration in a non-tmux terminal session.

Run inside a non-tmux shell:

```bash
printf '\033]52;c;%s\a' "$(printf 'warp-osc52-check' | base64)"
```

Expected: OS clipboard contains `warp-osc52-check`.

- [x] **Step 7: Final status check**

Run:

```bash
git status --short
```

Expected: only intentional files are modified.

## Risk Notes

- The first implementation should preserve Warp's existing OSC 52 behavior. Do not route ordinary OSC 52 through the new tmux setting.
- The buffer name must remain a validated type. Do not accept arbitrary names and escape them later.
- Command-output association is the main correctness risk. The queue-based design is acceptable because the feature sends one `show-buffer` command per notification and tmux control-mode command output is serialized. If manual testing shows interleaving, parse `%begin` command numbers and track outbound control-mode command order explicitly.
- Binary paste buffers are out of scope for the first pass. Non-UTF-8 content should be ignored with a warning rather than writing lossy text to the clipboard.

## References

- tmux control mode: https://github.com/tmux/tmux/wiki/Control-Mode
- tmux clipboard and OSC 52: https://github.com/tmux/tmux/wiki/Clipboard
- iTerm2 source: https://github.com/gnachman/iTerm2
- Windows Terminal source: https://github.com/microsoft/terminal
