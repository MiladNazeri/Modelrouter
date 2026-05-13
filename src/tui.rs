use std::{
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    time::Duration,
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Tabs, Wrap},
};

use crate::{
    ProviderHealth, ProviderId, RequestLogEntry, RouteDecision, RouteRequest, Router, RouterConfig,
    RouterMetrics, TaskHint, append_request_log, apply_project_profile,
    availability_filtered_config, health_report, load_metrics, metrics_from_logs,
    run_provider_with_usage,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TuiView {
    Route,
    Providers,
    Config,
    Projects,
    History,
    Help,
}

impl TuiView {
    fn title(self) -> &'static str {
        match self {
            Self::Route => "Route",
            Self::Providers => "Providers",
            Self::Config => "Config",
            Self::Projects => "Projects",
            Self::History => "History",
            Self::Help => "Help",
        }
    }

    fn all() -> &'static [Self] {
        &[
            Self::Route,
            Self::Providers,
            Self::Config,
            Self::Projects,
            Self::History,
            Self::Help,
        ]
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TuiEvent {
    Insert(char),
    Backspace,
    Newline,
    CycleProvider,
    CycleHint,
    NextView,
    PreviousView,
}

#[derive(Clone, Debug)]
pub struct TuiState {
    prompt: String,
    status: String,
    view_index: usize,
    provider_index: usize,
    hint_index: usize,
    cwd: Option<PathBuf>,
    last_decision: Option<RouteDecision>,
    last_output: String,
}

impl Default for TuiState {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            status:
                "Ready. Type a prompt. Ctrl-R routes, Ctrl-X runs, Tab switches panels, Esc quits."
                    .to_string(),
            view_index: 0,
            provider_index: 0,
            hint_index: 0,
            cwd: None,
            last_decision: None,
            last_output: String::new(),
        }
    }
}

impl TuiState {
    pub fn apply(&mut self, event: TuiEvent) {
        match event {
            TuiEvent::Insert(value) => self.prompt.push(value),
            TuiEvent::Backspace => {
                self.prompt.pop();
            }
            TuiEvent::Newline => self.prompt.push('\n'),
            TuiEvent::CycleProvider => {
                self.provider_index = (self.provider_index + 1) % provider_choices().len();
            }
            TuiEvent::CycleHint => {
                self.hint_index = (self.hint_index + 1) % hint_choices().len();
            }
            TuiEvent::NextView => {
                self.view_index = (self.view_index + 1) % TuiView::all().len();
            }
            TuiEvent::PreviousView => {
                self.view_index =
                    (self.view_index + TuiView::all().len() - 1) % TuiView::all().len();
            }
        }
    }

    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn view(&self) -> TuiView {
        TuiView::all()[self.view_index]
    }

    pub fn prefer(&self) -> Option<ProviderId> {
        provider_choices()[self.provider_index].1
    }

    pub fn hint(&self) -> Option<TaskHint> {
        hint_choices()[self.hint_index].1
    }

    pub fn record_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }

    fn provider_label(&self) -> &'static str {
        provider_choices()[self.provider_index].0
    }

    fn hint_label(&self) -> &'static str {
        hint_choices()[self.hint_index].0
    }
}

pub fn run_tui(
    config: RouterConfig,
    log_path: Option<PathBuf>,
    cwd: Option<PathBuf>,
) -> anyhow::Result<()> {
    if !io::stdout().is_terminal() {
        anyhow::bail!("modelrouter tui requires an interactive terminal");
    }

    let mut state = TuiState {
        cwd,
        ..TuiState::default()
    };
    let mut health = health_report(&config);
    let mut metrics = load_or_empty_metrics(log_path.as_deref());

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_tui_loop(
        &mut terminal,
        &config,
        log_path.as_deref(),
        &mut state,
        &mut health,
        &mut metrics,
    );
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run_tui_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    config: &RouterConfig,
    log_path: Option<&Path>,
    state: &mut TuiState,
    health: &mut Vec<ProviderHealth>,
    metrics: &mut RouterMetrics,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, state, config, health, metrics))?;
        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
            && handle_key(key, config, log_path, state, health, metrics)?
        {
            break;
        }
    }
    Ok(())
}

fn handle_key(
    key: KeyEvent,
    config: &RouterConfig,
    log_path: Option<&Path>,
    state: &mut TuiState,
    health: &mut Vec<ProviderHealth>,
    metrics: &mut RouterMetrics,
) -> anyhow::Result<bool> {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(true),
        KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(true),
        KeyCode::Esc => return Ok(true),
        KeyCode::Tab => state.apply(TuiEvent::NextView),
        KeyCode::BackTab => state.apply(TuiEvent::PreviousView),
        KeyCode::F(2) => state.apply(TuiEvent::CycleProvider),
        KeyCode::F(3) => state.apply(TuiEvent::CycleHint),
        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            route_prompt(config, log_path, state, health, metrics)?;
        }
        KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            run_prompt(config, log_path, state, health, metrics)?;
        }
        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            *health = health_report(config);
            *metrics = load_or_empty_metrics(log_path);
            state.record_status("Refreshed health and metrics.");
        }
        KeyCode::Backspace => state.apply(TuiEvent::Backspace),
        KeyCode::Enter => state.apply(TuiEvent::Newline),
        KeyCode::Char(value) => state.apply(TuiEvent::Insert(value)),
        _ => {}
    }
    Ok(false)
}

fn route_prompt(
    config: &RouterConfig,
    log_path: Option<&Path>,
    state: &mut TuiState,
    health: &[ProviderHealth],
    metrics: &mut RouterMetrics,
) -> anyhow::Result<()> {
    if state.prompt.trim().is_empty() {
        state.record_status("Prompt is empty.");
        return Ok(());
    }
    let (profiled, request) = tui_route_request(config, state)?;
    let filtered = availability_filtered_config(&profiled, health);
    let decision = Router::new(filtered).route(request)?;
    append_tui_log(log_path, "route", state.prompt(), &decision, true, None)?;
    state.last_decision = Some(decision.clone());
    state.last_output.clear();
    state.record_status(format!(
        "Routed to {} / {} for {:.6} estimated cents.",
        decision.provider, decision.model, decision.estimated_cost_cents
    ));
    *metrics = load_or_empty_metrics(log_path);
    Ok(())
}

fn run_prompt(
    config: &RouterConfig,
    log_path: Option<&Path>,
    state: &mut TuiState,
    health: &[ProviderHealth],
    metrics: &mut RouterMetrics,
) -> anyhow::Result<()> {
    if state.prompt.trim().is_empty() {
        state.record_status("Prompt is empty.");
        return Ok(());
    }
    let (profiled, request) = tui_route_request(config, state)?;
    let filtered = availability_filtered_config(&profiled, health);
    let decision = Router::new(filtered).route(request)?;
    let provider =
        profiled
            .provider(decision.provider)
            .ok_or(crate::ProviderRunError::MissingProvider {
                provider: decision.provider,
            })?;
    let result = run_provider_with_usage(provider, state.prompt(), state.cwd.as_deref())?;
    if let Some(log_path) = log_path {
        let entry = match result.usage {
            Some(usage) => RequestLogEntry::from_decision_with_actual_usage(
                "run",
                state.prompt(),
                &decision,
                usage,
                result.actual_cost_cents,
                0,
            ),
            None => RequestLogEntry::from_decision("run", state.prompt(), &decision, 0, true, None),
        };
        append_request_log(log_path, &entry)?;
    }
    state.last_decision = Some(decision.clone());
    state.last_output = result.output;
    state.record_status(format!("Ran {} / {}.", decision.provider, decision.model));
    *metrics = load_or_empty_metrics(log_path);
    Ok(())
}

fn tui_route_request(
    config: &RouterConfig,
    state: &TuiState,
) -> anyhow::Result<(RouterConfig, RouteRequest)> {
    let cwd = state.cwd.clone().unwrap_or(std::env::current_dir()?);
    let profiled = apply_project_profile(config, &cwd);
    let mut request = RouteRequest::new(state.prompt.clone());
    if let Some(provider) = state.prefer() {
        request = request.prefer(provider);
    }
    if let Some(hint) = state.hint() {
        request = request.with_hint(hint);
    }
    Ok((profiled, request))
}

fn append_tui_log(
    log_path: Option<&Path>,
    operation: &str,
    prompt: &str,
    decision: &RouteDecision,
    success: bool,
    error: Option<String>,
) -> anyhow::Result<()> {
    if let Some(log_path) = log_path {
        append_request_log(
            log_path,
            &RequestLogEntry::from_decision(operation, prompt, decision, 0, success, error),
        )?;
    }
    Ok(())
}

fn draw(
    frame: &mut ratatui::Frame<'_>,
    state: &TuiState,
    config: &RouterConfig,
    health: &[ProviderHealth],
    metrics: &RouterMetrics,
) {
    let area = frame.area();
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);
    draw_header(frame, vertical[0]);
    draw_tabs(frame, vertical[1], state);
    match state.view() {
        TuiView::Route => draw_route(frame, vertical[2], state, metrics),
        TuiView::Providers => draw_providers(frame, vertical[2], config, health),
        TuiView::Config => draw_config(frame, vertical[2], config),
        TuiView::Projects => draw_projects(frame, vertical[2], config),
        TuiView::History => draw_history(frame, vertical[2], metrics),
        TuiView::Help => draw_help(frame, vertical[2]),
    }
    frame.render_widget(
        Paragraph::new(state.status.as_str())
            .style(Style::default().fg(Color::Cyan))
            .block(Block::default().borders(Borders::ALL).title("Status")),
        vertical[3],
    );
}

fn draw_header(frame: &mut ratatui::Frame<'_>, area: Rect) {
    frame.render_widget(
        Paragraph::new(
            "Modelrouter TUI  |  Esc quit  Tab panels  F2 provider  F3 hint  Ctrl-R route  Ctrl-X run  Ctrl-L refresh",
        )
        .style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn draw_tabs(frame: &mut ratatui::Frame<'_>, area: Rect, state: &TuiState) {
    let titles = TuiView::all()
        .iter()
        .map(|view| Line::from(Span::raw(view.title())))
        .collect::<Vec<_>>();
    frame.render_widget(
        Tabs::new(titles)
            .select(state.view_index)
            .highlight_style(Style::default().fg(Color::Cyan))
            .block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn draw_route(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    state: &TuiState,
    metrics: &RouterMetrics,
) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(area);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(8),
            Constraint::Min(8),
        ])
        .split(columns[0]);
    let controls = format!(
        "provider: {}    hint: {}    cwd: {}",
        state.provider_label(),
        state.hint_label(),
        state.cwd.as_ref().map_or_else(
            || "current directory".to_string(),
            |cwd| cwd.display().to_string()
        )
    );
    frame.render_widget(
        Paragraph::new(controls).block(Block::default().borders(Borders::ALL).title("Controls")),
        left[0],
    );
    frame.render_widget(
        Paragraph::new(state.prompt.as_str())
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title("Prompt")),
        left[1],
    );
    frame.render_widget(
        Paragraph::new(state.last_output.as_str())
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title("Last Output")),
        left[2],
    );
    let decision = state
        .last_decision
        .as_ref()
        .map(|decision| {
            let reasons = decision
                .reasons
                .iter()
                .map(|reason| format!("- {reason}"))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "provider: {}\nmodel: {}\nbilling: {:?}\nestimated cost: {:.6} cents\nconfidence: {:.2}\n\n{}",
                decision.provider,
                decision.model,
                decision.billing,
                decision.estimated_cost_cents,
                decision.confidence,
                reasons
            )
        })
        .unwrap_or_else(|| "No route decision yet.".to_string());
    let side = format!(
        "{decision}\n\nMetrics\nrequests: {}\nsuccess: {}\nfailed: {}\nestimated cents: {:.6}",
        metrics.total_requests,
        metrics.successful_requests,
        metrics.failed_requests,
        metrics.estimated_cost_cents
    );
    frame.render_widget(
        Paragraph::new(side)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title("Decision")),
        columns[1],
    );
}

fn draw_providers(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    config: &RouterConfig,
    health: &[ProviderHealth],
) {
    let rows = config.providers.iter().map(|provider| {
        let status = health
            .iter()
            .find(|item| item.provider == provider.id)
            .map_or_else(
                || "not checked".to_string(),
                |item| format!("{:?}", item.status),
            );
        Row::new(vec![
            Cell::from(provider.id.to_string()),
            Cell::from(if provider.enabled { "yes" } else { "no" }),
            Cell::from(provider.model.clone()),
            Cell::from(format!("{:?}", provider.billing)),
            Cell::from(status),
        ])
    });
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(18),
                Constraint::Length(8),
                Constraint::Length(28),
                Constraint::Length(14),
                Constraint::Min(12),
            ],
        )
        .header(Row::new([
            "Provider", "Enabled", "Model", "Billing", "Status",
        ]))
        .block(Block::default().borders(Borders::ALL).title("Providers")),
        area,
    );
}

fn draw_config(frame: &mut ratatui::Frame<'_>, area: Rect, config: &RouterConfig) {
    let text = toml::to_string_pretty(config).unwrap_or_else(|error| error.to_string());
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Config Preview"),
        ),
        area,
    );
}

fn draw_projects(frame: &mut ratatui::Frame<'_>, area: Rect, config: &RouterConfig) {
    let rows = config.profiles.iter().map(|profile| {
        Row::new(vec![
            Cell::from(profile.name.clone()),
            Cell::from(profile.path_contains.clone()),
            Cell::from(
                profile
                    .default_provider
                    .map_or_else(String::new, |provider| provider.to_string()),
            ),
            Cell::from(
                profile
                    .code_provider
                    .map_or_else(String::new, |provider| provider.to_string()),
            ),
        ])
    });
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(24),
                Constraint::Length(42),
                Constraint::Length(16),
                Constraint::Min(16),
            ],
        )
        .header(Row::new(["Name", "Path Contains", "Default", "Code"]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Project Profiles"),
        ),
        area,
    );
}

fn draw_history(frame: &mut ratatui::Frame<'_>, area: Rect, metrics: &RouterMetrics) {
    let rows = metrics.by_provider.iter().map(|provider| {
        Row::new(vec![
            Cell::from(provider.provider.clone()),
            Cell::from(provider.requests.to_string()),
            Cell::from(format!("{:.6}", provider.estimated_cost_cents)),
        ])
    });
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(24),
                Constraint::Length(12),
                Constraint::Min(16),
            ],
        )
        .header(Row::new(["Provider", "Requests", "Estimated cents"]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Spend And History Summary"),
        ),
        area,
    );
}

fn draw_help(frame: &mut ratatui::Frame<'_>, area: Rect) {
    let help = [
        "This is the interactive Modelrouter control plane.",
        "",
        "Keys:",
        "  tab / shift-tab: switch panels",
        "  F2: cycle provider preference",
        "  F3: cycle task hint",
        "  Ctrl-R: route without running",
        "  Ctrl-X: run selected route",
        "  Ctrl-L: refresh health and metrics",
        "  Esc or Ctrl-Q: quit",
        "",
        "GUI control plane:",
        "  http://127.0.0.1:8787",
        "",
        "Scriptable CLI remains available:",
        "  modelrouter route --prompt \"...\" --json",
        "  modelrouter run --prompt \"...\"",
    ]
    .join("\n");
    frame.render_widget(
        Paragraph::new(help)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title("Help")),
        area,
    );
}

fn load_or_empty_metrics(log_path: Option<&Path>) -> RouterMetrics {
    match log_path {
        Some(path) => load_metrics(path).unwrap_or_else(|_| metrics_from_logs(&[])),
        None => metrics_from_logs(&[]),
    }
}

fn provider_choices() -> &'static [(&'static str, Option<ProviderId>)] {
    &[
        ("auto", None),
        ("local", Some(ProviderId::Local)),
        ("claude", Some(ProviderId::Claude)),
        ("codex", Some(ProviderId::Codex)),
        ("gemini", Some(ProviderId::Gemini)),
        ("lmstudio", Some(ProviderId::LmStudio)),
        ("llamacpp", Some(ProviderId::LlamaCpp)),
        ("openai_compatible", Some(ProviderId::OpenAiCompatible)),
        ("aider", Some(ProviderId::Aider)),
    ]
}

fn hint_choices() -> &'static [(&'static str, Option<TaskHint>)] {
    &[
        ("auto", None),
        ("simple", Some(TaskHint::Simple)),
        ("code", Some(TaskHint::Code)),
        ("deep_reasoning", Some(TaskHint::DeepReasoning)),
        ("writing", Some(TaskHint::Writing)),
    ]
}
