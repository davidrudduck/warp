---
name: warp-ui-ux-review
description: Reviews and guides Warp UI changes for terminal-first usability, visual polish, accessibility, keyboard flow, dense settings surfaces, and agentic AI workflows. Use before implementing UI and again before completion for app, settings, terminal, onboarding, or agent views.
---

# Warp UI UX Review

Use this skill with `warp-ui-guidelines`. This skill is a workflow; `warp-ui-guidelines` contains concrete design-system rules.

## Product Fit

Warp is a terminal-first development app. UI should feel fast, focused, and operational:

- Prefer dense, scannable layouts over marketing-style composition.
- Keep primary terminal work visible and uninterrupted where possible.
- Make agentic AI feel organic: clear entry points, readable status, cancellable work, and no surprise background action.
- Avoid explanatory UI copy when familiar controls, labels, and states are enough.
- Use existing settings and view patterns before inventing new surfaces.

## Interaction Checklist

Check these before coding and again before finishing:

- Keyboard path exists for the primary action.
- Focus lands where the user expects after opening, saving, cancelling, or changing modes.
- Busy, success, error, empty, and disabled states are visible and useful.
- Destructive or externally visible actions require clear confirmation.
- Long-running agent or terminal actions can be cancelled or handed back to the user where relevant.
- Text does not overflow buttons, controls, sidebars, or compact cards.
- Inputs have visible chrome, placeholder behavior, and validation feedback.
- Settings controls write through the settings system, not local view-only state.

## Visual Checklist

- Reuse existing WarpUI components, `ActionButton` themes, settings sections, nav items, and theme colors.
- Use icons for icon-shaped actions and tooltips for unfamiliar icons.
- Use restrained contrast and spacing. Terminal productivity beats decorative density.
- Keep cards for repeated items, modals, or framed tools; avoid nested cards.
- Check light/dark theme colors through `appearance.theme()` and internal color helpers.
- Keep border radii modest unless an existing component requires otherwise.

## Accessibility

- Preserve labels or equivalent accessible names for inputs and buttons.
- Provide non-color state signals for errors and success.
- Keep focus order coherent.
- Check contrast for text and icons, including disabled states.
- For modal-like flows, check focus trap, escape/cancel behavior, and return focus.

External references when deeper design review is needed:

- Apple Human Interface Guidelines: https://developer.apple.com/design/human-interface-guidelines
- WCAG 2.2: https://www.w3.org/TR/wcag/

## Validation

Use the smallest meaningful proof:

- pure helper logic: unit tests
- view/model state: `warpui::App::test` or existing view tests
- critical user flow: `warp-integration-test`
- visual-heavy changes: run the app or capture screenshots when possible

In the final report, name the UI states and paths actually verified.
