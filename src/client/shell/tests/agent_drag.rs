use super::*;
use crate::api::schema::{Method, PaneMoveDestination, SplitDirection};

fn agent(pane_id: &str, workspace_id: &str, tab_id: &str, rotulo: &str) -> ClientShellAgent {
    ClientShellAgent {
        pane_id: pane_id.into(),
        workspace_id: workspace_id.into(),
        tab_id: tab_id.into(),
        name: Some(pane_id.into()),
        display_agent: None,
        agent: Some("claude".into()),
        title: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: AgentStatus::Idle,
        state_change_seq: 1,
        state_labels: Vec::new(),
        tokens: vec![("rotulo".into(), rotulo.into())],
        focused: false,
    }
}

/// ws_1 has tab_1 (pane_1, shown) and tab_2 (pane_2); ws_2 has tab_3 (pane_3).
fn state_with_agents() -> ClientShellState {
    let mut projected = snapshot();
    let mut tab_2 = projected.tabs[0].clone();
    tab_2.tab_id = "tab_2".into();
    tab_2.number = 2;
    tab_2.label = "br-0052 update".into();
    tab_2.focused = false;
    projected.tabs.push(tab_2);
    let mut ws_2 = projected.workspaces[0].clone();
    ws_2.workspace_id = "ws_2".into();
    ws_2.active_tab_id = "tab_3".into();
    ws_2.number = 2;
    ws_2.label = "dr".into();
    ws_2.focused = false;
    projected.workspaces.push(ws_2);
    let mut tab_3 = projected.tabs[0].clone();
    tab_3.tab_id = "tab_3".into();
    tab_3.workspace_id = "ws_2".into();
    tab_3.focused = false;
    projected.tabs.push(tab_3);
    for (pane_id, workspace_id, tab_id) in
        [("pane_2", "ws_1", "tab_2"), ("pane_3", "ws_2", "tab_3")]
    {
        let mut pane = projected.panes[0].clone();
        pane.pane_id = pane_id.into();
        pane.workspace_id = workspace_id.into();
        pane.tab_id = tab_id.into();
        pane.focused = false;
        projected.panes.push(pane);
    }
    projected.agents = vec![
        agent("pane_2", "ws_1", "tab_2", "Update diário"),
        agent("pane_3", "ws_2", "tab_3", "Copy da VSL"),
    ];
    let mut config = Config::default();
    config.ui.agent_panel_sort = crate::config::AgentPanelSortConfig::Spaces;
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&config));
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(surface());
    state.compose(106, 30).expect("frame");
    state
}

fn mouse(state: &mut ClientShellState, kind: MouseEventKind, at: (u16, u16)) -> ClientShellInput {
    state.handle_raw_events(vec![RawInputEvent::Mouse(crossterm::event::MouseEvent {
        kind,
        column: at.0,
        row: at.1,
        modifiers: KeyModifiers::empty(),
    })])
}

fn agent_row(state: &ClientShellState, pane_id: &str) -> (u16, u16) {
    let rect = state
        .hits
        .agents
        .iter()
        .find(|(_, id)| id == pane_id)
        .map(|(rect, _)| *rect)
        .expect("agent row");
    (rect.x + 1, rect.y)
}

fn drag(state: &mut ClientShellState, pane_id: &str, to: (u16, u16)) -> Vec<Method> {
    let from = agent_row(state, pane_id);
    let down = mouse(state, MouseEventKind::Down(MouseButton::Left), from);
    assert!(down.actions.is_empty(), "press alone must not focus");
    mouse(state, MouseEventKind::Drag(MouseButton::Left), to);
    state.compose(106, 30).expect("drag preview");
    let up = mouse(state, MouseEventKind::Up(MouseButton::Left), to);
    up.actions
        .into_iter()
        .map(|action| match action {
            ClientShellAction::Endpoint { request, .. } => request.method,
            other => panic!("unexpected action {other:?}"),
        })
        .collect()
}

fn pane_rect(state: &ClientShellState) -> Rect {
    state.hits.panes[0].rect
}

#[test]
fn dropping_an_agent_near_the_right_edge_joins_it_to_the_right() {
    let mut state = state_with_agents();
    let rect = pane_rect(&state);
    let methods = drag(&mut state, "pane_2", (rect.right() - 1, rect.y));
    assert!(
        matches!(
            &methods[..],
            [Method::PaneMove(params)] if params.pane_id == "pane_2"
                && params.focus
                && params.destination == PaneMoveDestination::Tab {
                    tab_id: "tab_1".into(),
                    target_pane_id: Some("pane_1".into()),
                    split: SplitDirection::Right,
                    ratio: None,
                }
        ),
        "{methods:?}"
    );
}

#[test]
fn dropping_near_the_left_edge_splits_then_swaps_so_the_agent_lands_first() {
    let mut state = state_with_agents();
    let rect = pane_rect(&state);
    let methods = drag(&mut state, "pane_2", (rect.x, rect.y));
    assert!(
        matches!(
            &methods[..],
            [Method::PaneMove(moved), Method::PaneSwap(swap)]
                if moved.pane_id == "pane_2"
                    && matches!(&moved.destination, PaneMoveDestination::Tab { split: SplitDirection::Right, .. })
                    && swap.source_pane_id.as_deref() == Some("pane_2")
                    && swap.target_pane_id.as_deref() == Some("pane_1")
        ),
        "{methods:?}"
    );
}

#[test]
fn an_agent_from_another_space_joins_the_pane_on_screen() {
    let mut state = state_with_agents();
    let rect = pane_rect(&state);
    let methods = drag(&mut state, "pane_3", (rect.right() - 1, rect.y));
    assert!(
        matches!(
            &methods[..],
            [Method::PaneMove(params)] if params.pane_id == "pane_3"
                && params.destination == PaneMoveDestination::Tab {
                    tab_id: "tab_1".into(),
                    target_pane_id: Some("pane_1".into()),
                    split: SplitDirection::Right,
                    ratio: None,
                }
        ),
        "{methods:?}"
    );
}

#[test]
fn an_agent_from_another_space_dropped_on_the_tab_bar_gets_a_tab_here() {
    let mut state = state_with_agents();
    let tab = state.hits.tabs[0].0;
    let methods = drag(&mut state, "pane_3", (tab.x + 1, tab.y));
    assert!(
        matches!(
            &methods[..],
            [Method::PaneMove(params)] if params.pane_id == "pane_3"
                && params.destination == PaneMoveDestination::NewTab {
                    workspace_id: Some("ws_1".into()),
                    label: Some("Copy da VSL".into()),
                }
        ),
        "{methods:?}"
    );
}

#[test]
fn drag_preview_marks_the_half_and_follows_the_pointer() {
    let mut state = state_with_agents();
    let rect = pane_rect(&state);
    let from = agent_row(&state, "pane_2");
    mouse(&mut state, MouseEventKind::Down(MouseButton::Left), from);
    mouse(
        &mut state,
        MouseEventKind::Drag(MouseButton::Left),
        (rect.right() - 1, rect.y),
    );
    assert!(matches!(
        state.chrome_drag,
        Some(ClientChromeDrag::Agent {
            target: Some(AgentDropTarget::Pane {
                side: AgentDropSide::Right,
                ..
            }),
            ..
        })
    ));
    let frame = state.compose(106, 30).expect("preview");
    let text = frame
        .cells
        .chunks(frame.width as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("Update diário"),
        "pointer tag uses the rotulo: {text}"
    );
}

#[test]
fn dropping_on_the_tab_bar_detaches_an_agent_that_shares_a_tab() {
    let mut state = state_with_agents();
    let tab = state.hits.tabs[0].0;
    // pane_2 still has its own tab: nothing to detach.
    assert!(drag(&mut state, "pane_2", (tab.x + 1, tab.y)).is_empty());
    let mut projected = state.snapshot.as_deref().unwrap().clone();
    projected
        .panes
        .iter_mut()
        .find(|pane| pane.pane_id == "pane_2")
        .unwrap()
        .tab_id = "tab_1".into();
    projected.tabs.retain(|tab| tab.tab_id != "tab_2");
    projected.revision += 1;
    state.set_snapshot(Box::new(projected));
    let mut surface = surface();
    surface.projection_revision += 1;
    state.set_pane_surface(surface);
    state.compose(106, 30).expect("joined frame");
    let tab = state.hits.tabs[0].0;
    let methods = drag(&mut state, "pane_2", (tab.x + 1, tab.y));
    assert!(
        matches!(
            &methods[..],
            [Method::PaneMove(params)] if params.pane_id == "pane_2"
                && params.destination == PaneMoveDestination::NewTab {
                    workspace_id: Some("ws_1".into()),
                    label: Some("Update diário".into()),
                }
        ),
        "{methods:?}"
    );
}

#[test]
fn agent_row_click_without_movement_still_focuses_on_release() {
    let mut state = state_with_agents();
    let at = agent_row(&state, "pane_2");
    assert!(
        mouse(&mut state, MouseEventKind::Down(MouseButton::Left), at)
            .actions
            .is_empty()
    );
    let up = mouse(&mut state, MouseEventKind::Up(MouseButton::Left), at);
    assert!(matches!(
        &up.actions[..],
        [ClientShellAction::Endpoint { request, .. }]
            if matches!(&request.method, Method::PaneFocus(target) if target.pane_id == "pane_2")
    ));
}

#[test]
fn drop_side_picks_the_nearest_edge_and_swaps_in_the_middle_of_the_same_tab() {
    use crate::client::shell::mouse::agent_drop_side;
    let rect = Rect::new(10, 5, 40, 20);
    assert_eq!(agent_drop_side(rect, (12, 15), false), AgentDropSide::Left);
    assert_eq!(agent_drop_side(rect, (48, 15), false), AgentDropSide::Right);
    assert_eq!(agent_drop_side(rect, (30, 5), false), AgentDropSide::Up);
    assert_eq!(agent_drop_side(rect, (30, 24), false), AgentDropSide::Down);
    assert_eq!(agent_drop_side(rect, (30, 15), true), AgentDropSide::Swap);
    assert_ne!(agent_drop_side(rect, (30, 15), false), AgentDropSide::Swap);
}

/// tab_1 shows pane_1 and pane_4 side by side, each 20x12.
fn state_with_two_panes_side_by_side() -> ClientShellState {
    let mut state = state_with_agents();
    let mut projected = (**state.snapshot.as_ref().expect("snapshot")).clone();
    let mut pane_4 = projected.panes[0].clone();
    pane_4.pane_id = "pane_4".into();
    pane_4.focused = false;
    projected.panes.push(pane_4);
    projected.revision += 1;
    state.set_snapshot(Box::new(projected));
    let mut frame = surface();
    frame.projection_revision += 1;
    frame.frame = FrameData::from_ratatui_buffer_with_hyperlinks(
        &Buffer::empty(Rect::new(0, 0, 40, 12)),
        None,
        &[],
    );
    let mut left = frame.panes[0].clone();
    let rect = SurfaceRect {
        x: 0,
        y: 0,
        width: 20,
        height: 12,
    };
    left.rect = rect;
    left.inner_rect = rect;
    let mut right = left.clone();
    right.pane_id = "pane_4".into();
    right.focused = false;
    right.rect.x = 20;
    right.inner_rect.x = 20;
    frame.panes = vec![left, right];
    state.set_pane_surface(frame);
    state.compose(106, 30).expect("frame");
    state
}

fn tab_area(state: &ClientShellState) -> Rect {
    state
        .hits
        .panes
        .iter()
        .map(|hit| hit.rect)
        .reduce(|a, b| a.union(b))
        .expect("panes")
}

#[test]
fn dropping_on_the_bottom_band_puts_the_agent_below_every_pane_of_the_tab() {
    let mut state = state_with_two_panes_side_by_side();
    let area = tab_area(&state);
    assert_eq!(state.hits.panes.len(), 2);
    let methods = drag(
        &mut state,
        "pane_3",
        (area.x + area.width / 2, area.bottom() - 1),
    );
    assert!(
        matches!(
            &methods[..],
            [Method::PanePlace(params)] if params.pane_id == "pane_3"
                && params.focus
                && params.placement == crate::api::schema::PanePlacement::TabEdge {
                    tab_id: "tab_1".into(),
                    side: crate::api::schema::PaneDirection::Down,
                }
        ),
        "{methods:?}"
    );
}

#[test]
fn above_the_band_the_drop_still_splits_only_the_pane_under_the_pointer() {
    let mut state = state_with_two_panes_side_by_side();
    let left = state.hits.panes[0].rect;
    let methods = drag(
        &mut state,
        "pane_3",
        (left.x + left.width / 2, left.y + left.height * 2 / 3),
    );
    assert!(
        matches!(
            &methods[..],
            [Method::PaneMove(params)] if matches!(
                &params.destination,
                PaneMoveDestination::Tab { split: SplitDirection::Down, target_pane_id: Some(target), .. }
                    if target == "pane_1"
            )
        ),
        "{methods:?}"
    );
}

#[test]
fn edge_preview_names_the_whole_tab() {
    let mut state = state_with_two_panes_side_by_side();
    let area = tab_area(&state);
    let from = agent_row(&state, "pane_3");
    mouse(&mut state, MouseEventKind::Down(MouseButton::Left), from);
    mouse(
        &mut state,
        MouseEventKind::Drag(MouseButton::Left),
        (area.x + area.width / 2, area.bottom() - 1),
    );
    let frame = state.compose(106, 30).expect("preview");
    let text = frame
        .cells
        .chunks(frame.width as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("below all"), "{text}");
}

#[test]
fn a_pane_sharing_its_tab_can_be_detached_home_from_the_right_click_menu() {
    let mut state = state_with_two_panes_side_by_side();
    state.open_pane_context_menu("pane_4".into(), 30, 5);
    let Some(ClientShellOverlay::ContextMenu(menu)) = state.overlay.as_ref() else {
        panic!("menu");
    };
    let index = menu
        .items()
        .iter()
        .position(|item| item.action == ClientContextMenuAction::Detach)
        .expect("Detach item");
    let mut outcome = ClientShellInput::default();
    state.activate_context_menu_item(index, &mut outcome);
    let methods: Vec<Method> = outcome
        .actions
        .into_iter()
        .map(|action| match action {
            ClientShellAction::Endpoint { request, .. } => request.method,
            other => panic!("unexpected action {other:?}"),
        })
        .collect();
    assert!(
        matches!(
            &methods[..],
            [Method::PanePlace(params)] if params.pane_id == "pane_4"
                && !params.focus
                && matches!(params.placement, crate::api::schema::PanePlacement::Home { .. })
        ),
        "{methods:?}"
    );
}

#[test]
fn a_pane_alone_in_its_tab_has_no_detach() {
    let mut state = state_with_agents();
    state.open_pane_context_menu("pane_1".into(), 30, 5);
    let Some(ClientShellOverlay::ContextMenu(menu)) = state.overlay.as_ref() else {
        panic!("menu");
    };
    assert!(menu
        .items()
        .iter()
        .all(|item| item.action != ClientContextMenuAction::Detach));
}
