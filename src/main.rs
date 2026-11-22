use std::{collections::BTreeSet, io, time::Duration};

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Terminal,
};
use scraper::{Html, Selector};

const BASE_URL: &str = "https://stats.swehockey.se";
const SHL_TEAM_COUNT: usize = 14;

#[derive(Clone)]
struct Record(Vec<String>);

struct Data {
    games: Vec<Record>,
    players: Vec<Record>,
    standings: Vec<Record>,
    season: String,
}

fn fetch_table(url: &str, preferred: &[&str]) -> Result<(String, Vec<Record>)> {
    let client = http_client()?;
    let body = client.get(url).send()?.error_for_status()?.text()?;
    let document = Html::parse_document(&body);
    let title_sel = Selector::parse("h1, h2").unwrap();
    let title = document.select(&title_sel).map(|e| e.text().collect::<String>())
        .find(|s| s.to_lowercase().contains("shl")).unwrap_or_else(|| "SHL".into());
    let table_sel = Selector::parse("table").unwrap();
    let row_sel = Selector::parse("tr").unwrap();
    let cell_sel = Selector::parse("th, td").unwrap();
    let mut candidates = Vec::new();
    for table in document.select(&table_sel) {
        let rows: Vec<Record> = table.select(&row_sel).map(|row| Record(row.select(&cell_sel)
            .map(|cell| cell.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" "))
            .collect())).filter(|r| !r.0.is_empty()).collect();
        if rows.len() > 1 { candidates.push(rows); }
    }
    let rows = candidates.into_iter().enumerate().max_by_key(|(index, rows)| {
        let header = rows[0].0.join(" ").to_lowercase();
        (preferred.iter().filter(|word| header.contains(**word)).count(), std::cmp::Reverse(*index))
    }).map(|(_, rows)| rows).context("Hittade ingen statistiktabell på sidan")?;
    let records = rows.into_iter().skip(1).filter(|r| !r.0.is_empty()).collect();
    Ok((title, records))
}

fn http_client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent("shl_stats/0.1 (terminal statistics viewer)")
        .timeout(Duration::from_secs(20))
        .build()?)
}

fn current_shl_id() -> Result<String> {
    let body = http_client()?.get(BASE_URL).send()?.error_for_status()?.text()?;
    let document = Html::parse_document(&body);
    let links = Selector::parse("a[href*='/ScheduleAndResults/Overview/']").unwrap();
    document.select(&links)
        .find_map(|link| {
            let label = link.text().collect::<String>().trim().to_lowercase();
            if label != "shl" { return None; }
            link.value().attr("href")?.rsplit('/').next().map(str::to_owned)
        })
        .context("Kunde inte hitta aktuell SHL-säsong på stats.swehockey.se")
}

fn fetch_data() -> Result<Data> {
    let season_id = current_shl_id()?;
    let games_url = format!("{BASE_URL}/ScheduleAndResults/Overview/{season_id}");
    let players_url = format!("{BASE_URL}/Players/Statistics/ScoringLeaders/{season_id}");
    let standings_url = format!("{BASE_URL}/ScheduleAndResults/Standings/{season_id}");
    let (_, all_games) = fetch_table(&games_url, &["date", "game", "result"])
        .context("Kunde inte hämta matchinformation")?;
    let games = latest_and_upcoming_rounds(all_games);
    let (_, raw_players) = fetch_table(&players_url, &["name", "team", "gp", "tp"])
        .context("Kunde inte hämta spelarstatistik")?;
    let players = normalize_players(raw_players);
    let (_, standings) = fetch_table(&standings_url, &["rk", "team", "gp", "gd", "tp"])
        .context("Kunde inte hämta SHL-tabellen")?;
    // Standings-sidan innehåller även hemma, borta och senaste matcher.
    // Den första kompletta rankingen är totalställningen för grundserien.
    let standings = standings.into_iter()
        .filter(|row| row.0.len() >= 8 && row.0[0].parse::<usize>().is_ok())
        .take(SHL_TEAM_COUNT)
        .collect();
    Ok(Data { games, players, standings, season: "SHL • aktuell säsong".into() })
}

fn normalize_players(rows: Vec<Record>) -> Vec<Record> {
    rows.into_iter().filter_map(|row| {
        if row.0.len() < 8 || row.0[0].parse::<usize>().is_err() { return None; }
        let team = match row.0[2].as_str() {
            "BIF" => "Brynäs IF", "DIF" => "Djurgårdens IF", "FRÖ" => "Frölunda HC",
            "FBK" => "Färjestad BK", "HV71" => "HV 71", "IFB" => "IF Björklöven",
            "MIF" => "IF Malmö Redhawks", "LHC" => "Linköping HC", "LHF" => "Luleå HF",
            "RBK" => "Rögle BK", "SKE" => "Skellefteå AIK", "TIK" => "Timrå IK",
            "VÄX" => "Växjö Lakers HC", "ÖHK" => "Örebro HK", code => code,
        };
        Some(Record(vec![row.0[0].clone(), row.0[1].clone(), team.to_owned(),
            row.0[4].clone(), row.0[5].clone(), row.0[6].clone(), row.0[7].clone()]))
    }).collect()
}

fn is_date(value: &str) -> bool {
    let date = value.get(..10).unwrap_or("");
    date.len() == 10
        && date.as_bytes()[4] == b'-'
        && date.as_bytes()[7] == b'-'
        && date.chars().enumerate().all(|(i, c)| matches!(i, 4 | 7) || c.is_ascii_digit())
}

fn is_score(value: &str) -> bool {
    let Some((home, away)) = value.split_once('-') else { return false; };
    home.trim().parse::<u16>().is_ok() && away.trim().parse::<u16>().is_ok()
}

fn latest_and_upcoming_rounds(rows: Vec<Record>) -> Vec<Record> {
    let mut matches = Vec::new();
    let mut played_dates = BTreeSet::new();
    let mut future_dates = BTreeSet::new();

    for row in rows {
        if row.0.len() < 3 || !is_date(&row.0[0]) { continue; }
        let date = row.0[0].chars().take(10).collect::<String>();
        let score_index = row.0.iter().enumerate().skip(2).find(|(_, value)| is_score(value)).map(|(i, _)| i);
        let (score, details, played) = if let Some(index) = score_index {
            played_dates.insert(date.clone());
            (row.0[index].clone(), row.0.get(index + 1).cloned().unwrap_or_default(), true)
        } else {
            future_dates.insert(date.clone());
            ("—".to_owned(), row.0.last().cloned().unwrap_or_default(), false)
        };
        matches.push((date, played, Record(vec![row.0[0].clone(), row.0[1].clone(), score, details])));
    }

    let recent: BTreeSet<String> = played_dates.iter().rev().take(2).cloned().collect();
    let upcoming: BTreeSet<String> = future_dates.iter().take(2).cloned().collect();
    matches.into_iter()
        .filter(|(date, played, _)| if *played { recent.contains(date) } else { upcoming.contains(date) })
        .map(|(_, _, row)| row)
        .collect()
}

struct App { data: Data, tab: usize, selected: usize, sort: Option<(usize, bool)>, status: String }

fn run_ui(mut terminal: Terminal<CrosstermBackend<io::Stdout>>, data: Data) -> Result<()> {
    let mut app = App { data, tab: 0, selected: 0, sort: None, status: "↑/↓ navigera  •  Tab byt flik  •  g mål  •  p poäng  •  r uppdatera  •  q avsluta".into() };
    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default().direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(2)]).split(area);
            let tabs = ["MATCHER", "SPELARE", "SHL-TABELL"].iter().enumerate().map(|(i, label)| {
                if i == app.tab { format!("  [{label}]  ") } else { format!("   {label}   ") }
            }).collect::<String>();
            frame.render_widget(Paragraph::new(Line::from(vec![
                Span::styled(" SHL ", Style::default().fg(Color::Black).bg(Color::LightCyan).add_modifier(Modifier::BOLD)),
                Span::styled(format!("  {}  ", app.data.season), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::styled(tabs, Style::default().fg(Color::Cyan)),
            ])).block(Block::default().borders(Borders::BOTTOM)), chunks[0]);
            let (headers, records): (Vec<&str>, &Vec<Record>) = match app.tab {
                0 => (vec!["Datum", "Match", "Resultat", "Perioder / arena"], &app.data.games),
                1 => (vec!["#", "Spelare", "Lag", "GP", "G", "A", "P"], &app.data.players),
                _ => (vec!["#", "Lag", "SP", "V", "O", "F", "+/-", "P"], &app.data.standings),
            };
            let widths = match app.tab {
                0 => vec![Constraint::Length(18), Constraint::Min(35), Constraint::Length(12), Constraint::Min(20)],
                1 => vec![Constraint::Length(5), Constraint::Fill(1), Constraint::Length(22), Constraint::Length(5), Constraint::Length(5), Constraint::Length(5), Constraint::Length(5)],
                _ => vec![Constraint::Length(5), Constraint::Min(26), Constraint::Length(5), Constraint::Length(5), Constraint::Length(5), Constraint::Length(5), Constraint::Length(7), Constraint::Length(5)],
            };
            let rows = records.iter().map(|r| {
                Row::new(r.0.iter().cloned().map(Cell::from).collect::<Vec<_>>())
            });
            let title = match app.tab { 0 => " MATCHER · två senaste och två kommande omgångar ", 1 => " SPELARE · g mål · p poäng (sortera) ", _ => " SHL-TABELL · grundserien " };
            let table = Table::new(rows, widths).header(Row::new(headers).style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
                .block(Block::default().title(title).borders(Borders::ALL))
                .row_highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));
            let mut state = TableState::default();
            if !records.is_empty() { state.select(Some(app.selected.min(records.len() - 1))); }
            frame.render_stateful_widget(table, chunks[1], &mut state);
            frame.render_widget(Paragraph::new(app.status.clone()).style(Style::default().fg(Color::Gray)), chunks[2]);
        })?;
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press { continue; }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Tab | KeyCode::Right => { app.tab = (app.tab + 1) % 3; app.selected = 0; },
                    KeyCode::Left => { app.tab = (app.tab + 2) % 3; app.selected = 0; },
                    KeyCode::Down | KeyCode::Char('j') => {
                        let len = match app.tab { 0 => app.data.games.len(), 1 => app.data.players.len(), _ => app.data.standings.len() };
                        app.selected = app.selected.saturating_add(1).min(len.saturating_sub(1));
                    }
                    KeyCode::Up | KeyCode::Char('k') => app.selected = app.selected.saturating_sub(1),
                    KeyCode::Char('r') => match fetch_data() {
                        Ok(data) => { app.data = data; app.selected = 0; app.sort = None; app.status = "Data uppdaterad.".into(); },
                        Err(error) => app.status = format!("Uppdatering misslyckades: {error:#}"),
                    },
                    KeyCode::Char('g') | KeyCode::Char('p') if app.tab == 1 => {
                        let col = if key.code == KeyCode::Char('g') { 4 } else { 6 };
                        let descending = match app.sort { Some((old_col, desc)) if old_col == col => !desc, _ => true };
                        app.data.players.sort_by(|a, b| {
                            let av = a.0.get(col).and_then(|v| v.parse::<i32>().ok()).unwrap_or_default();
                            let bv = b.0.get(col).and_then(|v| v.parse::<i32>().ok()).unwrap_or_default();
                            if descending { bv.cmp(&av) } else { av.cmp(&bv) }
                        });
                        for (rank, row) in app.data.players.iter_mut().enumerate() {
                            if !row.0.is_empty() { row.0[0] = (rank + 1).to_string(); }
                        }
                        app.sort = Some((col, descending));
                        app.selected = 0;
                        app.status = format!("Sorterat {} {}. Tryck samma tangent igen för motsatt ordning.", if col == 5 { "på mål" } else { "på poäng" }, if descending { "fallande" } else { "stigande" });
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let data = fetch_data()?;
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    let result = run_ui(terminal, data);
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    result
}
