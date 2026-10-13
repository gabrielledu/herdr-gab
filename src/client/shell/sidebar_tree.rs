//! Integrated layout of the expanded desktop sidebar (`ui.sidebar.layout =
//! "tree"`, alias `"bands"`). One scrollable panel replaces the split
//! spaces/agents panels:
//!
//! * `AGORA`: every blocked or done (unseen) agent of every space, most
//!   recent first (by the `ult` token, agents without `ult` last). Each row shows the state icon, the client code
//!   (`$cli`) in the client color, the label and the elapsed time on the
//!   right; a second line shows `$precisa` (red), else `$passo` when `fase` is
//!   `bloq` (red) or `vez` (peach). Seeing a finished agent turns it idle, so
//!   it leaves AGORA.
//! * `RODANDO`: every working agent, in the same row format with `agora` on
//!   the right. Hidden when empty.
//! * One band per space that has agents, the space with the most recent
//!   activity first: a header tinted with the client color, `▸/▾ name`, how
//!   many agents wait for the user (`➜N`) and the space summary (`$resumo`).
//!   Expanding a band lists, behind a `┃` rail, the agents of that space that
//!   are not in AGORA, so no agent appears twice: first the ones still waiting
//!   for the user (`$precisa`, `fase=bloq` or `fase=vez`) as `➜` and
//!   `$precisa` or `$passo` in place of the label, then the rest; each group
//!   most recent first. The arrow (and `➜N`) is red when an agent is stopped on
//!   the user (`$precisa` or `bloq`) and peach for a step left (`vez`);
//!   `fase=esp` adds `· $passo` in blue.
//!   Bands start collapsed except the one holding the focused pane; a manual
//!   toggle sticks for the session.
//! * `PARADOS`: spaces without agents, one compact row each.
//!
//! The `fase`, `passo` and `ult` tokens come from Gab HQ (`ult` is the epoch
//! second of the session's last message); a working agent keeps a stale
//! `fase`, so the bar ignores `fase` while it works. Elapsed time reads
//! `agora`, `Nmin`, `Nh` or `Nd` and turns peach after 24 hours; the client
//! repaints once a minute while this layout is on. All of this is client
//! presentation state.

use ratatui::style::Color;
use unicode_width::UnicodeWidthStr as _;

use super::agent_sidebar::AgentRow;
use super::*;
use crate::protocol::ClientShellAgent;

/// Share of the client color in a band header background.
pub(super) const BAND_TINT: f32 = 0.22;
const RAIL: &str = "┃";
const AGORA_TITLE: &str = " AGORA";
const AGORA_EMPTY: &str = " nada novo";
const RUNNING_TITLE: &str = " RODANDO";
const PENDING_ICON: &str = "➜";
const PARKED_TITLE: &str = " PARADOS";
/// Elapsed time turns peach from this age on.
const OLD_AFTER_SECS: u64 = 24 * 3600;

/// The navigate cursor on an agent row (AGORA or band), beyond the space's
/// band header or PARADOS row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TreeCursor {
    Agent(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TreeRow {
    AgoraTitle,
    /// "nada novo" when no agent is blocked or done.
    AgoraEmpty,
    /// A blocked or done agent under AGORA, or a working one under RODANDO.
    Agora {
        workspace_index: usize,
        pane_id: String,
    },
    RunningTitle,
    /// An agent of an expanded band that still waits for the user.
    Pending {
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
            | Self::Pending {
                workspace_index, ..
            }
            | Self::Header {
                workspace_index, ..
            }
            | Self::Agent {
                workspace_index, ..
            }
            | Self::Parked { workspace_index } => Some(*workspace_index),
            Self::AgoraTitle | Self::AgoraEmpty | Self::RunningTitle | Self::ParkedTitle => None,
        }
    }

    pub(super) fn pane_id(&self) -> Option<&str> {
        match self {
            Self::Agora { pane_id, .. }
            | Self::Pending { pane_id, .. }
            | Self::Agent { pane_id, .. } => Some(pane_id),
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

/// Tie-break inside AGORA for agents with the same recency: blocked, done,
/// working.
fn agora_rank(status: crate::api::schema::AgentStatus) -> u8 {
    use crate::api::schema::AgentStatus;
    match status {
        AgentStatus::Blocked => 0,
        AgentStatus::Done => 1,
        AgentStatus::Working => 2,
        AgentStatus::Idle | AgentStatus::Unknown => 3,
    }
}

/// The session state Gab HQ writes in the `fase` token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Fase {
    /// The agent stopped waiting for the user.
    Bloq,
    /// Delivered, with one step left for the user.
    Vez,
    /// Waiting on CI, a job or someone else.
    Esp,
    /// Nothing left for the user.
    Ok,
}

/// A non-blank custom token of the agent.
fn token_of<'a>(agent: &'a ClientShellAgent, name: &str) -> Option<&'a str> {
    agent
        .tokens
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
}

fn working(agent: &ClientShellAgent) -> bool {
    agent.agent_status == crate::api::schema::AgentStatus::Working
}

/// The agent's `fase`; none while it works (the token keeps the previous
/// session's state then).
pub(super) fn fase(agent: &ClientShellAgent) -> Option<Fase> {
    if working(agent) {
        return None;
    }
    match token_of(agent, "fase")? {
        "bloq" => Some(Fase::Bloq),
        "vez" => Some(Fase::Vez),
        "esp" => Some(Fase::Esp),
        "ok" => Some(Fase::Ok),
        _ => None,
    }
}

/// Epoch second of the session's last message (`ult`); a millisecond value
/// is accepted too.
pub(super) fn last_activity(agent: &ClientShellAgent) -> Option<u64> {
    let value = token_of(agent, "ult")?;
    let seconds = value.parse::<u64>().ok().or_else(|| {
        value
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && *value >= 0.0)
            .map(|value| value as u64)
    })?;
    Some(if seconds >= 100_000_000_000 {
        seconds / 1000
    } else {
        seconds
    })
}

/// Sort key, larger is more recent: a working agent is "now", otherwise its
/// last activity; agents without `ult` sort last.
fn recency(agent: &ClientShellAgent) -> Option<u64> {
    if working(agent) {
        Some(u64::MAX)
    } else {
        last_activity(agent)
    }
}

/// AGORA: blocked, done (unseen) or working. An idle agent that waits for the
/// user was already seen and goes to PENDENTE instead.
pub(super) fn in_now(agent: &ClientShellAgent) -> bool {
    in_agora(agent.agent_status)
}

/// Stopped on the user: `$precisa` or `fase=bloq`, not while it works.
pub(super) fn waits_on_user(agent: &ClientShellAgent) -> bool {
    !working(agent) && (token_of(agent, "precisa").is_some() || fase(agent) == Some(Fase::Bloq))
}

/// Outside AGORA, stopped on the user or with a step left: listed first in
/// its band and counted on the band header.
pub(super) fn is_pending(agent: &ClientShellAgent) -> bool {
    !in_now(agent) && (waits_on_user(agent) || fase(agent) == Some(Fase::Vez))
}

/// "agora" under a minute, then "Nmin", "Nh", "Nd" (rounded down).
pub(super) fn elapsed_label(since: u64, now: u64) -> String {
    let seconds = now.saturating_sub(since);
    match seconds {
        0..60 => "agora".to_owned(),
        60..3600 => format!("{}min", seconds / 60),
        3600..86400 => format!("{}h", seconds / 3600),
        _ => format!("{}d", seconds / 86400),
    }
}

/// Elapsed label and whether it is older than 24 hours; "agora" while the
/// agent works, none without `ult`.
pub(super) fn agent_elapsed(agent: &ClientShellAgent, now: u64) -> Option<(String, bool)> {
    if working(agent) {
        return Some(("agora".to_owned(), false));
    }
    let since = last_activity(agent)?;
    Some((
        elapsed_label(since, now),
        now.saturating_sub(since) >= OLD_AFTER_SECS,
    ))
}

/// Wall-clock epoch seconds.
pub(super) fn epoch_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
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

/// Visible rows in visual order: AGORA, RODANDO, the bands, PARADOS.
pub(super) fn tree_rows(snapshot: &ClientShellSnapshot, tree: &SidebarTreeState) -> Vec<TreeRow> {
    use std::cmp::Reverse;
    let index_of = |workspace_id: &str| {
        snapshot
            .workspaces
            .iter()
            .position(|workspace| workspace.workspace_id == workspace_id)
    };
    let (mut running, mut agora): (Vec<_>, Vec<_>) = snapshot
        .agents
        .iter()
        .filter(|agent| in_now(agent))
        .filter_map(|agent| Some((index_of(&agent.workspace_id)?, agent)))
        .partition(|(_, agent)| working(agent));
    agora.sort_by_key(|(workspace_index, agent)| {
        (
            Reverse(recency(agent)),
            agora_rank(agent.agent_status),
            *workspace_index,
        )
    });
    running.sort_by_key(|(workspace_index, _)| *workspace_index);
    let as_rows = |agents: Vec<(usize, &ClientShellAgent)>| {
        agents
            .into_iter()
            .map(|(workspace_index, agent)| TreeRow::Agora {
                workspace_index,
                pane_id: agent.pane_id.clone(),
            })
            .collect::<Vec<_>>()
    };
    let mut rows = vec![TreeRow::AgoraTitle];
    if agora.is_empty() {
        rows.push(TreeRow::AgoraEmpty);
    }
    rows.extend(as_rows(agora));
    if !running.is_empty() {
        rows.push(TreeRow::RunningTitle);
        rows.extend(as_rows(running));
    }
    let mut parked = Vec::new();
    let mut bands = Vec::new();
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
        let latest = agents.iter().filter_map(|agent| recency(agent)).max();
        bands.push((workspace_index, workspace, agents, latest));
    }
    // The space with the most recent activity first; ties keep sidebar order.
    bands.sort_by_key(|(workspace_index, _, _, latest)| (Reverse(*latest), *workspace_index));
    for (workspace_index, workspace, agents, _) in bands {
        let expanded = tree.is_expanded(snapshot, &workspace.workspace_id);
        rows.push(TreeRow::Header {
            workspace_index,
            expanded,
        });
        if expanded {
            let (mut waiting, mut rest): (Vec<_>, Vec<_>) = agents
                .into_iter()
                .filter(|agent| !in_now(agent))
                .partition(|agent| is_pending(agent));
            waiting.sort_by_key(|agent| Reverse(recency(agent)));
            rest.sort_by_key(|agent| Reverse(recency(agent)));
            rows.extend(waiting.into_iter().map(|agent| TreeRow::Pending {
                workspace_index,
                pane_id: agent.pane_id.clone(),
            }));
            rows.extend(rest.into_iter().map(|agent| TreeRow::Agent {
                workspace_index,
                pane_id: agent.pane_id.clone(),
            }));
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

fn find_agent<'a>(
    snapshot: &'a ClientShellSnapshot,
    pane_id: &str,
) -> Option<&'a ClientShellAgent> {
    snapshot
        .agents
        .iter()
        .find(|agent| agent.pane_id == pane_id)
}

/// Elapsed time on the right of an agent row: the text and whether it is
/// older than 24 hours.
type Elapsed = Option<(String, bool)>;

enum RenderedRow {
    AgoraTitle {
        count: usize,
    },
    AgoraEmpty,
    RunningTitle {
        count: usize,
    },
    Agora {
        client: Color,
        row: AgentRow,
        cli: Option<String>,
        /// Second line: `$precisa`, else `$passo` for `fase` bloq or vez.
        detail: Option<(String, Color)>,
        elapsed: Elapsed,
    },
    Pending {
        client: Color,
        pane_id: String,
        focused: bool,
        /// `$precisa`, else `$passo`, else the label.
        text: String,
        /// Stopped on the user (`$precisa` or `bloq`): red arrow.
        urgent: bool,
        elapsed: Elapsed,
    },
    Header {
        workspace_index: usize,
        client: Color,
        expanded: bool,
        summary: String,
        /// Agents of the space waiting for the user, and whether one is
        /// stopped on the user.
        waiting: (usize, bool),
    },
    Agent {
        client: Color,
        row: AgentRow,
        /// `$passo` of a `fase=esp` agent.
        waiting_on: Option<String>,
        elapsed: Elapsed,
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
            Self::Agora { detail, .. } => 1 + u16::from(detail.is_some()),
            _ => 1,
        }
    }

    fn focused(&self) -> bool {
        match self {
            Self::Agora { row, .. } | Self::Agent { row, .. } => row.focused,
            Self::Pending { focused, .. } => *focused,
            _ => false,
        }
    }
}

/// Blank rows before the sections after AGORA (PENDENTE, the bands, PARADOS)
/// and between bands, as far as they fit; PARADOS then sinks to the bottom of
/// the panel.
fn row_gaps(rows: &[RenderedRow], heights: &[u16], body_height: u16) -> Vec<u16> {
    let first_header = rows
        .iter()
        .position(|row| matches!(row, RenderedRow::Header { .. }));
    let section_break = |index: usize| {
        index > 0
            && (Some(index) == first_header
                || matches!(
                    rows[index],
                    RenderedRow::ParkedTitle | RenderedRow::RunningTitle { .. }
                ))
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

/// Rendered form of each tree row; `now` is the wall clock in epoch seconds.
fn rendered_rows(
    tree_rows: &[TreeRow],
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    now: u64,
) -> Vec<RenderedRow> {
    let palette = &config.palette;
    let running_at = tree_rows
        .iter()
        .position(|row| *row == TreeRow::RunningTitle)
        .unwrap_or(tree_rows.len());
    let agora_rows = |rows: &[TreeRow]| {
        rows.iter()
            .filter(|row| matches!(row, TreeRow::Agora { .. }))
            .count()
    };
    let agora_count = agora_rows(&tree_rows[..running_at]);
    let running_count = agora_rows(&tree_rows[running_at..]);
    tree_rows
        .iter()
        .filter_map(|row| {
            let workspace = row
                .workspace_index()
                .and_then(|index| snapshot.workspaces.get(index));
            let client = workspace.map(|workspace| client_color(workspace, config));
            Some(match row {
                TreeRow::AgoraTitle => RenderedRow::AgoraTitle { count: agora_count },
                TreeRow::AgoraEmpty => RenderedRow::AgoraEmpty,
                TreeRow::RunningTitle => RenderedRow::RunningTitle {
                    count: running_count,
                },
                TreeRow::Agora { pane_id, .. } => {
                    let agent = find_agent(snapshot, pane_id)?;
                    let row = tree_agent_row(snapshot, pane_id, config)?;
                    let fase = fase(agent);
                    let precisa = (!working(agent))
                        .then(|| token_of(agent, "precisa"))
                        .flatten();
                    let passo = token_of(agent, "passo");
                    let detail = precisa
                        .map(|precisa| (precisa.to_owned(), palette.red))
                        .or_else(|| match (fase, passo) {
                            (Some(Fase::Bloq), Some(passo)) => {
                                Some((passo.to_owned(), palette.red))
                            }
                            (Some(Fase::Vez), Some(passo)) => {
                                Some((passo.to_owned(), palette.peach))
                            }
                            _ => None,
                        });
                    RenderedRow::Agora {
                        client: client?,
                        cli: token_of(agent, "cli").map(str::to_owned),
                        detail,
                        elapsed: agent_elapsed(agent, now),
                        row,
                    }
                }
                TreeRow::Pending { pane_id, .. } => {
                    let agent = find_agent(snapshot, pane_id)?;
                    let text = token_of(agent, "precisa")
                        .or_else(|| token_of(agent, "passo"))
                        .map(str::to_owned)
                        .or_else(|| {
                            tree_agent_row(snapshot, pane_id, config)?
                                .rows
                                .first()?
                                .iter()
                                .find_map(|token| match &token.kind {
                                    crate::ui::ResolvedTokenKind::StateIcon
                                    | crate::ui::ResolvedTokenKind::GitStatus { .. } => None,
                                    crate::ui::ResolvedTokenKind::StateText(text)
                                    | crate::ui::ResolvedTokenKind::Machine(text)
                                    | crate::ui::ResolvedTokenKind::Workspace(text)
                                    | crate::ui::ResolvedTokenKind::Tab(text)
                                    | crate::ui::ResolvedTokenKind::Pane(text)
                                    | crate::ui::ResolvedTokenKind::Agent(text)
                                    | crate::ui::ResolvedTokenKind::TerminalTitle(text)
                                    | crate::ui::ResolvedTokenKind::Branch(text)
                                    | crate::ui::ResolvedTokenKind::Custom(text) => {
                                        Some(text.trim().to_owned()).filter(|text| !text.is_empty())
                                    }
                                })
                        })
                        .unwrap_or_else(|| pane_id.clone());
                    RenderedRow::Pending {
                        client: client?,
                        pane_id: pane_id.clone(),
                        focused: agent.focused,
                        text,
                        urgent: waits_on_user(agent),
                        elapsed: agent_elapsed(agent, now),
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
                    waiting: workspace_waiting(snapshot, &workspace?.workspace_id),
                },
                TreeRow::Agent { pane_id, .. } => {
                    let agent = find_agent(snapshot, pane_id)?;
                    RenderedRow::Agent {
                        client: client?,
                        row: tree_agent_row(snapshot, pane_id, config)?,
                        waiting_on: (fase(agent) == Some(Fase::Esp))
                            .then(|| token_of(agent, "passo").map(str::to_owned))
                            .flatten(),
                        elapsed: agent_elapsed(agent, now),
                    }
                }
                TreeRow::ParkedTitle => RenderedRow::ParkedTitle,
                TreeRow::Parked { workspace_index } => RenderedRow::Parked {
                    workspace_index: *workspace_index,
                    client: client?,
                },
            })
        })
        .collect()
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
    let rows = rendered_rows(&tree_rows, snapshot, config, epoch_now());
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
        rows.iter().position(RenderedRow::focused).or_else(|| {
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
            RenderedRow::AgoraTitle { count } => {
                render_section_title(buffer, rect, AGORA_TITLE, Some(*count), label, palette);
            }
            RenderedRow::RunningTitle { count } => {
                render_section_title(buffer, rect, RUNNING_TITLE, Some(*count), label, palette);
            }
            RenderedRow::ParkedTitle => {
                render_section_title(buffer, rect, PARKED_TITLE, None, label, palette);
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
                detail,
                elapsed,
            } => {
                render_agora_row(
                    buffer,
                    rect,
                    AgoraLine {
                        row,
                        client: *client,
                        cli: cli.as_deref(),
                        detail: detail.as_ref().map(|(text, color)| (text.as_str(), *color)),
                        elapsed: elapsed.as_ref(),
                    },
                    selected,
                    config,
                );
                hits.agents.push((rect, row.pane_id.clone()));
            }
            RenderedRow::Pending {
                client,
                pane_id,
                focused,
                text,
                urgent,
                elapsed,
            } => {
                render_pending_row(
                    buffer,
                    rect,
                    *client,
                    text,
                    *urgent,
                    elapsed.as_ref(),
                    *focused,
                    selected,
                    palette,
                );
                hits.agents.push((rect, pane_id.clone()));
            }
            RenderedRow::Header {
                workspace_index,
                client,
                expanded,
                summary,
                waiting,
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
                    buffer, rect, workspace, *expanded, summary, *waiting, background, palette,
                );
                hits.workspaces.push(WorkspaceHit {
                    rect,
                    endpoint_id: ClientEndpointId::Local,
                    workspace_id: workspace.workspace_id.clone(),
                    indented: false,
                    group_toggle: None,
                });
            }
            RenderedRow::Agent {
                client,
                row,
                waiting_on,
                elapsed,
            } => {
                render_band_agent(
                    buffer,
                    rect,
                    row,
                    *client,
                    waiting_on.as_deref(),
                    elapsed.as_ref(),
                    selected,
                    config,
                );
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

/// Section title on the left, its count on the right.
fn render_section_title(
    buffer: &mut Buffer,
    rect: Rect,
    title: &str,
    count: Option<usize>,
    style: Style,
    palette: &Palette,
) {
    render::put_text(buffer, rect.x, rect.y, rect.width, title, style);
    if let Some(count) = count.filter(|count| *count > 0) {
        render::put_right_text(
            buffer,
            Rect::new(rect.x, rect.y, rect.width.saturating_sub(1), 1),
            rect.y,
            &count.to_string(),
            Style::default().fg(palette.overlay0),
        );
    }
}

fn elapsed_style(old: bool, palette: &Palette) -> Style {
    Style::default().fg(if old { palette.peach } else { palette.overlay0 })
}

/// One agent line: `prefix`, a label fitted by `label` to a width, an optional
/// `· suffix` and the elapsed time right-aligned. The label keeps at least
/// three fifths of the room when the suffix does not fit beside it.
fn render_agent_line(
    buffer: &mut Buffer,
    rect: Rect,
    mut spans: Vec<ratatui::text::Span<'static>>,
    label: &dyn Fn(usize) -> Vec<ratatui::text::Span<'static>>,
    suffix: Option<(&str, Style)>,
    elapsed: Option<&(String, bool)>,
    palette: &Palette,
) {
    use ratatui::{
        text::{Line, Span},
        widgets::{Paragraph, Widget},
    };
    let line = Rect::new(rect.x, rect.y, rect.width, rect.height.min(1));
    let elapsed_width = elapsed.map_or(0, |(text, _)| text.width() + 1);
    let available = (line.width as usize)
        .saturating_sub(1)
        .saturating_sub(spans_width(&spans))
        .saturating_sub(elapsed_width);
    let full_label = spans_width(&label(usize::MAX / 2));
    let suffix_width = suffix.map_or(0, |(text, _)| text.width() + 3);
    let label_budget = if full_label + suffix_width <= available {
        full_label
    } else {
        full_label.min(
            available
                .saturating_sub(suffix_width)
                .max(available * 3 / 5),
        )
    };
    let label = label(label_budget);
    let used = spans_width(&label);
    spans.extend(label);
    if let Some((suffix, style)) = suffix {
        let room = available.saturating_sub(used).saturating_sub(3);
        if room >= 2 {
            spans.push(Span::styled(" · ", Style::default().fg(palette.overlay0)));
            spans.push(Span::styled(crate::ui::truncate_end(suffix, room), style));
        }
    }
    Paragraph::new(Line::from(spans)).render(line, buffer);
    if let Some((text, old)) = elapsed.filter(|_| line.width > 2) {
        render::put_right_text(
            buffer,
            Rect::new(line.x, line.y, line.width.saturating_sub(1), 1),
            line.y,
            text,
            elapsed_style(*old, palette),
        );
    }
}

/// The configured first row of an agent without its state icon, fitted to a
/// width.
fn label_spans<'a>(
    row: &'a AgentRow,
    icon: (&'a str, Style),
    name_style: Style,
    palette: &'a Palette,
) -> impl Fn(usize) -> Vec<ratatui::text::Span<'static>> + 'a {
    let tokens = row
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
    move |width| {
        crate::ui::resolved_token_spans(
            &tokens,
            icon,
            icon.1,
            name_style,
            Style::default().fg(palette.overlay0),
            name_style,
            palette,
            width,
        )
    }
}

struct AgoraLine<'a> {
    row: &'a AgentRow,
    client: Color,
    cli: Option<&'a str>,
    detail: Option<(&'a str, Color)>,
    elapsed: Option<&'a (String, bool)>,
}

/// "◐ os Melhorias para o Remo      3h" and, below, what the agent needs or the
/// step left for the user: state icon, client code in the client color, the
/// label (bold while done and unseen), the elapsed time on the right.
fn render_agora_row(
    buffer: &mut Buffer,
    rect: Rect,
    line: AgoraLine<'_>,
    selected: bool,
    config: &ClientShellConfig,
) {
    use crate::api::schema::AgentStatus;
    use ratatui::text::Span;
    let palette = &config.palette;
    let row = line.row;
    let status_style = Style::default().fg(status_color(line.row.status, palette));
    let icon = status_icon(line.row.status, config.status_indicators);
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(icon.to_owned(), status_style),
        Span::raw(" "),
    ];
    if let Some(cli) = line.cli {
        spans.push(Span::styled(
            cli.to_owned(),
            Style::default()
                .fg(line.client)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
    }
    let name_style = if row.focused || selected || row.status == AgentStatus::Done {
        Style::default()
            .fg(palette.text)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(palette.subtext0)
    };
    render_agent_line(
        buffer,
        rect,
        spans,
        &label_spans(row, (icon, status_style), name_style, palette),
        None,
        line.elapsed,
        palette,
    );
    if let Some((detail, color)) = line.detail.filter(|_| rect.height > 1) {
        render::put_text(
            buffer,
            rect.x + 4,
            rect.y + 1,
            rect.width.saturating_sub(5),
            &crate::ui::truncate_end(detail, rect.width.saturating_sub(5) as usize),
            Style::default().fg(color),
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

/// "➜ br enviar 7 rascunhos da Brez      2h": the step left for the user in
/// place of the label.
#[allow(clippy::too_many_arguments)]
fn render_pending_row(
    buffer: &mut Buffer,
    rect: Rect,
    client: Color,
    text: &str,
    urgent: bool,
    elapsed: Option<&(String, bool)>,
    focused: bool,
    selected: bool,
    palette: &Palette,
) {
    use ratatui::text::Span;
    let spans = vec![
        Span::raw(" "),
        Span::styled(RAIL, Style::default().fg(client)),
        Span::raw(" "),
        Span::styled(
            PENDING_ICON,
            Style::default()
                .fg(waiting_color(urgent, palette))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ];
    let style = if focused || selected {
        Style::default()
            .fg(palette.text)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(palette.subtext0)
    };
    let text = text.to_owned();
    render_agent_line(
        buffer,
        rect,
        spans,
        &move |width| vec![Span::styled(crate::ui::truncate_end(&text, width), style)],
        None,
        elapsed,
        palette,
    );
    let background = if selected {
        Some(selection_background(palette))
    } else if focused {
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
    waiting: (usize, bool),
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
    let count = (waiting.0 > 0).then(|| format!("{PENDING_ICON}{}", waiting.0));
    let count_width = count.as_ref().map_or(0, |count| count.width() + 1);
    let summary_width = summary.width();
    let name_width = workspace.label.width();
    let show_summary =
        !summary.is_empty() && name_width + 2 + count_width + summary_width <= inner;
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
    // `➜N` just left of the summary, or at the right edge when it does not fit.
    if let Some(count) = count {
        let right = rect.right().saturating_sub(1)
            - if show_summary { summary_width as u16 + 1 } else { 0 };
        let width = count.width() as u16;
        if rect.x + 3 + name_width as u16 + 1 + width <= right {
            render::put_text(
                buffer,
                right - width,
                rect.y,
                width,
                &count,
                Style::default()
                    .fg(waiting_color(waiting.1, palette))
                    .bg(background)
                    .add_modifier(Modifier::BOLD),
            );
        }
    }
}

/// Red when an agent is stopped on the user, peach for a step left.
fn waiting_color(urgent: bool, palette: &Palette) -> Color {
    if urgent {
        palette.red
    } else {
        palette.peach
    }
}

/// How many agents of the space wait for the user outside AGORA, and whether
/// one of them is stopped on the user.
fn workspace_waiting(snapshot: &ClientShellSnapshot, workspace_id: &str) -> (usize, bool) {
    snapshot
        .agents
        .iter()
        .filter(|agent| agent.workspace_id == workspace_id && is_pending(agent))
        .fold((0, false), |(count, urgent), agent| {
            (count + 1, urgent || waits_on_user(agent))
        })
}

/// "┃ ◐ label · passo      5h": a rail in the client color, then the
/// configured agent rows; the first one carries what a waiting (`fase=esp`)
/// agent waits on and the elapsed time.
#[allow(clippy::too_many_arguments)]
fn render_band_agent(
    buffer: &mut Buffer,
    rect: Rect,
    row: &AgentRow,
    client: Color,
    waiting_on: Option<&str>,
    elapsed: Option<&(String, bool)>,
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
        let line = Rect::new(rect.x, rect.y + index as u16, rect.width, 1);
        if index == 0 {
            let tokens = tokens.clone();
            render_agent_line(
                buffer,
                line,
                spans,
                &move |width| {
                    crate::ui::resolved_token_spans(
                        &tokens, icon, icon.1, name_style, secondary, name_style, palette, width,
                    )
                },
                waiting_on.map(|text| (text, Style::default().fg(palette.blue))),
                elapsed,
                palette,
            );
            continue;
        }
        spans.extend(crate::ui::resolved_token_spans(
            tokens,
            icon,
            icon.1,
            name_style,
            secondary,
            name_style,
            palette,
            (rect.width as usize).saturating_sub(indent + 1),
        ));
        Paragraph::new(Line::from(spans)).render(line, buffer);
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

    /// Repaint once per wall-clock minute while the tree layout is on, so the
    /// elapsed times keep moving without input.
    pub(crate) fn tick_sidebar_clock(&mut self) -> bool {
        self.tick_sidebar_clock_at(epoch_now())
    }

    /// [`Self::tick_sidebar_clock`] at `now` (epoch seconds).
    pub(super) fn tick_sidebar_clock_at(&mut self, now: u64) -> bool {
        if !self.tree_layout_active() || self.snapshot.is_none() {
            self.sidebar_tree_minute = None;
            return false;
        }
        let minute = now / 60;
        let changed = self
            .sidebar_tree_minute
            .is_some_and(|previous| previous != minute);
        self.sidebar_tree_minute = Some(minute);
        changed
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

    /// Navigate mode moves through AGORA and PENDENTE rows, band headers,
    /// expanded band rows and PARADOS rows in visual order.
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
        // Only a band row (an agent listed under its header) climbs to the header.
        let in_band = self.navigate_tree_item.as_ref().is_some_and(|cursor| {
            tree_rows(snapshot, &self.sidebar_tree).iter().any(|row| {
                matches!(row, TreeRow::Agent { .. }) && row.cursor().as_ref() == Some(cursor)
            })
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

    /// Enter on an agent row (AGORA, PENDENTE or band) focuses that agent, like
    /// a click.
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
