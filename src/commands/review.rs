use std::env;
use std::io::{self, Stdout};
use std::path::Path;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Terminal;
use rusqlite::Connection;

use crate::db::{self, LineComment, ReviewItem};
use crate::error::{system, CliResult};

type Tui = Terminal<CrosstermBackend<Stdout>>;

const DECISION_KEYS: &[(char, &str)] = &[
    ('1', "compliant"),
    ('2', "violation"),
    ('3', "false_positive"),
    ('4', "needs_more_context"),
    ('5', "uncertain"),
];

#[derive(Clone, PartialEq)]
enum Mode {
    Normal,
    Comment,
    Rationale(String),
}

struct App {
    items: Vec<ReviewItem>,
    idx: usize,
    selected_line: usize,
    code_scroll: usize,
    context_scroll: u16,
    mode: Mode,
    input: String,
    message: String,
    comments: Vec<LineComment>,
    reviewer: Option<String>,
    should_quit: bool,
}

/// Resolve `file_path` to an absolute, symlink-resolved path suitable for
/// copy/pasting into a shell or editor. Falls back to the stored path
/// as-is if it can't be canonicalized (e.g. gavel is running somewhere
/// other than the checkout the finding was generated from).
fn display_file_path(file_path: &str) -> String {
    std::fs::canonicalize(file_path)
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| file_path.to_string())
}

/// Expand tabs to spaces at fixed tab stops. Without this, a source line's
/// on-screen width as the terminal actually renders it (which expands tabs
/// to the next stop) diverges from what ratatui's buffer thinks the line's
/// width is (it counts a tab as a single cell) — so when the following
/// frame's content is shorter, ratatui only clears cells up to where it
/// believes the line ended, leaving the terminal's wider previous rendering
/// visible past that point. CERT-C source is tab-indented often enough that
/// this shows up as soon as line 1.
fn expand_tabs(s: &str, tab_width: usize) -> String {
    let mut out = String::with_capacity(s.len());
    let mut col = 0usize;
    for ch in s.chars() {
        if ch == '\t' {
            let spaces = tab_width - (col % tab_width);
            out.push_str(&" ".repeat(spaces));
            col += spaces;
        } else {
            out.push(ch);
            col += 1;
        }
    }
    out
}

pub fn run(db_path: &Path, id: Option<&str>, reviewer: Option<&str>) -> CliResult<()> {
    let conn = db::open(db_path)?;
    db::require_initialized(&conn)?;

    let items = match id {
        Some(raw) => vec![db::resolve_one(&conn, raw)?],
        None => {
            let mut pending = db::list_items(&conn, Some("pending"))?;
            let mut in_review = db::list_items(&conn, Some("in_review"))?;
            pending.append(&mut in_review);
            pending
        }
    };

    if items.is_empty() {
        println!("nothing to review — no pending or in_review items (try `gavel import` first)");
        return Ok(());
    }

    let reviewer = reviewer
        .map(|s| s.to_string())
        .or_else(|| env::var("USER").ok());

    let mut app = App {
        items,
        idx: 0,
        selected_line: 0,
        code_scroll: 0,
        context_scroll: 0,
        mode: Mode::Normal,
        input: String::new(),
        message: String::new(),
        comments: Vec::new(),
        reviewer,
        should_quit: false,
    };
    load_comments(&conn, &mut app)?;
    ensure_in_review(&conn, &mut app)?;

    let mut terminal = setup_terminal()?;
    let result = run_app(&mut terminal, &conn, &mut app);
    teardown_terminal(&mut terminal)?;
    result
}

fn setup_terminal() -> CliResult<Tui> {
    enable_raw_mode().map_err(|e| system(format!("enable raw mode failed: {e}")))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|e| system(format!("enter alternate screen failed: {e}")))?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).map_err(|e| system(format!("terminal init failed: {e}")))
}

fn teardown_terminal(terminal: &mut Tui) -> CliResult<()> {
    disable_raw_mode().map_err(|e| system(format!("disable raw mode failed: {e}")))?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .map_err(|e| system(format!("leave alternate screen failed: {e}")))?;
    terminal
        .show_cursor()
        .map_err(|e| system(format!("show cursor failed: {e}")))
}

fn load_comments(conn: &Connection, app: &mut App) -> CliResult<()> {
    let item_id = app.items[app.idx].id.clone();
    app.comments = db::load_comments(conn, &item_id)?;
    Ok(())
}

fn ensure_in_review(conn: &Connection, app: &mut App) -> CliResult<()> {
    let item = &mut app.items[app.idx];
    if item.status == "pending" {
        db::set_status(conn, &item.id, "in_review")?;
        item.status = "in_review".to_string();
    }
    Ok(())
}

fn goto(conn: &Connection, app: &mut App, new_idx: usize) -> CliResult<()> {
    if new_idx >= app.items.len() {
        app.should_quit = true;
        return Ok(());
    }
    app.idx = new_idx;
    app.selected_line = 0;
    app.code_scroll = 0;
    app.context_scroll = 0;
    app.mode = Mode::Normal;
    app.input.clear();
    load_comments(conn, app)?;
    ensure_in_review(conn, app)?;
    Ok(())
}

fn run_app(terminal: &mut Tui, conn: &Connection, app: &mut App) -> CliResult<()> {
    loop {
        terminal
            .draw(|f| draw(f, app))
            .map_err(|e| system(format!("draw failed: {e}")))?;

        if app.should_quit {
            return Ok(());
        }

        let ev = event::read().map_err(|e| system(format!("event read failed: {e}")))?;
        let Event::Key(key) = ev else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match app.mode.clone() {
            Mode::Normal => handle_normal_key(conn, app, key.code)?,
            Mode::Comment => handle_comment_key(conn, app, key.code)?,
            Mode::Rationale(decision) => handle_rationale_key(conn, app, key.code, &decision)?,
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn code_line_count(app: &App) -> usize {
    app.items[app.idx].code.lines().count().max(1)
}

fn handle_normal_key(conn: &Connection, app: &mut App, code: KeyCode) -> CliResult<()> {
    match code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('j') | KeyCode::Down => {
            app.message.clear();
            let max = code_line_count(app).saturating_sub(1);
            if app.selected_line < max {
                app.selected_line += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.message.clear();
            app.selected_line = app.selected_line.saturating_sub(1);
        }
        KeyCode::PageDown => {
            app.message.clear();
            app.context_scroll = app.context_scroll.saturating_add(5);
        }
        KeyCode::PageUp => {
            app.message.clear();
            app.context_scroll = app.context_scroll.saturating_sub(5);
        }
        KeyCode::Char('c') => {
            app.mode = Mode::Comment;
            app.input.clear();
        }
        KeyCode::Char('n') => {
            app.message = "skipped".to_string();
            goto(conn, app, app.idx + 1)?;
        }
        KeyCode::Char('p') => {
            if app.idx > 0 {
                goto(conn, app, app.idx - 1)?;
            } else {
                app.message = "already at first item".to_string();
            }
        }
        KeyCode::Char(c) => {
            if let Some((_, decision)) = DECISION_KEYS.iter().find(|(k, _)| *k == c) {
                app.mode = Mode::Rationale(decision.to_string());
                app.input.clear();
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_comment_key(conn: &Connection, app: &mut App, code: KeyCode) -> CliResult<()> {
    match code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            app.input.clear();
        }
        KeyCode::Enter => {
            if !app.input.trim().is_empty() {
                let item_id = app.items[app.idx].id.clone();
                let line_number = app.selected_line as i64 + 1;
                let file_line = app.items[app.idx].start_line + app.selected_line as i64;
                db::add_comment(conn, &item_id, line_number, app.input.trim())?;
                load_comments(conn, app)?;
                app.message = format!("comment added on line {file_line}");
            }
            app.mode = Mode::Normal;
            app.input.clear();
        }
        KeyCode::Backspace => {
            app.input.pop();
        }
        KeyCode::Char(c) => app.input.push(c),
        _ => {}
    }
    Ok(())
}

fn handle_rationale_key(
    conn: &Connection,
    app: &mut App,
    code: KeyCode,
    decision: &str,
) -> CliResult<()> {
    match code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            app.input.clear();
        }
        KeyCode::Enter => {
            let item_id = app.items[app.idx].id.clone();
            let rationale = if app.input.trim().is_empty() {
                None
            } else {
                Some(app.input.trim())
            };
            db::upsert_verdict(conn, &item_id, decision, rationale, app.reviewer.as_deref())?;
            app.items[app.idx].status = "adjudicated".to_string();
            app.message = format!("verdict '{decision}' saved");
            goto(conn, app, app.idx + 1)?;
        }
        KeyCode::Backspace => {
            app.input.pop();
        }
        KeyCode::Char(c) => app.input.push(c),
        _ => {}
    }
    Ok(())
}

fn draw(f: &mut ratatui::Frame, app: &mut App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(10),
            Constraint::Length(7),
            Constraint::Length(1),
        ])
        .split(f.area());

    let item = &app.items[app.idx];

    // Header — line 1 is title/rule/progress, line 2 is the file path
    // (full, canonicalized where possible) on its own plain line so it can
    // be selected/copied whole regardless of terminal width.
    let path_line = match &item.file_path {
        Some(fp) => format!("path: {}:{}", display_file_path(fp), item.start_line),
        None => "path: (no file_path recorded on import)".to_string(),
    };
    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                item.title.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(item.rule_id.clone(), Style::default().fg(Color::Yellow)),
            Span::raw(format!(
                "   item {} of {}   [{}]",
                app.idx + 1,
                app.items.len(),
                item.status
            )),
        ]),
        Line::from(Span::styled(
            path_line,
            Style::default().fg(Color::DarkGray),
        )),
    ])
    .block(Block::default().borders(Borders::ALL).title("gavel review"));
    f.render_widget(header, outer[0]);

    // Main split: code | rule/context
    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(outer[1]);

    let code_lines: Vec<Line> = item
        .code
        .lines()
        .enumerate()
        .map(|(i, l)| {
            let lineno = item.start_line + i as i64;
            let style = if i == app.selected_line {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else {
                Style::default()
            };
            let expanded = expand_tabs(l, 8);
            Line::from(Span::styled(format!("{lineno:>6} | {expanded}"), style))
        })
        .collect();

    // Keep the selected line in view. No wrapping on this pane (below) so
    // one logical source line is always exactly one screen row, which is
    // what makes a plain line-count scroll offset correct here.
    let visible_height = main[0].height.saturating_sub(2) as usize;
    let total_lines = code_lines.len();
    if visible_height > 0 {
        if app.selected_line < app.code_scroll {
            app.code_scroll = app.selected_line;
        } else if app.selected_line >= app.code_scroll + visible_height {
            app.code_scroll = app.selected_line + 1 - visible_height;
        }
        let max_scroll = total_lines.saturating_sub(visible_height);
        if app.code_scroll > max_scroll {
            app.code_scroll = max_scroll;
        }
    }

    let code = Paragraph::new(code_lines)
        .block(
            Block::default().borders(Borders::ALL).title(
                item.file_path
                    .clone()
                    .unwrap_or_else(|| item.language.clone()),
            ),
        )
        .scroll((app.code_scroll as u16, 0));
    f.render_widget(code, main[0]);

    let mut context_text = String::new();
    if let Some(rt) = &item.rule_text {
        context_text.push_str(&expand_tabs(rt, 8));
        context_text.push_str("\n\n");
    }
    context_text.push_str(&expand_tabs(
        item.context.as_deref().unwrap_or("(no context provided)"),
        8,
    ));
    let context = Paragraph::new(context_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("rule / context"),
        )
        .wrap(Wrap { trim: false })
        .scroll((app.context_scroll, 0));
    f.render_widget(context, main[1]);

    // Comments / input pane
    let bottom_title = match &app.mode {
        Mode::Comment => format!(
            "add comment on line {} (Enter=save, Esc=cancel)",
            item.start_line + app.selected_line as i64
        ),
        Mode::Rationale(decision) => {
            format!("verdict: {decision} — rationale (Enter=save, Esc=cancel)")
        }
        Mode::Normal => "comments".to_string(),
    };
    let mut lines: Vec<Line> = app
        .comments
        .iter()
        .map(|c| {
            let file_line = item.start_line + c.line_number - 1;
            Line::from(format!("  line {file_line}: {}", c.comment))
        })
        .collect();
    if lines.is_empty() && app.mode == Mode::Normal {
        lines.push(Line::from("  (none yet)"));
    }
    if !matches!(app.mode, Mode::Normal) {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("> {}", app.input),
            Style::default().fg(Color::Cyan),
        )));
    }
    // Keep the tail of the pane visible — in particular the blank spacer +
    // input line pushed on above, which is otherwise the first thing to
    // scroll out of view once there are enough existing comments to fill
    // the pane on their own (no scroll was applied here at all before,
    // so the input line silently became invisible while still accepting
    // keystrokes).
    let bottom_visible_height = outer[2].height.saturating_sub(2) as usize;
    let bottom_scroll = (lines.len().saturating_sub(bottom_visible_height)) as u16;
    let comments = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(bottom_title))
        .wrap(Wrap { trim: false })
        .scroll((bottom_scroll, 0));
    f.render_widget(comments, outer[2]);

    // Help / status bar
    let help = if app.message.is_empty() {
        let decisions: String = DECISION_KEYS
            .iter()
            .map(|(k, d)| format!("{k} {d}"))
            .collect::<Vec<_>>()
            .join("  ");
        format!("j/k move  c comment  {decisions}  n skip  p prev  q quit")
    } else {
        app.message.clone()
    };
    let help_bar = Paragraph::new(help).style(Style::default().fg(Color::Black).bg(Color::Gray));
    f.render_widget(help_bar, outer[3]);
}
