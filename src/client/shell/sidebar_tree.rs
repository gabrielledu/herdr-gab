//! Integrated layout of the expanded desktop sidebar (`ui.sidebar.layout =
//! "tree"`, alias `"bands"`). One scrollable panel replaces the split
//! spaces/agents panels:
//!
//! * `AGORA`: every blocked, done (unseen) or working agent of every space,
//!   blocked > done > working, then sidebar order. Each row shows the state
//!   icon, the client code (`$cli`) in the client color, the label and, for a
//!   blocked agent, what it needs (`$precisa`) in red.
//! * One band per space that has agents, in sidebar order: a header tinted
//!   with the client color, `▸/▾ name` and the space summary (`$resumo`).
//!   Expanding a band lists, behind a `┃` rail, the agents of that space that
//!   are not in AGORA, so no agent appears twice. Bands start collapsed except
//!   the one holding the focused pane; a manual toggle sticks for the session.
//! * `PARADOS`: spaces without agents, one compact row each.
//!
//! All of this is client presentation state.

use ratatui::style::Color;
use unicode_width::UnicodeWidthStr as _;

use super::agent_sidebar::AgentRow;
use super::*;

/// Share of the client color in a band header background.
pub(super) const BAND_TINT: f32 = 0.22;
const RAIL: &str = "┃";
const AGORA_TITLE: &str = " AGORA";
const AGORA_EMPTY: &str = " nada rodando";
const PARKED_TITLE: &str = " PARADOS";

/// The navigate cursor on an agent row (AGORA or band), beyond the space's
/// band header or PARADOS row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TreeCursor {
    Agent(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TreeRow {
    AgoraTitle,
    /// "nada rodando" when no agent is blocked, done or working.
    AgoraEmpty,
    /// A blocked, done or working agent.
    Agora {
        workspace_index: usize,
        pane_id: String,
    },
    /// Header of the band of a space that has agents.
    Header {
        workspace_index: usize,
        expanded: bool,
    },
    /// An idle agent listed in an expanded band.
    Agent {
        workspace_index: usize,
        pane_id: String,
    },
    ParkedTitle,
    /// A space without agents.
    Parked {
        workspace_index: usize,
    },
}

impl TreeRow {
    pub(super) fn workspace_index(&self) -> Option<usize> {
        match self {
            Self::Agora {
                workspace_index, ..
            }
            | Self::Header {
                workspace_index, ..
            }
            | Self::Agent {
                workspace_index, ..
            }
            | Self::Parked { workspace_index } => Some(*workspace_index),
            Self::AgoraTitle | Self::AgoraEmpty | Self::ParkedTitle => None,
        }
    }

    pub(super) fn pane_id(&self) -> Option<&str> {
        match self {
            Self::Agora { pane_id, .. } | Self::Agent { pane_id, .. } => Some(pane_id),
            _ => None,
        }
    }

    fn cursor(&self) -> Option<TreeCursor> {
        self.pane_id()
            .map(|pane_id| TreeCursor::Agent(pane_id.to_owned()))
    }

    fn navigable(&self) -> bool {
        self.workspace_index().is_some()
    }
}

/// Per-client band state: manual expand/collapse choices by workspace id.
#[derive(Debug, Default)]
pub(crate) struct SidebarTreeState {
    manual: HashMap<String, bool>,
}

/// Blocked, done and working agents go to AGORA.
pub(super) fn in_agora(status: crate::api::schema::AgentStatus) -> bool {
    use crate::api::schema::AgentStatus;
    matches!(
        status,
        AgentStatus::Blocked | AgentStatus::Done | AgentStatus::Working
    )
}

/// AGORA order: blocked, done, working.
fn agora_rank(status: crate::api::schema::AgentStatus) -> u8 {
    use crate::api::schema::AgentStatus;
    match status {
        AgentStatus::Blocked => 0,
        AgentStatus::Done => 1,
        AgentStatus::Working => 2,
        AgentStatus::Idle | AgentStatus::Unknown => 3,
    }
}

impl SidebarTreeState {
    /// Drops choices for spaces that no longer exist.
    pub(super) fn reconcile(&mut self, snapshot: &ClientShellSnapshot) {
        self.manual.retain(|workspace_id, _| {
            snapshot
                .workspaces
                .iter()
                .any(|workspace| &workspace.workspace_id == workspace_id)
        });
    }

    /// A band is expanded when toggled open by hand, or, without a manual
    /// choice, while its space holds the focused pane.
    pub(super) fn is_expanded(&self, snapshot: &ClientShellSnapshot, workspace_id: &str) -> bool {
        if let Some(expanded) = self.manual.get(workspace_id) {
            return *expanded;
        }
        snapshot.workspaces.iter().any(|workspace| {
            workspace.workspace_id == workspace_id
                && (workspace.focused
                    || snapshot.focused_workspace_id.as_deref() == Some(workspace_id))
        })
    }

    pub(super) fn set_expanded(&mut self, workspace_id: &str, expanded: bool) {
        self.manual.insert(workspace_id.to_owned(), expanded);
    }

    pub(super) fn toggle(&mut self, snapshot: &ClientShellSnapshot, workspace_id: &str) {
        let expanded = !self.is_expanded(snapshot, workspace_id);
        self.set_expanded(workspace_id, expanded);
    }
}

/// Visible rows in visual order: AGORA, the bands, PARADOS.
pub(super) fn tree_rows(snapshot: &ClientShellSnapshot, tree: &SidebarTreeState) -> Vec<TreeRow> {
    let index_of = |workspace_id: &str| {
        snapshot
            .workspaces
            .iter()
            .position(|workspace| workspace.workspace_id == workspace_id)
    };
    let mut agora = snapshot
        .agents
        .iter()
        .filter(|agent| in_agora(agent.agent_status))
        .filter_map(|agent| Some((index_of(&agent.workspace_id)?, agent)))
        .collect::<Vec<_>>();
    agora
        .sort_by_key(|(workspace_index, agent)| (agora_rank(agent.agent_status), *workspace_index));
    let mut rows = vec![TreeRow::AgoraTitle];
    if agora.is_empty() {
        rows.push(TreeRow::AgoraEmpty);
    }
    rows.extend(
        agora
            .into_iter()
            .map(|(workspace_index, agent)| TreeRow::Agora {
                workspace_index,
                pane_id: agent.pane_id.clone(),
            }),
    );
    let mut parked = Vec::new();
    for (workspace_index, workspace) in snapshot.workspaces.iter().enumerate() {
        let agents = snapshot
            .agents
            .iter()
            .filter(|agent| agent.workspace_id == workspace.workspace_id)
            .collect::<Vec<_>>();
        if agents.is_empty() {
            parked.push(TreeRow::Parked { workspace_index });
            continue;
        }
        let expanded = tree.is_expanded(snapshot, &workspace.workspace_id);
        rows.push(TreeRow::Header {
            workspace_index,
            expanded,
        });
        if expanded {
            rows.extend(
                agents
                    .into_iter()
                    .filter(|agent| !in_agora(agent.agent_status))
                    .map(|agent| TreeRow::Agent {
                        workspace_index,
                        pane_id: agent.pane_id.clone(),
                    }),
            );
        }
    }
    if !parked.is_empty() {
        rows.push(TreeRow::ParkedTitle);
        rows.extend(parked);
    }
    rows
}

/// Blends `over` onto `under` (both RGB); `alpha` is the share of `over`.
pub(super) fn blend(over: Color, under: Color, alpha: f32) -> Option<Color> {
    let (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) = (over, under) else {
        return None;
    };
    let mix = |top: u8, bottom: u8| {
        (f32::from(top) * alpha + f32::from(bottom) * (1.0 - alpha))
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Some(Color::Rgb(mix(r1, r2), mix(g1, g2), mix(b1, b2)))
}

/// The space's client color: the `fg` its `workspace` token resolves to in
/// `ui.sidebar.spaces.rows` (first matching rule), or the accent.
pub(super) fn client_color(workspace: &ClientShellWorkspace, config: &ClientShellConfig) -> Color {
    super::sidebar::workspace_rows(workspace, workspace.agent_status, false, &config.spaces)
        .iter()
        .flatten()
        .find_map(|token| match token.kind {
            crate::ui::ResolvedTokenKind::Workspace(_) => token.style.fg.map(|fg| fg.ratatui()),
            _ => None,
        })
        .unwrap_or(config.palette.accent)
}

/// Band header background: the client color blended over the sidebar.
pub(super) fn band_background(client: Color, palette: &Palette) -> Color {
    blend(client, palette.sidebar_bg, BAND_TINT)
        .or_else(|| blend(client, palette.panel_bg, BAND_TINT))
        .unwrap_or(palette.active_row_bg)
}

/// The header's right-hand text: the first spaces row without its state icon
/// and workspace name (with the usual config, `$resumo`).
fn header_summary(workspace: &ClientShellWorkspace, config: &ClientShellConfig) -> String {
    super::sidebar::workspace_rows(workspace, workspace.agent_status, false, &config.spaces)
        .into_iter()
        .next()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|token| match token.kind {
            crate::ui::ResolvedTokenKind::StateIcon
            | crate::ui::ResolvedTokenKind::Workspace(_)
            | crate::ui::ResolvedTokenKind::GitStatus { .. } => None,
            crate::ui::ResolvedTokenKind::StateText(text)
            | crate::ui::ResolvedTokenKind::Machine(text)
            | crate::ui::ResolvedTokenKind::Tab(text)
            | crate::ui::ResolvedTokenKind::Pane(text)
            | crate::ui::ResolvedTokenKind::Agent(text)
            | crate::ui::ResolvedTokenKind::TerminalTitle(text)
            | crate::ui::ResolvedTokenKind::Branch(text)
            | crate::ui::ResolvedTokenKind::Custom(text) => {
                Some(text).filter(|text| !text.trim().is_empty())
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

fn blank_token(token: &crate::ui::ResolvedToken) -> bool {
    use crate::ui::ResolvedTokenKind;
    match &token.kind {
        ResolvedTokenKind::StateIcon => true,
        ResolvedTokenKind::StateText(text)
        | ResolvedTokenKind::Machine(text)
        | ResolvedTokenKind::Workspace(text)
        | ResolvedTokenKind::Tab(text)
        | ResolvedTokenKind::Pane(text)
        | ResolvedTokenKind::Agent(text)
        | ResolvedTokenKind::TerminalTitle(text)
        | ResolvedTokenKind::Branch(text)
        | ResolvedTokenKind::Custom(text) => text.trim().is_empty(),
        ResolvedTokenKind::GitStatus { .. } => false,
    }
}

/// An agent's rows (`ui.sidebar.tree.agent_rows`), with a readable fallback
/// when the configured label is missing: the stripped terminal title, then
/// the agent name, then the pane id.
pub(super) fn tree_agent_row(
    snapshot: &ClientShellSnapshot,
    pane_id: &str,
    config: &ClientShellConfig,
) -> Option<AgentRow> {
    let mut row =
        super::agent_sidebar::agent_row_with(snapshot, pane_id, &config.tree_agents, None)?;
    if row.rows.is_empty() {
        row.rows.push(vec![crate::ui::ResolvedToken {
            kind: crate::ui::ResolvedTokenKind::StateIcon,
            style: Default::default(),
        }]);
    }
    if row.rows[0].iter().all(blank_token) {
        let agent = snapshot
            .agents
            .iter()
            .find(|agent| agent.pane_id == pane_id)?;
        let present = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        let kind = present(&agent.terminal_title_stripped)
            .map(crate::ui::ResolvedTokenKind::TerminalTitle)
            .or_else(|| {
                present(&agent.display_agent)
                    .or_else(|| present(&agent.name))
                    .or_else(|| present(&agent.agent))
                    .map(crate::ui::ResolvedTokenKind::Agent)
            })
            .unwrap_or_else(|| crate::ui::ResolvedTokenKind::Agent(pane_id.to_owned()));
        let first = &mut row.rows[0];
        first.retain(|token| matches!(token.kind, crate::ui::ResolvedTokenKind::StateIcon));
        first.push(crate::ui::ResolvedToken {
            kind,
            style: Default::default(),
        });
    }
    Some(row)
}

fn agent_token<'a>(
    snapshot: &'a ClientShellSnapshot,
    pane_id: &str,
    name: &str,
) -> Option<&'a str> {
    snapshot
        .agents
        .iter()
        .find(|agent| agent.pane_id == pane_id)?
        .tokens
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
}

enum RenderedRow {
    AgoraTitle,
    AgoraEmpty,
    Agora {
        client: Color,
        row: AgentRow,
        cli: Option<String>,
        precisa: Option<String>,
    },
    Header {
        workspace_index: usize,
        client: Color,
        expanded: bool,
        summary: String,
    },
    Agent {
        client: Color,
        row: AgentRow,
    },
    ParkedTitle,
    Parked {
        workspace_index: usize,
        client: Color,
    },
}

impl RenderedRow {
    fn height(&self) -> u16 {
        match self {
            Self::Agent { row, .. } => row.rows.len().clamp(1, u16::MAX as usize) as u16,
            _ => 1,
        }
    }
}

/// Blank rows before the bands and before PARADOS (section breaks), and between
/// bands, as far as they fit; PARADOS then sinks to the bottom of the panel.
fn row_gaps(rows: &[RenderedRow], heights: &[u16], body_height: u16) -> Vec<u16> {
    let first_header = rows
        .iter()
        .position(|row| matches!(row, RenderedRow::Header { .. }));
    let section_break = |index: usize| {
        index > 0
            && (Some(index) == first_header || matches!(rows[index], RenderedRow::ParkedTitle))
    };
    let band_break = |index: usize| {
        Some(index) != first_header && matches!(rows[index], RenderedRow::Header { .. })
    };
    let content = heights
        .iter()
        .map(|height| usize::from(*height))
        .sum::<usize>();
    let sections = (0..rows.len())
        .filter(|index| section_break(*index))
        .count();
    let bands = (0..rows.len()).filter(|index| band_break(*index)).count();
    let available = usize::from(body_height);
    let (use_sections, use_bands) = if content + sections + bands <= available {
        (true, true)
    } else if content + sections <= available {
        (true, false)
    } else {
        (false, false)
    };
    let mut gaps = (0..rows.len())
        .map(|index| {
            let next = index + 1;
            u16::from(
                next < rows.len()
                    && ((use_sections && section_break(next)) || (use_bands && band_break(next))),
            )
        })
        .collect::<Vec<_>>();
    let used = content + gaps.iter().map(|gap| usize::from(*gap)).sum::<usize>();
    if let Some(title) = rows
        .iter()
        .position(|row| matches!(row, RenderedRow::ParkedTitle))
        .filter(|title| *title > 0 && used < available)
    {
        gaps[title - 1] = gaps[title - 1].saturating_add((available - used) as u16);
    }
    gaps
}

pub(super) fn render_sidebar_tree(
    buffer: &mut Buffer,
    area: Rect,
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    state: &mut render::ShellRenderState<'_>,
    hits: &mut ShellHitMap,
) {
    let palette = &config.palette;
    render::render_sidebar_background(buffer, area, palette);
    hits.sidebar_divider = if area.is_empty() {
        Rect::default()
    } else {
        Rect::new(area.right().saturating_sub(1), area.y, 1, area.height)
    };
    hits.sidebar_section_divider = Rect::default();
    let content = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
    if content.is_empty() {
        return;
    }
    let body = Rect::new(
        content.x,
        content.y,
        content.width,
        content.height.saturating_sub(1),
    );
    hits.workspace_body = body;

    let tree_rows = tree_rows(snapshot, state.sidebar_tree);
    let rows = tree_rows
        .iter()
        .filter_map(|row| {
            let workspace = row
                .workspace_index()
                .and_then(|index| snapshot.workspaces.get(index));
            let client = workspace.map(|workspace| client_color(workspace, config));
            Some(match row {
                TreeRow::AgoraTitle => RenderedRow::AgoraTitle,
                TreeRow::AgoraEmpty => RenderedRow::AgoraEmpty,
                TreeRow::Agora { pane_id, .. } => {
                    let row = tree_agent_row(snapshot, pane_id, config)?;
                    let precisa = (row.status == crate::api::schema::AgentStatus::Blocked)
                        .then(|| agent_token(snapshot, pane_id, "precisa"))
                        .flatten()
                        .map(str::to_owned);
                    RenderedRow::Agora {
                        client: client?,
                        cli: agent_token(snapshot, pane_id, "cli").map(str::to_owned),
                        precisa,
                        row,
                    }
                }
                TreeRow::Header {
                    workspace_index,
                    expanded,
                } => RenderedRow::Header {
                    workspace_index: *workspace_index,
                    client: client?,
                    expanded: *expanded,
                    summary: header_summary(workspace?, config),
                },
                TreeRow::Agent { pane_id, .. } => RenderedRow::Agent {
                    client: client?,
                    row: tree_agent_row(snapshot, pane_id, config)?,
                },
                TreeRow::ParkedTitle => RenderedRow::ParkedTitle,
                TreeRow::Parked { workspace_index } => RenderedRow::Parked {
                    workspace_index: *workspace_index,
                    client: client?,
                },
            })
        })
        .collect::<Vec<_>>();
    let heights = rows.iter().map(RenderedRow::height).collect::<Vec<_>>();
    let gaps = row_gaps(&rows, &heights, body.height);

    let navigating = state.selected_workspace_id.is_some();
    let selected_index = state.selected_workspace_id.and_then(|target| {
        if &target.endpoint_id != state.active_endpoint_id {
            return None;
        }
        let in_target = |row: &TreeRow| {
            row.workspace_index()
                .and_then(|index| snapshot.workspaces.get(index))
                .is_some_and(|workspace| workspace.workspace_id == target.workspace_id)
        };
        state
            .navigate_tree_item
            .and_then(|cursor| {
                tree_rows
                    .iter()
                    .position(|row| in_target(row) && row.cursor().as_ref() == Some(cursor))
            })
            .or_else(|| {
                tree_rows.iter().position(|row| {
                    in_target(row) && matches!(row, TreeRow::Header { .. } | TreeRow::Parked { .. })
                })
            })
    });

    let mut metrics =
        super::scroll::list_scroll_metrics(&heights, &gaps, body.height, *state.workspace_scroll);
    let reveal_focus = std::mem::take(state.reveal_focused_workspace);
    let reveal_selection = std::mem::take(state.reveal_navigation_workspace);
    let reveal_target = if reveal_selection && selected_index.is_some() {
        selected_index
    } else if reveal_focus {
        rows.iter()
            .position(|row| {
                matches!(row, RenderedRow::Agora { row, .. } | RenderedRow::Agent { row, .. }
                    if row.focused)
            })
            .or_else(|| {
                rows.iter().position(|row| match row {
                    RenderedRow::Header {
                        workspace_index, ..
                    }
                    | RenderedRow::Parked {
                        workspace_index, ..
                    } => snapshot.workspaces[*workspace_index].focused,
                    _ => false,
                })
            })
    } else {
        None
    };
    if let Some(target) = reveal_target.filter(|_| !body.is_empty()) {
        *state.workspace_scroll = super::scroll::list_scroll_start_to_reveal(
            &heights,
            &gaps,
            body.height,
            *state.workspace_scroll,
            target,
        );
        metrics = super::scroll::list_scroll_metrics(
            &heights,
            &gaps,
            body.height,
            *state.workspace_scroll,
        );
    }
    hits.workspace_max_scroll = metrics.max_offset_from_bottom;
    hits.workspace_scroll_metrics = Some(metrics);
    *state.workspace_scroll = metrics
        .max_offset_from_bottom
        .saturating_sub(metrics.offset_from_bottom);
    let show_scrollbar = metrics.max_offset_from_bottom > 0 && body.width > 1;
    let content_width = body.width.saturating_sub(u16::from(show_scrollbar));
    let label = Style::default()
        .fg(palette.overlay0)
        .add_modifier(Modifier::BOLD);

    let mut y = body.y;
    for (index, row) in rows.iter().enumerate().skip(*state.workspace_scroll) {
        let height = heights[index].min(body.height);
        if body.is_empty() || y.saturating_add(height) > body.bottom() {
            break;
        }
        let rect = Rect::new(body.x, y, content_width, height);
        let selected = selected_index == Some(index);
        match row {
            RenderedRow::AgoraTitle => {
                render::put_text(buffer, rect.x, rect.y, rect.width, AGORA_TITLE, label);
            }
            RenderedRow::ParkedTitle => {
                render::put_text(buffer, rect.x, rect.y, rect.width, PARKED_TITLE, label);
            }
            RenderedRow::AgoraEmpty => {
                render::put_text(
                    buffer,
                    rect.x,
                    rect.y,
                    rect.width,
                    AGORA_EMPTY,
                    Style::default()
                        .fg(palette.overlay0)
                        .add_modifier(Modifier::DIM),
                );
            }
            RenderedRow::Agora {
                client,
                row,
                cli,
                precisa,
            } => {
                render_agora_row(
                    buffer,
                    rect,
                    row,
                    *client,
                    cli.as_deref(),
                    precisa.as_deref(),
                    selected,
                    config,
                );
                hits.agents.push((rect, row.pane_id.clone()));
            }
            RenderedRow::Header {
                workspace_index,
                client,
                expanded,
                summary,
            } => {
                let workspace = &snapshot.workspaces[*workspace_index];
                let dragged = state.dragged_workspace_id == Some(workspace.workspace_id.as_str());
                let background = if selected {
                    selection_background(palette)
                } else if dragged {
                    palette.surface1
                } else {
                    band_background(*client, palette)
                };
                render_band_header(
                    buffer, rect, workspace, *expanded, summary, background, palette,
                );
                hits.workspaces.push(WorkspaceHit {
                    rect,
                    endpoint_id: ClientEndpointId::Local,
                    workspace_id: workspace.workspace_id.clone(),
                    indented: false,
                    group_toggle: None,
                });
            }
            RenderedRow::Agent { client, row } => {
                render_band_agent(buffer, rect, row, *client, selected, config);
                hits.agents.push((rect, row.pane_id.clone()));
            }
            RenderedRow::Parked {
                workspace_index,
                client,
            } => {
                let workspace = &snapshot.workspaces[*workspace_index];
                let dragged = state.dragged_workspace_id == Some(workspace.workspace_id.as_str());
                render_parked(buffer, rect, workspace, *client, palette);
                let background = if selected {
                    Some(selection_background(palette))
                } else if dragged {
                    Some(palette.surface1)
                } else if workspace.focused {
                    Some(super::sidebar::workspace_active_background(
                        palette, navigating,
                    ))
                } else {
                    None
                };
                if let Some(background) = background {
                    fill_background(buffer, rect, background);
                }
                hits.workspaces.push(WorkspaceHit {
                    rect,
                    endpoint_id: ClientEndpointId::Local,
                    workspace_id: workspace.workspace_id.clone(),
                    indented: false,
                    group_toggle: None,
                });
            }
        }
        y = y.saturating_add(height).saturating_add(gaps[index]);
    }

    if show_scrollbar {
        let track = Rect::new(body.right().saturating_sub(1), body.y, 1, body.height);
        hits.workspace_scrollbar = track;
        super::scroll::render_list_scrollbar(buffer, track, metrics, palette);
    }

    if let Some(row) = state
        .workspace_drop_indicator_row
        .filter(|row| *row >= body.y && *row < body.bottom())
    {
        render::put_text(
            buffer,
            body.x,
            row,
            body.width,
            &"─".repeat(body.width as usize),
            Style::default().fg(palette.accent),
        );
    }

    render_footer(buffer, content, snapshot, config, hits);
}

fn spans_width(spans: &[ratatui::text::Span<'_>]) -> usize {
    spans.iter().map(|span| span.content.width()).sum()
}

/// "◐ os Melhorias para o Remo · precisa": state icon, client code in the client
/// color, the label and, for a blocked agent, what it needs in red.
#[allow(clippy::too_many_arguments)]
fn render_agora_row(
    buffer: &mut Buffer,
    rect: Rect,
    row: &AgentRow,
    client: Color,
    cli: Option<&str>,
    precisa: Option<&str>,
    selected: bool,
    config: &ClientShellConfig,
) {
    use ratatui::{
        text::{Line, Span},
        widgets::{Paragraph, Widget},
    };
    let palette = &config.palette;
    let status_style = Style::default().fg(status_color(row.status, palette));
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(
            status_icon(row.status, config.status_indicators).to_owned(),
            status_style,
        ),
        Span::raw(" "),
    ];
    if let Some(cli) = cli {
        spans.push(Span::styled(
            cli.to_owned(),
            Style::default().fg(client).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
    }
    let available = (rect.width as usize)
        .saturating_sub(1)
        .saturating_sub(spans_width(&spans));
    let name_style = if row.focused || selected {
        Style::default()
            .fg(palette.text)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(palette.subtext0)
    };
    let label_tokens = row
        .rows
        .first()
        .map(|tokens| {
            tokens
                .iter()
                .filter(|token| !matches!(token.kind, crate::ui::ResolvedTokenKind::StateIcon))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let label_spans = |width: usize| {
        crate::ui::resolved_token_spans(
            &label_tokens,
            (
                status_icon(row.status, config.status_indicators),
                status_style,
            ),
            status_style,
            name_style,
            Style::default().fg(palette.overlay0),
            name_style,
            palette,
            width,
        )
    };
    let full_label = spans_width(&label_spans(usize::MAX / 2));
    let precisa_width = precisa.map_or(0, |text| text.width() + 3);
    let label_budget = if full_label + precisa_width <= available {
        full_label
    } else {
        full_label.min(
            available
                .saturating_sub(precisa_width)
                .max(available * 3 / 5),
        )
    };
    let label = label_spans(label_budget);
    let used = spans_width(&label);
    spans.extend(label);
    if let Some(precisa) = precisa {
        let room = available.saturating_sub(used).saturating_sub(3);
        if room >= 2 {
            spans.push(Span::styled(" · ", Style::default().fg(palette.overlay0)));
            spans.push(Span::styled(
                crate::ui::truncate_end(precisa, room),
                Style::default()
                    .fg(palette.red)
                    .add_modifier(Modifier::BOLD),
            ));
        }
    }
    Paragraph::new(Line::from(spans)).render(rect, buffer);
    let background = if selected {
        Some(selection_background(palette))
    } else if row.focused {
        Some(palette.active_row_bg)
    } else {
        None
    };
    if let Some(background) = background {
        fill_background(buffer, rect, background);
    }
}

fn selection_background(palette: &Palette) -> Color {
    if palette.selection_bg == Color::Reset {
        palette.active_row_bg
    } else {
        palette.selection_bg
    }
}

fn fill_background(buffer: &mut Buffer, rect: Rect, background: Color) {
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            buffer[(x, y)].set_bg(background);
        }
    }
}

/// "▸ name" / "▾ name" on the left, the summary right-aligned, over the tinted band.
fn render_band_header(
    buffer: &mut Buffer,
    rect: Rect,
    workspace: &ClientShellWorkspace,
    expanded: bool,
    summary: &str,
    background: Color,
    palette: &Palette,
) {
    fill_background(buffer, rect, background);
    let text = Style::default().fg(palette.text).bg(background);
    render::put_text(
        buffer,
        rect.x + 1,
        rect.y,
        1,
        if expanded { "▾" } else { "▸" },
        text,
    );
    let inner = rect.width.saturating_sub(4) as usize;
    let summary_width = summary.width();
    let name_width = workspace.label.width();
    let show_summary = !summary.is_empty() && name_width + 2 + summary_width <= inner;
    let name = crate::ui::truncate_end(&workspace.label, inner);
    render::put_text(
        buffer,
        rect.x + 3,
        rect.y,
        rect.width.saturating_sub(3),
        &name,
        text.add_modifier(Modifier::BOLD),
    );
    if show_summary {
        render::put_right_text(
            buffer,
            Rect::new(rect.x, rect.y, rect.width.saturating_sub(1), 1),
            rect.y,
            summary,
            Style::default().fg(palette.subtext0).bg(background),
        );
    }
}

/// "┃ ◐ label": a rail in the client color, then the configured agent rows.
fn render_band_agent(
    buffer: &mut Buffer,
    rect: Rect,
    row: &AgentRow,
    client: Color,
    selected: bool,
    config: &ClientShellConfig,
) {
    use ratatui::{
        text::{Line, Span},
        widgets::{Paragraph, Widget},
    };
    let palette = &config.palette;
    let name_style = if row.focused || selected {
        Style::default()
            .fg(palette.text)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(palette.subtext0)
    };
    let secondary = Style::default().fg(palette.overlay0);
    let icon = (
        status_icon(row.status, config.status_indicators),
        Style::default().fg(status_color(row.status, palette)),
    );
    for (index, tokens) in row.rows.iter().take(rect.height as usize).enumerate() {
        let indent = if index == 0 { 3 } else { 5 };
        let mut spans = vec![
            Span::raw(" "),
            Span::styled(RAIL, Style::default().fg(client)),
            Span::raw(" ".repeat(indent - 2)),
        ];
        spans.extend(crate::ui::resolved_token_spans(
            tokens,
            icon,
            Style::default().fg(status_color(row.status, palette)),
            name_style,
            secondary,
            name_style,
            palette,
            (rect.width as usize).saturating_sub(indent + 1),
        ));
        Paragraph::new(Line::from(spans)).render(
            Rect::new(rect.x, rect.y + index as u16, rect.width, 1),
            buffer,
        );
    }
    let background = if selected {
        Some(selection_background(palette))
    } else if row.focused {
        Some(palette.active_row_bg)
    } else {
        None
    };
    if let Some(background) = background {
        fill_background(buffer, rect, background);
    }
}

/// "▸ name" with the name in the client color.
fn render_parked(
    buffer: &mut Buffer,
    rect: Rect,
    workspace: &ClientShellWorkspace,
    client: Color,
    palette: &Palette,
) {
    render::put_text(
        buffer,
        rect.x + 1,
        rect.y,
        1,
        "▸",
        Style::default().fg(palette.overlay0),
    );
    let name = crate::ui::truncate_end(&workspace.label, rect.width.saturating_sub(4) as usize);
    render::put_text(
        buffer,
        rect.x + 3,
        rect.y,
        rect.width.saturating_sub(4),
        &name,
        Style::default().fg(client).add_modifier(Modifier::BOLD),
    );
}

/// Bottom row: "new" on the left, "menu" and the collapse arrow on the right.
fn render_footer(
    buffer: &mut Buffer,
    content: Rect,
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    hits: &mut ShellHitMap,
) {
    let palette = &config.palette;
    let footer_y = content.bottom().saturating_sub(1);
    hits.sidebar_toggle = Rect::new(
        content.right().saturating_sub(1),
        footer_y,
        u16::from(content.width > 0),
        1,
    );
    render::put_text(
        buffer,
        hits.sidebar_toggle.x,
        hits.sidebar_toggle.y,
        hits.sidebar_toggle.width,
        "«",
        Style::default().fg(palette.overlay0),
    );
    if !config.mouse_capture {
        return;
    }
    let footer = Rect::new(content.x, footer_y, content.width.saturating_sub(2), 1);
    hits.new_workspace = Rect::new(footer.x, footer_y, 5.min(footer.width), 1);
    render::put_text(
        buffer,
        footer.x,
        footer_y,
        footer.width,
        " new",
        Style::default().fg(palette.overlay0),
    );
    let attention = super::global_menu::global_menu_attention(snapshot);
    let launcher_width = if attention { 8 } else { 6 }.min(footer.width);
    hits.global_launcher = Rect::new(
        footer.right().saturating_sub(launcher_width),
        footer_y,
        launcher_width,
        1,
    );
    if attention {
        let start_x = footer.right().saturating_sub(6);
        render::put_text(
            buffer,
            start_x,
            footer_y,
            2,
            "● ",
            Style::default()
                .fg(palette.accent)
                .add_modifier(Modifier::BOLD),
        );
        render::put_text(
            buffer,
            start_x.saturating_add(2),
            footer_y,
            4,
            "menu",
            Style::default().fg(palette.overlay0),
        );
    } else {
        render::put_right_text(
            buffer,
            footer,
            footer_y,
            "menu",
            Style::default().fg(palette.overlay0),
        );
    }
}

impl ClientShellState {
    /// The integrated layout replaces the expanded desktop sidebar of a single
    /// machine only; the collapsed rail, mobile layout and multi-machine
    /// sidebar stay as they are.
    pub(super) fn tree_layout_active(&self) -> bool {
        self.config.sidebar_layout == crate::config::SidebarLayoutConfig::Tree
            && self.endpoints.len() <= 1
            && !self.sidebar_collapsed
            && !self.mobile_layout_active()
    }

    /// A click (press and release without a drag) on a band header expands or
    /// collapses the band and leaves the space on screen alone. A click on a
    /// PARADOS row is not handled here: it switches to that space as usual.
    pub(super) fn tree_header_click_toggles(&mut self, press: &ClientWorkspacePress) -> bool {
        if !self.tree_layout_active() || press.endpoint_id != self.active_endpoint_id {
            return false;
        }
        let Some(snapshot) = self.snapshot.as_deref() else {
            return false;
        };
        let has_band = snapshot
            .agents
            .iter()
            .any(|agent| agent.workspace_id == press.workspace_id);
        if !has_band {
            return false;
        }
        self.sidebar_tree.toggle(snapshot, &press.workspace_id);
        true
    }

    fn tree_cursor_position(
        &self,
        rows: &[TreeRow],
        snapshot: &ClientShellSnapshot,
    ) -> Option<usize> {
        let target = self.navigate_workspace_id.as_ref()?;
        if target.endpoint_id != self.active_endpoint_id {
            return None;
        }
        let in_target = |row: &TreeRow| {
            row.workspace_index()
                .and_then(|index| snapshot.workspaces.get(index))
                .is_some_and(|workspace| workspace.workspace_id == target.workspace_id)
        };
        self.navigate_tree_item
            .as_ref()
            .and_then(|cursor| {
                rows.iter()
                    .position(|row| in_target(row) && row.cursor().as_ref() == Some(cursor))
            })
            .or_else(|| {
                rows.iter().position(|row| {
                    in_target(row) && matches!(row, TreeRow::Header { .. } | TreeRow::Parked { .. })
                })
            })
    }

    /// Navigate mode moves through AGORA rows, band headers, expanded band rows
    /// and PARADOS rows in visual order.
    pub(super) fn move_navigate_tree(&mut self, delta: isize) {
        let Some(snapshot) = self.snapshot.as_deref() else {
            return;
        };
        let rows = tree_rows(snapshot, &self.sidebar_tree)
            .into_iter()
            .filter(TreeRow::navigable)
            .collect::<Vec<_>>();
        if rows.is_empty() {
            return;
        }
        let next = match self.tree_cursor_position(&rows, snapshot) {
            Some(current) => (current as isize + delta).rem_euclid(rows.len() as isize) as usize,
            None if delta < 0 => rows.len() - 1,
            None => 0,
        };
        let row = &rows[next];
        let Some(workspace) = row
            .workspace_index()
            .and_then(|index| snapshot.workspaces.get(index))
        else {
            return;
        };
        let workspace_id = workspace.workspace_id.clone();
        let cursor = row.cursor();
        let Some(target) = self.navigation_target(&self.active_endpoint_id, &workspace_id) else {
            return;
        };
        self.navigate_workspace_id = Some(target);
        self.navigate_tree_item = cursor;
        self.reveal_navigation_workspace = true;
    }

    /// Band keys in navigate mode: Space toggles the band under the cursor,
    /// Left collapses it (or climbs from one of its rows to its header), Right
    /// expands it. Returns false for keys left to the usual navigate bindings.
    pub(super) fn route_tree_navigate_key(
        &mut self,
        code: KeyCode,
        outcome: &mut ClientShellInput,
    ) -> bool {
        if !self.tree_layout_active() {
            return false;
        }
        let Some(target) = self.navigate_workspace_id.as_ref() else {
            return false;
        };
        if target.endpoint_id != self.active_endpoint_id {
            return false;
        }
        let workspace_id = target.workspace_id.clone();
        let Some(snapshot) = self.snapshot.as_deref() else {
            return false;
        };
        let has_band = snapshot
            .agents
            .iter()
            .any(|agent| agent.workspace_id == workspace_id);
        // Only a band row (an idle agent under its header) climbs to the header.
        let in_band = self.navigate_tree_item.as_ref().is_some_and(|cursor| {
            let TreeCursor::Agent(pane_id) = cursor;
            snapshot
                .agents
                .iter()
                .any(|agent| &agent.pane_id == pane_id && !in_agora(agent.agent_status))
        });
        let on_header = has_band && self.navigate_tree_item.is_none();
        let expanded = self.sidebar_tree.is_expanded(snapshot, &workspace_id);
        match code {
            KeyCode::Char(' ') if on_header => {
                self.sidebar_tree.toggle(snapshot, &workspace_id);
            }
            KeyCode::Left if in_band => {
                self.navigate_tree_item = None;
            }
            KeyCode::Left if on_header && expanded => {
                self.sidebar_tree.set_expanded(&workspace_id, false);
            }
            KeyCode::Right if on_header && !expanded => {
                self.sidebar_tree.set_expanded(&workspace_id, true);
            }
            _ => return false,
        }
        self.reveal_navigation_workspace = true;
        outcome.repaint = true;
        true
    }

    /// Enter on an agent row (AGORA or band) focuses that agent, like a click.
    pub(super) fn accept_navigate_tree_item(&mut self, outcome: &mut ClientShellInput) -> bool {
        let Some(TreeCursor::Agent(pane_id)) = self.navigate_tree_item.take() else {
            return false;
        };
        let valid = self.tree_layout_active()
            && self.navigate_workspace_id.as_ref().is_some_and(|target| {
                target.endpoint_id == self.active_endpoint_id
                    && self.navigation_target_valid(target)
            })
            && self.snapshot.as_deref().is_some_and(|snapshot| {
                snapshot.agents.iter().any(|agent| agent.pane_id == pane_id)
            });
        if !valid {
            return false;
        }
        self.push_endpoint_method(
            crate::api::schema::Method::PaneFocus(crate::api::schema::PaneTarget { pane_id }),
            outcome,
        );
        self.mode = ClientShellMode::Terminal;
        self.navigate_workspace_id = None;
        outcome.repaint = true;
        true
    }
}
