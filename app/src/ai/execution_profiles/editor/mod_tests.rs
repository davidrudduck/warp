use super::ui_helpers::{context_window_snap_values, direct_api_agent_backend_selector_state};
use crate::ai::execution_profiles::{AIExecutionProfile, DirectApiAgentBackend, ModelRouting};

/// Helper: round-trip f32 → u32 for readable assertions and absorb the
/// negligible f64→f32 drift the snap helper picks up on large ranges.
fn rounded(values: &[f32]) -> Vec<u32> {
    values.iter().map(|v| v.round() as u32).collect()
}

fn direct_api_profile_with_backend(backend: DirectApiAgentBackend) -> AIExecutionProfile {
    AIExecutionProfile {
        model_routing: ModelRouting::DirectApi,
        direct_api_agent_backend: backend,
        ..AIExecutionProfile::default()
    }
}

#[test]
fn execution_profile_editor_hides_rig_backend_selector_when_gate_disabled() {
    let profile = direct_api_profile_with_backend(DirectApiAgentBackend::Native);
    let state = direct_api_agent_backend_selector_state(&profile, false, true);

    assert!(state.is_none());
}

#[test]
fn execution_profile_editor_shows_rig_backend_selector_for_direct_api_when_gate_enabled() {
    let profile = direct_api_profile_with_backend(DirectApiAgentBackend::Native);
    let state = direct_api_agent_backend_selector_state(&profile, true, true)
        .expect("Direct API profile with enabled gate should show backend selector");

    assert_eq!(state.selected_backend, DirectApiAgentBackend::Native);
    assert_eq!(
        state.option_labels(),
        vec!["Native".to_string(), "Rig Agent".to_string()]
    );
    assert!(state.options.iter().all(|option| option.enabled));
}

#[test]
fn execution_profile_editor_hides_rig_backend_selector_for_warp_provider() {
    let profile = AIExecutionProfile {
        model_routing: ModelRouting::WarpProvider,
        ..AIExecutionProfile::default()
    };
    let state = direct_api_agent_backend_selector_state(&profile, true, true);

    assert!(state.is_none());
}

#[test]
fn execution_profile_editor_disables_rig_backend_selector_option_without_feature() {
    let profile = direct_api_profile_with_backend(DirectApiAgentBackend::RigAgent);
    let state = direct_api_agent_backend_selector_state(&profile, true, false)
        .expect("Direct API profile with enabled gate should show backend selector");

    assert_eq!(state.selected_backend, DirectApiAgentBackend::Native);
    assert_eq!(
        state.disabled_state_label,
        Some("Rig Agent backend is unavailable in this build. Native will be used.")
    );
    assert_eq!(
        state
            .options
            .iter()
            .map(|option| (option.label, option.enabled))
            .collect::<Vec<_>>(),
        vec![("Native", true), ("Rig Agent", false)]
    );
}

#[test]
fn snap_values_for_min_eq_max_returns_single_point() {
    assert_eq!(
        rounded(&context_window_snap_values(50_000, 50_000)),
        vec![50_000]
    );
}

#[test]
fn snap_values_for_min_gt_max_collapses_to_min() {
    // Defensive: invalid bounds shouldn't panic, just degrade gracefully.
    assert_eq!(rounded(&context_window_snap_values(100, 50)), vec![100]);
}

#[test]
fn snap_values_always_include_endpoints() {
    let values = rounded(&context_window_snap_values(1_000, 200_000));
    assert_eq!(values.first(), Some(&1_000));
    assert_eq!(values.last(), Some(&200_000));
}

#[test]
fn snap_values_for_classic_200k_range_match_legacy_layout() {
    // Mirrors the old hardcoded list, except `1_000` replaces the missing
    // round multiple at the start.
    let values = rounded(&context_window_snap_values(1_000, 200_000));
    assert_eq!(
        values,
        vec![1_000, 25_000, 50_000, 75_000, 100_000, 125_000, 150_000, 175_000, 200_000]
    );
}

#[test]
fn snap_values_for_claude_1m_range_pick_100k_steps() {
    let values = rounded(&context_window_snap_values(200_000, 1_000_000));
    assert_eq!(
        values,
        vec![200_000, 300_000, 400_000, 500_000, 600_000, 700_000, 800_000, 900_000, 1_000_000]
    );
}

#[test]
fn snap_values_for_min_zero_skips_duplicate_zero() {
    let values = rounded(&context_window_snap_values(0, 100));
    // First entry is min (0), then nice multiples up to and including max.
    assert_eq!(values.first(), Some(&0));
    assert_eq!(values.last(), Some(&100));
    assert!(values.iter().filter(|&&v| v == 0).count() == 1);
}

#[test]
fn snap_values_for_offset_min_align_to_nice_grid() {
    // min=26_000 doesn't sit on a 25k boundary; first nice value is 50_000.
    let values = rounded(&context_window_snap_values(26_000, 200_000));
    assert_eq!(values.first(), Some(&26_000));
    assert_eq!(values.last(), Some(&200_000));
    // Ensure the second point lands on a nice multiple, not on min+step.
    assert_eq!(values.get(1), Some(&50_000));
}

#[test]
fn snap_values_keep_count_reasonable_for_huge_range() {
    // 1B span should still produce a small (~9) snap-point list, not
    // millions of entries.
    let values = context_window_snap_values(0, 1_000_000_000);
    assert!(
        values.len() <= 12,
        "expected at most 12 snap points, got {}",
        values.len()
    );
    assert!(
        values.len() >= 5,
        "expected at least 5 snap points, got {}",
        values.len()
    );
}
