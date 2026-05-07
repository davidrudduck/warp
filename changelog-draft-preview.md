# Changelog Draft
**Channel:** stable
**Range:** v0.2026.04.29.08.56.stable_00 → v0.2026.05.06.09.12.stable_00
**Generated:** 2026-05-06T19:47:57Z
**Total PRs:** 211 | **Explicit markers:** 57 | **Unmarked:** 154

---

## New Features
- You can now drag tabs out of a window into their own window, or between windows, similar to Chrome. (#9275)
- Added a `/set-tab-color` slash command for setting or clearing the current tab's color from the input bar. (#9305) — @sebryu ✨

## Improvements
- Added tab context menu actions to copy visible tab and pane metadata when available. (#10120)
- The conversation details panelcan now be opened and closed with a configurable keyboard shortcut. (#9837)
- Conversation details side panel is now available for local Warp Agent conversations, not just cloud Oz runs. Click the info button in the pane header to open it for any active AI conversation. (#9493)
- Reduced memory usage and CPU work in the agent runs management view while a conversation is streaming. (#9866)
- Added support for drag-and-drop of image files into an active CLI agent session (e.g. Claude Code). (#9553) — @SagarSDagdu ✨
- Warp now renders inline local images and Mermaid diagrams in agent block output. (#9993)
- CHANGELOG-BUG-FIX: Fix macOS IME candidate popup positioning in code editor panes so it anchors to the editor caret instead of stale terminal/input positions. (#9555) — @qubaitian ✨
- Warp now silently falls back to a regular SSH session on remote hosts where the prebuilt remote-server binary is incompatible (e.g. glibc < 2.31), instead of attempting an install that would fail at runtime. (#9681)
- Tighten orchestration event subscription scope so SSE runs only for active parent and child agent runs. (#9273)
- HTML files using the .htm extension now open with HTML syntax highlighting in Warp's editor. (#9360) — @Abdalla-Eldoumani ✨
- Recognize Block's `goose` CLI agent — running `goose` now activates the CLI-agent toolbar, status, brand color, and icon like other recognized third-party agents. (#9497) — @webdevtodayjason ✨
- Added a `/continue-locally` slash command to continue cloud conversations locally. (#9500)
- Added a "Show in Finder" (macOS) / "Show containing folder" (Linux/Windows) option to the tooltip that appears when clicking a detected file link. (#9475)

## Bug Fixes
- Fixed /feedback recording "Unknown" instead of the installed Warp version on packaged builds. (#10219) — @SagarSDagdu ✨
- Fixed find (cmd+f) selection jumping to a different match when new output streams into the active block. (#10057) — @amazansky ✨
- Fix Japanese IME losing the last character of a phrase that ends right before a punctuation mark on macOS. (#9730) — @s-zaizen ✨
- Fixed local file tree blinking/reshuffling when connected to an SSH session (#10184)
- Fixed terminal text selection not auto-scrolling when dragging beyond bounds (#9448)
- Fixed Ctrl-G not closing CLI agent rich input on linux when editor is focused (fixes #9286) (#10030) — @nihalxkumar ✨
- Pressing backspace in the agent view when the buffer is empty no longer resets the conversation. (#10114)
- Fixed unnecessary reconnect attempts for remote SSH sessions after system sleep, reducing error noise (#10096)
- Fixes issue with repeated TUI redraws for CLI agents on terminal pane resize. (#9877)
- Fix new-session "+" dropdown alignment when the Tabs Panel is placed on the right side of the header toolbar. (#9492) — @R3flector ✨
- Copy keybinding now prioritizes selected text in the input over a selected block when both are active. (#9491)
- [Windows] Fix hotkey window. (#9891)
- [Windows] Symlink traversal fixed. (#9863)
- Fixed a crash on Windows when handing off a Web conversation to the native client ("Grid received input but did not receive Reset Grid OSC"). (#9987)
- Fixed a bug where multiple 'open skill' buttons shared hover state. (#9437)
- Fixed the OSS Linux desktop entry so WarpOss launches through the packaged `warp-terminal-oss` command. (#9424) — @princepal9120 ✨
- Fixed Ctrl/Cmd shortcuts (e.g. copy, paste) failing on Windows when a non-Latin keyboard layout was active. (#9476) — @landkirk ✨
- CHANGELOG-IMAGE: {{GCP-hosted URL goes here...}} (#9555) — @qubaitian ✨
- Fixed `/open-file` handling for relative WSL paths so Unix separators are preserved before opening files on Windows hosts. (#9322) — @kranthik10 ✨
- Fixed background colour bleeding in alt screen programs (e.g. delta, diff-so-fancy) where coloured regions would incorrectly fill the entire viewport when they dominated the visible area. (#9852) — @JamieMcMillan ✨
- Clip the warping indicator's action chips (e.g. "Hide responses", "Take over", auto-approve, queue-next-prompt, stop) onto a new line on narrow panes instead of overflowing into the adjacent pane. (#9297)
- Inline `.bmp`, `.tiff` / `.tif`, and `.ico` images in agent block output now render correctly instead of falling back to plain text. (#9397) — @anshul-garg27 ✨
- if user attaches an image in block input we should lock in agent mode, without running the NLD classifier to remove uncertainty (#9366)
- Remote-server installs no longer fail when the staging-directory cleanup hits a "Directory not empty" race after the binary has already been moved into place. (#9681)
- `.command` shell scripts now open with shell syntax highlighting in Warp's editor. (#9345) — @anshul-garg27 ✨
- Fix git diff chip flickering between tracked-only and all-files count when untracked files are present (#9244) — @zerone0x ✨
- `Open File → Default App` now opens files in the running Warp channel instead of routing to a different installed Warp. (#9285) — @Faizanq ✨
- Fixed vertical tabs settings popup items (View as, Density, Pane title as) being unclickable (#9540) — @leozeli ✨
- Fixed a macOS memory leak that occurred when Warp enumerated system fonts or built a font fallback chain. (#9665)
- Executable shell scripts opened from a \`file://\` URL now run in the terminal instead of opening in the editor. (#9503) — @amriksingh0786 ✨
- Co-Authored-By: Warp <agent@warp.dev> (#9538) — @landkirk ✨
- Fixed Option+Enter, Option+Tab, and Option+Escape sending literal key names instead of correct escape sequences (#9514) — @oliver-ni ✨
- Fixed read_files tool showing an empty box when the LLM requests line ranges beyond the end of a file. (#9326)
- Prevent Warp from consuming too much memory when identifying filepaths in long block outputs. (#9617)
- Don't trigger the agent onboarding tutorial when Warp is running in headless SDK/CLI mode. (#9590)
- Added `--version` flag support in the Oz CLI (#9252)
- Fixed file tree flickering when transitioning to an SSH remote session (#9320)
- Fixed scroll-to-start/end of selected block keybinding (Cmd/Ctrl+Shift+Up/Down) not working when the input is focused. (#9332)
- Fix the terminal pane background appearing darker in horizontal tabs mode when the active theme has a background image or a custom window opacity. (#9474)
- AI code blocks tagged `vue`, `xml`, `dockerfile`, `jsx`, `tsx`, `objective-c`, or `starlark` now render with syntax highlighting. Common aliases like `rs`, `py`, `js`, `ts`, `yml`, `kt`, `rb`, `golang`, `terraform`, and `docker` are also recognized. (#9471) — @Abdalla-Eldoumani ✨
- Reopen Closed Session is now reachable from the new-session menu on Linux and Windows. (#9347) — @mvanhorn ✨
- Fixed missing syntax highlighting for C++ header files using `.hpp`, `.hxx`, or `.H` extensions. (#9388) — @princepal9120 ✨

## Oz Updates
- Add Codex as a supported harness for local child agents. (#10176)
- Configurable max context window per profile. (#9352)

---

## Needs Review
These entries touch feature flags or have ambiguous scope — a human should verify:
- [ ] (#9988) "open local->cloud mode conversation in the same pane" — touches warp_features — verify channel visibility
- [ ] (#9455) "implement basic local cloud handoff UI" — touches warp_features — verify channel visibility
- [ ] (#9680) "Orchestration pill bar updates: same-pane pills, 3-dot menu, hover card, breadcr" — touches warp_features — verify channel visibility
- [ ] (#9628) "[QUALITY-569] Stage 1: orchestrate tool (client)" — touches warp_features — verify channel visibility
- [ ] (#9991) "enable tab dragging between windows for internal warp users" — touches warp_features — verify channel visibility
- [ ] (#9364) "Enabled cloud mode input v2 on dogfood." — touches warp_features — verify channel visibility
- [ ] (#9449) "Add experiment setup for SSH" — touches warp_features — verify channel visibility
- [ ] (#9313) "Add feature flag, API binding scaffolding for cloud->cloud handoff." — touches warp_features — verify channel visibility
- [ ] (#9334) "Orchestration pills bar in Agent View (1/N)" — touches warp_features — verify channel visibility
- [ ] (#9265) "Remove orchestration_event_push feature flag; rename poller to streamer" — touches warp_features — verify channel visibility

## Skipped PRs
| PR | Author | Reason |
|----|--------|--------|
| #9655 | Akeuuh | no marker, needs human triage |
| #10220 | captainsafia | CI/test/docs only |
| #10197 | zachbai | no marker, needs human triage |
| #10204 | captainsafia | no marker, needs human triage |
| #10203 | cephalonaut | no marker, needs human triage |
| #10206 | advait-m | no marker, needs human triage |
| #10167 | zachbai | no marker, needs human triage |
| #10199 | zachlloyd | no marker, needs human triage |
| #10183 | harryalbert | no marker, needs human triage |
| #9840 | exzshao | no marker, needs human triage |
| #9452 | exzshao | no marker, needs human triage |
| #9653 | harryalbert | no marker, needs human triage |
| #10188 | kevinyang372 | no marker, needs human triage |
| #10172 | harryalbert | no marker, needs human triage |
| #10186 | kevinyang372 | no marker, needs human triage |
| ... | ... | *(144 total skipped)* |

## External Contributors
- @Abdalla-Eldoumani — #9360, #9471
- @Akeuuh — #9655
- @AntonVishal — #9283
- @BennyWaitWhat — #9691, #9409, #9363, #9348
- @Faizanq — #9285
- @JamieMcMillan — #9852
- @R3flector — #9492
- @SagarSDagdu — #10219, #10130, #9553
- @amazansky — #10057
- @amriksingh0786 — #9503
- @anshul-garg27 — #9600, #9699, #9397, #9345, #9403, #9603, #9341, #9405, #9407, #9408, #9410, #9343, #9400, #9406, #9346, #9336, #9337, #9338
- @antonkesy — #9318
- @app/oz-for-oss — #9886, #9887
- @bradleyjames — #9885
- @exzshao — #9840, #9452, #9480, #9464, #9238, #9441
- @gulsahsarsilmaz — #9910
- @kranthik10 — #9322
- @landkirk — #9476, #9538, #9527
- @leozeli — #9540
- @mvanhorn — #9347
- @nihalxkumar — #10030
- @oliver-ni — #9514
- @princepal9120 — #9424, #9388
- @qubaitian — #9555
- @rudrabhoj — #9362
- @s-zaizen — #9730
- @sebryu — #9489, #9843, #9589, #9481, #9305
- @tautik — #9501
- @webdevtodayjason — #9497
- @zerone0x — #9244

---

*This draft was generated by the `changelog-draft` Oz skill. Review the "Needs Review" section and verify skipped PRs before publishing.*
