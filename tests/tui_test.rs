use modelrouter::{ProviderId, TaskHint, TuiEvent, TuiState, TuiView};

#[test]
fn tui_state_edits_prompt_and_cycles_controls() {
    let mut state = TuiState::default();

    state.apply(TuiEvent::Insert('h'));
    state.apply(TuiEvent::Insert('i'));
    state.apply(TuiEvent::CycleProvider);
    state.apply(TuiEvent::CycleHint);

    assert_eq!(state.prompt(), "hi");
    assert_eq!(state.prefer(), Some(ProviderId::Local));
    assert_eq!(state.hint(), Some(TaskHint::Simple));
}

#[test]
fn tui_state_moves_between_product_views() {
    let mut state = TuiState::default();

    state.apply(TuiEvent::NextView);
    state.apply(TuiEvent::NextView);
    assert_eq!(state.view(), TuiView::Config);

    state.apply(TuiEvent::PreviousView);
    assert_eq!(state.view(), TuiView::Providers);
}

#[test]
fn tui_state_records_route_output_without_losing_prompt() {
    let mut state = TuiState::default();
    state.apply(TuiEvent::Insert('x'));
    state.record_status("routed to local");

    assert_eq!(state.prompt(), "x");
    assert!(state.status().contains("routed"));
}
