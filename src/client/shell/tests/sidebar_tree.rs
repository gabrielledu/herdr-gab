use super::*;
use crate::api::schema::Method;
use crate::client::shell::sidebar_tree::{
    agent_elapsed, band_background, blend, client_color, elapsed_label, epoch_now, tree_agent_row,
    tree_rows, SidebarTreeState, TreeCursor, TreeRow,
};
use ratatui::style::Color;

const SPACES: &str = r##"
row_gap = 0
rows = [["state_icon", { token = "workspace", bold = true, rules = [
  { equals = "gabe-os", fg = "#b07d3c" },
  { equals = "a4d", fg = "#b8643a" },
] }, { token = "$resumo", dim = true }]]
"##;

fn agent(pane_id: &str, workspace_id: &str, status: AgentStatus, rotulo: &str) -> ClientShellAgent {
    let cli = match workspace_id {
        "ws_1" => "os",
        "ws_2" => "a4",
        _ => "br",
    };
    let mut tokens = vec![("cli".into(), cli.into())];
    if !rotulo.is_empty() {
        tokens.push(("rotulo".into(), rotulo.into()));
    }
    ClientShellAgent {
        pane_id: pane_id.into(),
        workspace_id: workspace_id.into(),
        tab_id: format!("tab_{workspace_id}"),
        name: None,
        display_agent: None,
        agent: Some("claude".into()),
        title: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: status,
        state_change_seq: 1,
        state_labels: Vec::new(),
        tokens,
        focused: false,
    }
}

/// gabe-os (focused): p_idle1, p_work (working), p_focus (idle, focused),
/// p_block (blocked, needs something), p_done (done), p_idle2.
/// a4d: p_a4d (idle). brez: p_brez (working), p_brez_idle. company: no agents.
fn spaces() -> ClientShellSnapshot {
    let mut projected = snapshot();
    let base = projected.workspaces[0].clone();
    projected.workspaces = [
        ("ws_1", "gabe-os"),
        ("ws_2", "a4d"),
        ("ws_3", "brez"),
        ("ws_4", "company"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (id, label))| {
        let mut workspace = base.clone();
        workspace.workspace_id = id.into();
        workspace.label = label.into();
        workspace.number = index + 1;
        workspace.focused = index == 0;
        workspace.branch = None;
        workspace.tokens = vec![("resumo".into(), format!("{} agentes", index + 1))];
        workspace
    })
    .collect();
    let mut focused = agent("p_focus", "ws_1", AgentStatus::Idle, "Focado");
    focused.focused = true;
    let mut blocked = agent("p_block", "ws_1", AgentStatus::Blocked, "Bloqueado");
    blocked
        .tokens
        .push(("precisa".into(), "aprovar o deck".into()));
    projected.agents = vec![
        agent("p_idle1", "ws_1", AgentStatus::Idle, "Parado um"),
        agent("p_work", "ws_1", AgentStatus::Working, "Trabalhando"),
        focused,
        blocked,
        agent("p_done", "ws_1", AgentStatus::Done, "Pronto"),
        agent("p_idle2", "ws_1", AgentStatus::Idle, "Parado dois"),
        agent("p_a4d", "ws_2", AgentStatus::Idle, "Doc do Ali"),
        agent("p_brez", "ws_3", AgentStatus::Working, "Update"),
        agent("p_brez_idle", "ws_3", AgentStatus::Idle, "Heaven Sent"),
    ];
    let base_pane = projected.panes[0].clone();
    for agent in &projected.agents {
        let mut pane = base_pane.clone();
        pane.pane_id = agent.pane_id.clone();
        pane.workspace_id = agent.workspace_id.clone();
        pane.tab_id = agent.tab_id.clone();
        pane.focused = false;
        projected.panes.push(pane);
    }
    projected
}

fn tree(snapshot: &ClientShellSnapshot) -> SidebarTreeState {
    let mut tree = SidebarTreeState::default();
    tree.reconcile(snapshot);
    tree
}

fn agora(snapshot: &ClientShellSnapshot, tree: &SidebarTreeState) -> Vec<String> {
    tree_rows(snapshot, tree)
        .into_iter()
        .filter_map(|row| match row {
            TreeRow::Agora { pane_id, .. } => Some(pane_id),
            _ => None,
        })
        .collect()
}

fn band(snapshot: &ClientShellSnapshot, tree: &SidebarTreeState, workspace: usize) -> Vec<String> {
    tree_rows(snapshot, tree)
        .into_iter()
        .filter_map(|row| match row {
            TreeRow::Agent {
                workspace_index,
                pane_id,
            } if workspace_index == workspace => Some(pane_id),
            _ => None,
        })
        .collect()
}

fn headers(snapshot: &ClientShellSnapshot, tree: &SidebarTreeState) -> Vec<(String, bool)> {
    tree_rows(snapshot, tree)
        .into_iter()
        .filter_map(|row| match row {
            TreeRow::Header {
                workspace_index,
                expanded,
            } => Some((snapshot.workspaces[workspace_index].label.clone(), expanded)),
            _ => None,
        })
        .collect()
}

fn parked(snapshot: &ClientShellSnapshot, tree: &SidebarTreeState) -> Vec<String> {
    tree_rows(snapshot, tree)
        .into_iter()
        .filter_map(|row| match row {
            TreeRow::Parked { workspace_index } => {
                Some(snapshot.workspaces[workspace_index].label.clone())
            }
            _ => None,
        })
        .collect()
}

fn set_status(snapshot: &mut ClientShellSnapshot, pane_id: &str, status: AgentStatus) {
    snapshot
        .agents
        .iter_mut()
        .find(|agent| agent.pane_id == pane_id)
        .unwrap()
        .agent_status = status;
}

fn focus_workspace(snapshot: &mut ClientShellSnapshot, workspace_id: &str) {
    for workspace in &mut snapshot.workspaces {
        workspace.focused = workspace.workspace_id == workspace_id;
    }
    snapshot.focused_workspace_id = Some(workspace_id.into());
}

fn set_token(snapshot: &mut ClientShellSnapshot, pane_id: &str, key: &str, value: &str) {
    let agent = snapshot
        .agents
        .iter_mut()
        .find(|agent| agent.pane_id == pane_id)
        .unwrap();
    agent.tokens.retain(|(name, _)| name != key);
    agent.tokens.push((key.into(), value.into()));
}

fn pending(snapshot: &ClientShellSnapshot, tree: &SidebarTreeState) -> Vec<String> {
    tree_rows(snapshot, tree)
        .into_iter()
        .filter_map(|row| match row {
            TreeRow::Pending { pane_id, .. } => Some(pane_id),
            _ => None,
        })
        .collect()
}

#[test]
fn agora_lists_blocked_done_working_across_spaces_working_first_without_times() {
    let mut projected = spaces();
    let state = tree(&projected);
    // Working counts as now; without `ult` the rest keeps blocked > done, then
    // sidebar order.
    assert_eq!(
        agora(&projected, &state),
        ["p_work", "p_brez", "p_block", "p_done"]
    );
    set_status(&mut projected, "p_a4d", AgentStatus::Done);
    assert_eq!(
        agora(&projected, &state),
        ["p_work", "p_brez", "p_block", "p_done", "p_a4d"]
    );
    let rows = tree_rows(&projected, &state);
    assert_eq!(rows[0], TreeRow::AgoraTitle);
    assert!(!rows.contains(&TreeRow::AgoraEmpty));

    // Nothing running: one "nada rodando" row once nobody needs anything.
    for agent in &mut projected.agents {
        agent.agent_status = AgentStatus::Idle;
    }
    assert_eq!(agora(&projected, &state), ["p_block"]);
    set_token(&mut projected, "p_block", "precisa", "");
    let rows = tree_rows(&projected, &state);
    assert_eq!(rows[..2], [TreeRow::AgoraTitle, TreeRow::AgoraEmpty]);
}

#[test]
fn agora_is_most_recent_first_by_ult() {
    let mut projected = spaces();
    let state = tree(&projected);
    set_token(&mut projected, "p_block", "ult", "1000");
    set_token(&mut projected, "p_done", "ult", "3000");
    // Agents without `ult` sort after every timed one; working stays on top.
    assert_eq!(
        agora(&projected, &state),
        ["p_work", "p_brez", "p_done", "p_block"]
    );
    set_token(&mut projected, "p_block", "ult", "5000");
    assert_eq!(
        agora(&projected, &state),
        ["p_work", "p_brez", "p_block", "p_done"]
    );
    // A malformed `ult` counts as missing.
    set_token(&mut projected, "p_block", "ult", "ontem");
    assert_eq!(
        agora(&projected, &state),
        ["p_work", "p_brez", "p_done", "p_block"]
    );
}

#[test]
fn fase_bloq_and_precisa_bring_an_idle_agent_into_agora() {
    let mut projected = spaces();
    let mut state = tree(&projected);
    state.set_expanded("ws_2", true);
    set_token(&mut projected, "p_idle1", "fase", "bloq");
    set_token(&mut projected, "p_idle1", "ult", "2000");
    set_token(&mut projected, "p_a4d", "precisa", "login no Meta");
    set_token(&mut projected, "p_a4d", "ult", "1000");
    assert_eq!(
        agora(&projected, &state),
        ["p_work", "p_brez", "p_idle1", "p_a4d", "p_block", "p_done"]
    );
    assert!(!band(&projected, &state, 0).contains(&"p_idle1".to_owned()));
    assert!(band(&projected, &state, 1).is_empty());
    // Other phases leave it in its band.
    for fase in ["esp", "ok", "", "outra"] {
        set_token(&mut projected, "p_idle1", "fase", fase);
        assert!(
            !agora(&projected, &state).contains(&"p_idle1".to_owned()),
            "{fase}"
        );
    }
}

#[test]
fn pending_lists_fase_vez_outside_agora_once_and_hides_when_empty() {
    let mut projected = spaces();
    let mut state = tree(&projected);
    for workspace in ["ws_1", "ws_2", "ws_3"] {
        state.set_expanded(workspace, true);
    }
    assert!(!tree_rows(&projected, &state).contains(&TreeRow::PendingTitle));
    assert!(pending(&projected, &state).is_empty());

    set_token(&mut projected, "p_idle2", "fase", "vez");
    set_token(&mut projected, "p_idle2", "ult", "1000");
    set_token(&mut projected, "p_a4d", "fase", "vez");
    set_token(&mut projected, "p_a4d", "ult", "4000");
    set_token(&mut projected, "p_brez_idle", "fase", "vez");
    // A working agent keeps a stale `fase` and stays in AGORA as running.
    set_token(&mut projected, "p_work", "fase", "vez");
    // A done agent with a step left is still news: AGORA, not PENDENTE.
    set_token(&mut projected, "p_done", "fase", "vez");
    assert_eq!(
        pending(&projected, &state),
        ["p_a4d", "p_idle2", "p_brez_idle"]
    );
    let rows = tree_rows(&projected, &state);
    let title = rows
        .iter()
        .position(|row| *row == TreeRow::PendingTitle)
        .unwrap();
    // PENDENTE sits right after AGORA and before the first band.
    assert!(matches!(rows[title - 1], TreeRow::Agora { .. }));
    assert!(matches!(rows[title + 4], TreeRow::Header { .. }));
    assert!(agora(&projected, &state).contains(&"p_work".to_owned()));
    assert!(agora(&projected, &state).contains(&"p_done".to_owned()));
    // Nobody is listed twice, and the bands keep only the rest.
    let listed = rows
        .iter()
        .filter_map(|row| row.pane_id().map(str::to_owned))
        .collect::<Vec<_>>();
    let mut unique = listed.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), listed.len(), "{listed:?}");
    assert_eq!(listed.len(), projected.agents.len());
    assert_eq!(band(&projected, &state, 0), ["p_idle1", "p_focus"]);
    assert!(band(&projected, &state, 1).is_empty());
    assert!(band(&projected, &state, 2).is_empty());

    // Resolved (fase=ok) everywhere: the section goes away.
    for pane in ["p_idle2", "p_a4d", "p_brez_idle"] {
        set_token(&mut projected, pane, "fase", "ok");
    }
    assert!(!tree_rows(&projected, &state).contains(&TreeRow::PendingTitle));
}

#[test]
fn elapsed_time_reads_agora_minutes_hours_days() {
    let now = 1_800_000_000;
    for (age, label) in [
        (0, "agora"),
        (59, "agora"),
        (60, "1min"),
        (59 * 60 + 59, "59min"),
        (3600, "1h"),
        (23 * 3600 + 3599, "23h"),
        (86400, "1d"),
        (3 * 86400 + 5, "3d"),
    ] {
        assert_eq!(elapsed_label(now - age, now), label, "{age}");
    }
    // A future `ult` (clock skew) reads as now.
    assert_eq!(elapsed_label(now + 90, now), "agora");

    let mut projected = spaces();
    let agent = |projected: &ClientShellSnapshot, pane: &str| {
        projected
            .agents
            .iter()
            .find(|agent| agent.pane_id == pane)
            .unwrap()
            .clone()
    };
    // Working is always "agora"; without `ult` there is no time.
    assert_eq!(
        agent_elapsed(&agent(&projected, "p_work"), now),
        Some(("agora".into(), false))
    );
    assert_eq!(agent_elapsed(&agent(&projected, "p_idle1"), now), None);
    // Peach (old) from 24 hours on.
    set_token(&mut projected, "p_idle1", "ult", &(now - 86399).to_string());
    assert_eq!(
        agent_elapsed(&agent(&projected, "p_idle1"), now),
        Some(("23h".into(), false))
    );
    set_token(&mut projected, "p_idle1", "ult", &(now - 86400).to_string());
    assert_eq!(
        agent_elapsed(&agent(&projected, "p_idle1"), now),
        Some(("1d".into(), true))
    );
    // Milliseconds and fractional seconds are tolerated.
    set_token(
        &mut projected,
        "p_idle1",
        "ult",
        &((now - 300) * 1000).to_string(),
    );
    assert_eq!(
        agent_elapsed(&agent(&projected, "p_idle1"), now),
        Some(("5min".into(), false))
    );
    set_token(
        &mut projected,
        "p_idle1",
        "ult",
        &format!("{}.75", now - 7200),
    );
    assert_eq!(
        agent_elapsed(&agent(&projected, "p_idle1"), now),
        Some(("2h".into(), false))
    );
}

#[test]
fn bands_follow_the_most_recent_activity_of_their_space() {
    let mut projected = spaces();
    let mut state = tree(&projected);
    for workspace in ["ws_1", "ws_2", "ws_3"] {
        state.set_expanded(workspace, true);
    }
    let order = |projected: &ClientShellSnapshot, state: &SidebarTreeState| {
        headers(projected, state)
            .into_iter()
            .map(|(label, _)| label)
            .collect::<Vec<_>>()
    };
    // gabe-os and brez both have a working agent (now); a4d has no time.
    assert_eq!(order(&projected, &state), ["gabe-os", "brez", "a4d"]);
    // Nothing working: the latest `ult` of any agent of the space decides,
    // including agents listed in AGORA.
    set_status(&mut projected, "p_work", AgentStatus::Idle);
    set_status(&mut projected, "p_brez", AgentStatus::Idle);
    set_token(&mut projected, "p_a4d", "ult", "5000");
    set_token(&mut projected, "p_brez", "ult", "4000");
    set_token(&mut projected, "p_block", "ult", "3000");
    assert_eq!(order(&projected, &state), ["a4d", "brez", "gabe-os"]);
    set_token(&mut projected, "p_block", "ult", "6000");
    assert_eq!(order(&projected, &state), ["gabe-os", "a4d", "brez"]);
    // Inside a band: most recent first, untimed last in sidebar order.
    set_token(&mut projected, "p_idle2", "ult", "2000");
    set_token(&mut projected, "p_focus", "ult", "1000");
    assert_eq!(
        band(&projected, &state, 0),
        ["p_idle2", "p_focus", "p_idle1", "p_work"]
    );
    // PARADOS keeps sidebar order at the bottom.
    assert_eq!(parked(&projected, &state), ["company"]);
}

#[test]
fn no_agent_is_listed_twice_and_expanded_bands_hold_only_the_rest() {
    let projected = spaces();
    let mut state = tree(&projected);
    for workspace in ["ws_2", "ws_3"] {
        state.set_expanded(workspace, true);
    }
    let listed = tree_rows(&projected, &state)
        .iter()
        .filter_map(|row| row.pane_id().map(str::to_owned))
        .collect::<Vec<_>>();
    let mut unique = listed.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), listed.len(), "{listed:?}");
    assert_eq!(listed.len(), projected.agents.len());
    assert_eq!(
        band(&projected, &state, 0),
        ["p_idle1", "p_focus", "p_idle2"]
    );
    assert_eq!(band(&projected, &state, 2), ["p_brez_idle"]);
}

#[test]
fn focused_band_starts_expanded_and_manual_toggles_stick() {
    let mut projected = spaces();
    let mut state = tree(&projected);
    assert_eq!(
        headers(&projected, &state),
        [
            ("gabe-os".to_owned(), true),
            ("brez".to_owned(), false),
            ("a4d".to_owned(), false)
        ]
    );
    // Manual choices survive new snapshots and status changes.
    state.toggle(&projected, "ws_1");
    state.toggle(&projected, "ws_2");
    set_status(&mut projected, "p_idle1", AgentStatus::Blocked);
    state.reconcile(&projected);
    assert_eq!(
        headers(&projected, &state),
        [
            ("gabe-os".to_owned(), false),
            ("brez".to_owned(), false),
            ("a4d".to_owned(), true)
        ]
    );
    // Focus moving to a band without a manual choice expands it.
    focus_workspace(&mut projected, "ws_3");
    state.reconcile(&projected);
    assert_eq!(
        headers(&projected, &state),
        [
            ("gabe-os".to_owned(), false),
            ("brez".to_owned(), true),
            ("a4d".to_owned(), true)
        ]
    );
}

#[test]
fn spaces_without_agents_are_parked() {
    let mut projected = spaces();
    let state = tree(&projected);
    assert_eq!(parked(&projected, &state), ["company"]);
    let rows = tree_rows(&projected, &state);
    let title = rows
        .iter()
        .position(|row| *row == TreeRow::ParkedTitle)
        .unwrap();
    assert_eq!(rows[title + 1..], [TreeRow::Parked { workspace_index: 3 }]);
    projected
        .agents
        .retain(|agent| agent.workspace_id != "ws_2");
    assert_eq!(parked(&projected, &state), ["a4d", "company"]);
    assert!(!headers(&projected, &state)
        .iter()
        .any(|(label, _)| label == "a4d"));
}

#[test]
fn empty_label_falls_back_to_title_then_agent_name() {
    let mut projected = spaces();
    projected
        .agents
        .push(agent("p_blank", "ws_1", AgentStatus::Working, ""));
    let mut config = Config::default();
    config.ui.sidebar.layout = crate::config::SidebarLayoutConfig::Tree;
    let config = ClientShellConfig::from_config(&config);
    let kinds = |projected: &ClientShellSnapshot| {
        tree_agent_row(projected, "p_blank", &config).unwrap().rows[0]
            .iter()
            .map(|token| token.kind.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        kinds(&projected),
        [
            crate::ui::ResolvedTokenKind::StateIcon,
            crate::ui::ResolvedTokenKind::Agent("claude".into())
        ]
    );
    projected.agents.last_mut().unwrap().terminal_title_stripped = Some("Claude Code".into());
    assert_eq!(
        kinds(&projected)[1],
        crate::ui::ResolvedTokenKind::TerminalTitle("Claude Code".into())
    );
    // A blank custom value counts as missing and is replaced.
    projected
        .agents
        .last_mut()
        .unwrap()
        .tokens
        .push(("rotulo".into(), " ".into()));
    assert_eq!(
        kinds(&projected),
        [
            crate::ui::ResolvedTokenKind::StateIcon,
            crate::ui::ResolvedTokenKind::TerminalTitle("Claude Code".into())
        ]
    );
}

#[test]
fn band_color_comes_from_the_workspace_rule_and_blends_per_theme() {
    let mut config = Config::default();
    config.ui.sidebar.spaces = toml::from_str(SPACES).unwrap();
    let mut shell = ClientShellConfig::from_config(&config);
    let projected = spaces();
    let gabe_os = client_color(&projected.workspaces[0], &shell);
    assert_eq!(gabe_os, Color::Rgb(0xb0, 0x7d, 0x3c));
    // No matching rule: the accent.
    assert_eq!(
        client_color(&projected.workspaces[2], &shell),
        shell.palette.accent
    );

    // Papel dark and light sidebars, 22% of the client color.
    assert_eq!(
        blend(gabe_os, Color::Rgb(0x14, 0x12, 0x10), 0.22),
        Some(Color::Rgb(54, 42, 26))
    );
    assert_eq!(
        blend(gabe_os, Color::Rgb(0xf3, 0xed, 0xe1), 0.22),
        Some(Color::Rgb(228, 212, 189))
    );
    shell.palette.sidebar_bg = Color::Rgb(0x14, 0x12, 0x10);
    assert_eq!(
        band_background(gabe_os, &shell.palette),
        Color::Rgb(54, 42, 26)
    );
    shell.palette.sidebar_bg = Color::Rgb(0xf3, 0xed, 0xe1);
    assert_eq!(
        band_background(gabe_os, &shell.palette),
        Color::Rgb(228, 212, 189)
    );
    // A terminal-default sidebar falls back to the panel, then to the active row.
    shell.palette.sidebar_bg = Color::Reset;
    shell.palette.panel_bg = Color::Reset;
    assert_eq!(
        band_background(gabe_os, &shell.palette),
        shell.palette.active_row_bg
    );
    assert_eq!(blend(Color::Indexed(3), Color::Rgb(0, 0, 0), 0.5), None);
}

// ---- client state: rendering, clicks, keyboard ----

fn tree_state() -> ClientShellState {
    tree_state_with(spaces())
}

fn tree_state_with(projected: ClientShellSnapshot) -> ClientShellState {
    let mut config = Config::default();
    config.ui.sidebar.layout = crate::config::SidebarLayoutConfig::Tree;
    config.ui.sidebar.spaces = toml::from_str(SPACES).unwrap();
    config.ui.sidebar_width = 36;
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&config));
    state.set_snapshot(Box::new(projected));
    state.set_pane_surface(surface());
    state.compose(120, 40).expect("frame");
    state
}

fn mouse(state: &mut ClientShellState, kind: MouseEventKind, at: (u16, u16)) -> ClientShellInput {
    state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind,
        column: at.0,
        row: at.1,
        modifiers: KeyModifiers::empty(),
    })])
}

fn methods(outcome: ClientShellInput) -> Vec<Method> {
    outcome
        .actions
        .into_iter()
        .filter_map(|action| match action {
            ClientShellAction::Endpoint { request, .. } => Some(request.method),
            _ => None,
        })
        .collect()
}

fn click(state: &mut ClientShellState, at: (u16, u16)) -> Vec<Method> {
    let mut sent = methods(mouse(state, MouseEventKind::Down(MouseButton::Left), at));
    sent.extend(methods(mouse(
        state,
        MouseEventKind::Up(MouseButton::Left),
        at,
    )));
    state.compose(120, 40).expect("frame");
    sent
}

fn workspace_row(state: &ClientShellState, workspace_id: &str) -> (u16, u16) {
    let rect = state
        .hits
        .workspaces
        .iter()
        .find(|hit| hit.workspace_id == workspace_id)
        .expect("workspace row")
        .rect;
    (rect.x + 4, rect.y)
}

fn agent_row(state: &ClientShellState, pane_id: &str) -> Option<(u16, u16)> {
    state
        .hits
        .agents
        .iter()
        .find(|(_, id)| id == pane_id)
        .map(|(rect, _)| (rect.x + 4, rect.y))
}

fn text(state: &mut ClientShellState) -> String {
    let frame = state.compose(120, 40).unwrap();
    frame_rows(&frame)
        .iter()
        .map(|row| row.chars().take(36).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn renders_agora_bands_and_parked_sections() {
    let mut state = tree_state();
    let text = text(&mut state);
    for expected in [
        " AGORA",
        "os Bloqueado",
        "    aprovar o deck",
        "os Trabalhando",
        "br Update",
        " ▾ gabe-os",
        "1 agentes",
        " ┃ ",
        "Parado um",
        " ▸ a4d",
        " ▸ brez",
        " PARADOS",
        " ▸ company",
        " new",
    ] {
        assert!(text.contains(expected), "missing {expected:?}\n{text}");
    }
    for unexpected in [" spaces", " agents", "Heaven Sent", "Doc do Ali"] {
        assert!(
            !text.contains(unexpected),
            "unexpected {unexpected:?}\n{text}"
        );
    }
    let buffer = state.compose(120, 40).unwrap().to_ratatui_buffer().unwrap();
    // The band header carries the blended client color.
    let (x, y) = workspace_row(&state, "ws_1");
    assert_eq!(
        buffer[(x, y)].bg,
        band_background(Color::Rgb(0xb0, 0x7d, 0x3c), &state.config.palette)
    );
    // The focused agent is highlighted where it is listed (its band here).
    let (x, y) = agent_row(&state, "p_focus").unwrap();
    assert_eq!(buffer[(x, y)].bg, state.config.palette.active_row_bg);
    // Sections in visual order: AGORA rows, bands, PARADOS.
    // (The client shows a done agent as done only after it saw it complete, so the
    // fixture's p_done reads as idle here and sits in its band.)
    let ys = ["p_work", "p_brez", "p_block", "p_idle1", "p_done"]
        .map(|pane| agent_row(&state, pane).unwrap().1);
    assert!(ys.windows(2).all(|pair| pair[0] < pair[1]), "{ys:?}");
    assert!(workspace_row(&state, "ws_3").1 < workspace_row(&state, "ws_4").1);
}

#[test]
fn clicks_map_each_section_to_its_action() {
    let mut state = tree_state();
    // AGORA row: focus that agent on release.
    let at = agent_row(&state, "p_brez").unwrap();
    assert!(
        matches!(&click(&mut state, at)[..], [Method::PaneFocus(target)] if target.pane_id == "p_brez")
    );
    // Band header: expands or collapses, without switching spaces.
    assert!(agent_row(&state, "p_a4d").is_none());
    let at = workspace_row(&state, "ws_2");
    assert!(click(&mut state, at).is_empty());
    assert!(agent_row(&state, "p_a4d").is_some());
    assert!(text(&mut state).contains(" ▾ a4d"));
    // Band agent row: focus.
    let at = agent_row(&state, "p_a4d").unwrap();
    assert!(
        matches!(&click(&mut state, at)[..], [Method::PaneFocus(target)] if target.pane_id == "p_a4d")
    );
    // Collapse it again; the choice sticks.
    let at = workspace_row(&state, "ws_2");
    assert!(click(&mut state, at).is_empty());
    assert!(agent_row(&state, "p_a4d").is_none());
    // PARADOS row: switches to that space.
    let at = workspace_row(&state, "ws_4");
    assert!(
        matches!(&click(&mut state, at)[..], [Method::WorkspaceFocus(target)] if target.workspace_id == "ws_4"),
    );
}

#[test]
fn dragging_an_agora_agent_into_the_pane_moves_it() {
    let mut state = tree_state();
    let from = agent_row(&state, "p_brez").unwrap();
    let pane = state.hits.panes[0].rect;
    let to = (pane.right() - 1, pane.y);
    assert!(
        mouse(&mut state, MouseEventKind::Down(MouseButton::Left), from)
            .actions
            .is_empty()
    );
    mouse(&mut state, MouseEventKind::Drag(MouseButton::Left), to);
    state.compose(120, 40).unwrap();
    let sent = methods(mouse(&mut state, MouseEventKind::Up(MouseButton::Left), to));
    assert!(
        matches!(&sent[..], [Method::PaneMove(params)] if params.pane_id == "p_brez"),
        "{sent:?}"
    );
}

fn key(state: &mut ClientShellState, bytes: &[u8]) -> ClientShellInput {
    let outcome = state.handle_input_bytes(bytes);
    state.compose(120, 40).unwrap();
    outcome
}

fn cursor(state: &ClientShellState) -> (String, Option<TreeCursor>) {
    (
        state
            .navigate_workspace_id
            .as_ref()
            .unwrap()
            .workspace_id
            .clone(),
        state.navigate_tree_item.clone(),
    )
}

fn on(workspace: &str, pane: Option<&str>) -> (String, Option<TreeCursor>) {
    (
        workspace.to_owned(),
        pane.map(|pane| TreeCursor::Agent(pane.to_owned())),
    )
}

#[test]
fn navigate_mode_walks_agora_bands_and_parked_rows_in_visual_order() {
    let mut state = tree_state();
    key(&mut state, &[0x02]);
    key(&mut state, b"w");
    assert_eq!(state.mode, ClientShellMode::Navigate);
    assert_eq!(cursor(&state), on("ws_1", None));
    let expected = [
        on("ws_1", Some("p_idle1")),
        on("ws_1", Some("p_focus")),
        on("ws_1", Some("p_done")),
        on("ws_1", Some("p_idle2")),
        on("ws_3", None),
        on("ws_2", None),
        on("ws_4", None),
        on("ws_1", Some("p_work")),
        on("ws_3", Some("p_brez")),
        on("ws_1", Some("p_block")),
        on("ws_1", None),
    ];
    for step in expected {
        key(&mut state, b"\x1b[B");
        assert_eq!(cursor(&state), step);
    }

    // Space on a header toggles its band; Left from a band row climbs to the header.
    for _ in 0..6 {
        key(&mut state, b"\x1b[B");
    }
    assert_eq!(cursor(&state), on("ws_2", None));
    key(&mut state, b" ");
    assert!(agent_row(&state, "p_a4d").is_some());
    key(&mut state, b"\x1b[B");
    assert_eq!(cursor(&state), on("ws_2", Some("p_a4d")));
    key(&mut state, b"\x1b[D");
    assert_eq!(cursor(&state), on("ws_2", None));
    key(&mut state, b"\x1b[D");
    assert!(agent_row(&state, "p_a4d").is_none());
    key(&mut state, b"\x1b[C");
    assert!(agent_row(&state, "p_a4d").is_some());
    assert_eq!(state.mode, ClientShellMode::Navigate);

    // Enter on an agent row focuses it.
    key(&mut state, b"\x1b[B");
    assert_eq!(cursor(&state), on("ws_2", Some("p_a4d")));
    let sent = methods(key(&mut state, b"\r"));
    assert!(
        matches!(&sent[..], [Method::PaneFocus(target)] if target.pane_id == "p_a4d"),
        "{sent:?}"
    );
    assert_eq!(state.mode, ClientShellMode::Terminal);
}

#[test]
fn split_layout_is_unchanged_by_default() {
    let mut config = Config::default();
    config.ui.sidebar_width = 36;
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&config));
    state.set_snapshot(Box::new(spaces()));
    state.set_pane_surface(surface());
    let text = text(&mut state);
    assert!(text.contains(" spaces"), "{text}");
    assert!(text.contains(" agents"), "{text}");
    assert!(!text.contains("AGORA"), "{text}");
    assert!(!text.contains("PARADOS"), "{text}");
}

/// gabe-os: p_idle1 waits for an OK (fase=bloq, 3h), p_idle2 waits on someone
/// else (fase=esp, 2h). a4d: p_a4d left a step (fase=vez, 5min). brez:
/// p_brez_idle left a step two days ago.
fn with_phases() -> ClientShellSnapshot {
    let mut projected = spaces();
    let now = epoch_now();
    for (pane, fase, passo, age) in [
        ("p_idle1", "bloq", "OK p/ criar TST23", 3 * 3600),
        ("p_idle2", "esp", "CI do PR", 2 * 3600),
        ("p_a4d", "vez", "ouvir 2 áudios", 300),
        ("p_brez_idle", "vez", "enviar 7 rascunhos", 2 * 86400),
    ] {
        set_token(&mut projected, pane, "fase", fase);
        set_token(&mut projected, pane, "passo", passo);
        set_token(&mut projected, pane, "ult", &(now - age).to_string());
    }
    projected
}

fn line_with<'a>(text: &'a str, needle: &str) -> &'a str {
    text.lines()
        .find(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("missing {needle:?}\n{text}"))
}

/// Buffer position of the last occurrence of `needle` on the line holding `anchor`.
fn cell_of(state: &mut ClientShellState, anchor: &str, needle: &str) -> (u16, u16) {
    let text = text(state);
    let y = text.lines().position(|line| line.contains(anchor)).unwrap();
    let line = text.lines().nth(y).unwrap();
    let byte = line.rfind(needle).unwrap();
    (line[..byte].chars().count() as u16, y as u16)
}

#[test]
fn renders_pending_section_elapsed_times_and_second_lines() {
    let mut state = tree_state_with(with_phases());
    let text = text(&mut state);
    let ends = |needle: &str, end: &str| {
        let line = line_with(&text, needle);
        let line = line.trim_end_matches([' ', '│']);
        assert!(
            line.ends_with(end),
            "{line:?} should end with {end:?}\n{text}"
        );
    };
    // AGORA: count on the title, working first ("agora"), then the blocked phase
    // with its step on a second line.
    ends(" AGORA", "4");
    ends("os Trabalhando", "agora");
    ends("os Parado um", "3h");
    assert!(text.contains("\n    OK p/ criar TST23"), "{text}");
    // PENDENTE: the step in place of the label, most recent first.
    ends(" PENDENTE COM VOCÊ", "2");
    ends(" ➜ a4 ouvir 2 áudios", "5min");
    ends(" ➜ br enviar 7 rascunhos", "2d");
    let pending_y = |needle: &str| text.lines().position(|line| line.contains(needle)).unwrap();
    assert!(pending_y("ouvir 2 áudios") < pending_y("enviar 7 rascunhos"));
    assert!(pending_y("Parado um") < pending_y("PENDENTE"));
    assert!(pending_y("enviar 7 rascunhos") < pending_y("▾ gabe-os"));
    // Band: what a waiting agent waits on, and its time.
    ends("Parado dois · CI do PR", "2h");
    // Nobody twice: the pending agents are not in their bands' text.
    assert_eq!(text.matches("Doc do Ali").count(), 0, "{text}");
    assert_eq!(text.matches("Heaven Sent").count(), 0, "{text}");

    let palette = state.config.palette.clone();
    let buffer = state.compose(120, 40).unwrap().to_ratatui_buffer().unwrap();
    let at = cell_of(&mut state, "enviar 7 rascunhos", "2d");
    assert_eq!(buffer[at].fg, palette.peach, "older than 24h");
    let at = cell_of(&mut state, "ouvir 2 áudios", "5min");
    assert_eq!(buffer[at].fg, palette.overlay0);
    let at = cell_of(&mut state, "ouvir 2 áudios", "➜");
    assert_eq!(buffer[at].fg, palette.peach);
    let at = cell_of(&mut state, "OK p/ criar TST23", "OK");
    assert_eq!(buffer[at].fg, palette.red);
    let at = cell_of(&mut state, "CI do PR", "CI");
    assert_eq!(buffer[at].fg, palette.blue);
    // An idle agent waiting for the user reads as blocked.
    let at = cell_of(&mut state, "os Parado um", "os Parado um");
    assert_eq!(buffer[(at.0 - 2, at.1)].fg, palette.red);
}

#[test]
fn a_vez_step_shows_in_peach_under_an_agora_row() {
    let mut projected = with_phases();
    set_status(&mut projected, "p_a4d", AgentStatus::Blocked);
    let mut state = tree_state_with(projected);
    let text = text(&mut state);
    // Blocked: AGORA, with the step below, and out of PENDENTE.
    assert!(text.contains("\n    ouvir 2 áudios"), "{text}");
    assert!(!text.contains("➜ a4"), "{text}");
    let palette = state.config.palette.clone();
    let buffer = state.compose(120, 40).unwrap().to_ratatui_buffer().unwrap();
    let at = cell_of(&mut state, "    ouvir 2 áudios", "ouvir");
    assert_eq!(buffer[at].fg, palette.peach);
}

#[test]
fn pending_rows_focus_drag_and_navigate_like_agora_rows() {
    let mut state = tree_state_with(with_phases());
    let at = agent_row(&state, "p_a4d").expect("pending row is a hit");
    assert!(
        matches!(&click(&mut state, at)[..], [Method::PaneFocus(target)] if target.pane_id == "p_a4d")
    );
    // Drag into the pane.
    let from = agent_row(&state, "p_brez_idle").unwrap();
    let pane = state.hits.panes[0].rect;
    let to = (pane.right() - 1, pane.y);
    mouse(&mut state, MouseEventKind::Down(MouseButton::Left), from);
    mouse(&mut state, MouseEventKind::Drag(MouseButton::Left), to);
    state.compose(120, 40).unwrap();
    let sent = methods(mouse(&mut state, MouseEventKind::Up(MouseButton::Left), to));
    assert!(
        matches!(&sent[..], [Method::PaneMove(params)] if params.pane_id == "p_brez_idle"),
        "{sent:?}"
    );

    // Navigate mode walks through the pending rows; Enter focuses.
    let mut state = tree_state_with(with_phases());
    key(&mut state, &[0x02]);
    key(&mut state, b"w");
    let mut seen = Vec::new();
    for _ in 0..20 {
        key(&mut state, b"\x1b[B");
        seen.push(cursor(&state));
        if cursor(&state) == on("ws_2", Some("p_a4d")) {
            break;
        }
    }
    assert_eq!(cursor(&state), on("ws_2", Some("p_a4d")), "{seen:?}");
    let next = {
        key(&mut state, b"\x1b[B");
        cursor(&state)
    };
    assert_eq!(next, on("ws_3", Some("p_brez_idle")));
    key(&mut state, b"\x1b[A");
    assert_eq!(cursor(&state), on("ws_2", Some("p_a4d")));
    let sent = methods(key(&mut state, b"\r"));
    assert!(
        matches!(&sent[..], [Method::PaneFocus(target)] if target.pane_id == "p_a4d"),
        "{sent:?}"
    );
}

#[test]
fn tree_layout_repaints_once_a_minute() {
    let mut state = tree_state();
    assert!(!state.tick_sidebar_clock_at(600));
    assert!(!state.tick_sidebar_clock_at(659));
    assert!(state.tick_sidebar_clock_at(660));
    assert!(!state.tick_sidebar_clock_at(719));
    assert!(state.tick_sidebar_clock_at(725));

    let mut config = Config::default();
    config.ui.sidebar_width = 36;
    let mut split = ClientShellState::new(ClientShellConfig::from_config(&config));
    split.set_snapshot(Box::new(spaces()));
    assert!(!split.tick_sidebar_clock_at(600));
    assert!(!split.tick_sidebar_clock_at(660));
}
