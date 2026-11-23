use std::{io, time::Duration};

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
};
use scraper::{ElementRef, Html, Selector};

const BASE_URL: &str = "https://stats.swehockey.se";
const SHL_TEAM_COUNT: usize = 14;

#[derive(Clone)]
struct Record(Vec<String>);

struct Data {
    games: Vec<Record>,
    games_boundary: usize,
    players: Vec<Record>,
    goalies: Vec<Record>,
    standings: Vec<Record>,
    season: String,
}

#[derive(Clone)]
struct Season {
    id: String,
    label: String,
}

fn fetch_table(url: &str, preferred: &[&str]) -> Result<(String, Vec<Record>)> {
    let client = http_client()?;
    let body = client.get(url).send()?.error_for_status()?.text()?;
    let document = Html::parse_document(&body);
    let title_sel = Selector::parse("h1, h2").unwrap();
    let title = document
        .select(&title_sel)
        .map(|e| e.text().collect::<String>())
        .find(|s| s.to_lowercase().contains("shl"))
        .unwrap_or_else(|| "SHL".into());
    let table_sel = Selector::parse("table").unwrap();
    let row_sel = Selector::parse("tr").unwrap();
    let cell_sel = Selector::parse("th, td").unwrap();
    let mut candidates = Vec::new();
    for table in document.select(&table_sel) {
        let rows: Vec<Record> = table
            .select(&row_sel)
            .map(|row| {
                Record(
                    row.select(&cell_sel)
                        .map(|cell| {
                            cell.text()
                                .collect::<String>()
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .collect(),
                )
            })
            .filter(|r| !r.0.is_empty())
            .collect();
        if rows.len() > 1 {
            candidates.push(rows);
        }
    }
    let rows = candidates
        .into_iter()
        .enumerate()
        .max_by_key(|(index, rows)| {
            let header = rows[0].0.join(" ").to_lowercase();
            (
                preferred
                    .iter()
                    .filter(|word| header.contains(**word))
                    .count(),
                std::cmp::Reverse(*index),
            )
        })
        .map(|(_, rows)| rows)
        .context("Hittade ingen statistiktabell på sidan")?;
    let records = rows
        .into_iter()
        .skip(1)
        .filter(|r| !r.0.is_empty())
        .collect();
    Ok((title, records))
}

fn http_client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent("shl_stats/0.1 (terminal statistics viewer)")
        .timeout(Duration::from_secs(20))
        .build()?)
}

fn current_shl_id() -> Result<String> {
    let body = http_client()?
        .get(BASE_URL)
        .send()?
        .error_for_status()?
        .text()?;
    let document = Html::parse_document(&body);
    let links = Selector::parse("a[href*='/ScheduleAndResults/Overview/']").unwrap();
    document
        .select(&links)
        .find_map(|link| {
            let label = link.text().collect::<String>().trim().to_lowercase();
            if label != "shl" {
                return None;
            }
            link.value()
                .attr("href")?
                .rsplit('/')
                .next()
                .map(str::to_owned)
        })
        .context("Kunde inte hitta aktuell SHL-säsong på stats.swehockey.se")
}

fn available_shl_seasons(current_id: &str) -> Result<Vec<Season>> {
    let url = format!("{BASE_URL}/ScheduleAndResults/Overview/{current_id}");
    let body = http_client()?.get(url).send()?.error_for_status()?.text()?;
    let document = Html::parse_document(&body);
    let options = Selector::parse("select option").unwrap();
    let mut seasons = Vec::new();
    for option in document.select(&options) {
        let label = option
            .text()
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !label.contains("SHL") || !label.chars().take(4).all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        let Some(value) = option.value().attr("value") else {
            continue;
        };
        let id = value
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(value);
        if !id.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        if !seasons.iter().any(|season: &Season| season.id == id) {
            seasons.push(Season {
                id: id.to_owned(),
                label,
            });
        }
    }
    // Swehockey's season selector is sometimes rendered without its options in the
    // returned HTML. Keep the known SHL overview IDs available in that case.
    for (id, label) in [
        ("20961", "2026-27 - SHL"),
        ("18263", "2025-26 - SHL"),
        ("15977", "2024-25 - SHL"),
        ("14677", "2023-24 - SHL"),
        ("13469", "2022-23 - SHL"),
        ("12318", "2021-22 - SHL"),
        ("11450", "2020-21 - SHL"),
    ] {
        if !seasons.iter().any(|season| season.id == id) {
            seasons.push(Season {
                id: id.into(),
                label: label.into(),
            });
        }
    }
    if !seasons.iter().any(|season| season.id == current_id) {
        seasons.push(Season {
            id: current_id.to_owned(),
            label: format!("Aktuell SHL ({current_id})"),
        });
    }
    seasons.sort_by(|a, b| b.label.cmp(&a.label));
    Ok(seasons)
}

fn fetch_data(season: &Season) -> Result<Data> {
    let season_id = &season.id;
    let games_url = format!("{BASE_URL}/ScheduleAndResults/Schedule/{season_id}");
    let players_url = format!("{BASE_URL}/Teams/Info/PlayersByTeam/{season_id}");
    let standings_url = format!("{BASE_URL}/ScheduleAndResults/Standings/{season_id}");
    let (_, all_games) = fetch_table(&games_url, &["date", "game", "result"])
        .context("Kunde inte hämta matchinformation")?;
    let (games, games_boundary) = all_season_games(all_games);
    let (_, standings) = fetch_table(&standings_url, &["rk", "team", "gp", "gd", "tp"])
        .context("Kunde inte hämta SHL-tabellen")?;
    // Standings-sidan innehåller även hemma, borta och senaste matcher.
    // Den första kompletta rankingen är totalställningen för grundserien.
    let standings = normalize_standings(standings);
    let team_names: Vec<String> = standings
        .iter()
        .filter_map(|row| row.0.get(1).cloned())
        .collect();
    let players = fetch_all_players(&players_url, &team_names)
        .context("Kunde inte hämta hela spelartruppen")?;
    let goalies =
        fetch_goalies(&players_url, &team_names).context("Kunde inte hämta målvaktsstatistik")?;
    Ok(Data {
        games,
        games_boundary,
        players,
        goalies,
        standings,
        season: season.label.clone(),
    })
}

fn fetch_all_players(url: &str, standings_teams: &[String]) -> Result<Vec<Record>> {
    let body = http_client()?.get(url).send()?.error_for_status()?.text()?;
    let document = Html::parse_document(&body);
    let table_sel = Selector::parse("table").unwrap();
    let row_sel = Selector::parse("tr").unwrap();
    let cell_sel = Selector::parse("th, td").unwrap();
    let team_set: std::collections::HashSet<&str> =
        standings_teams.iter().map(String::as_str).collect();
    let link_sel = Selector::parse("a").unwrap();
    let mut page_teams = Vec::new();
    for link in document.select(&link_sel) {
        let team = link.text().collect::<String>().trim().to_owned();
        if team_set.contains(team.as_str()) && !page_teams.contains(&team) {
            page_teams.push(team);
        }
    }
    let mut player_tables: Vec<Vec<Vec<String>>> = Vec::new();
    let mut first_players = std::collections::HashSet::new();
    for table in document.select(&table_sel) {
        let rows: Vec<Vec<String>> = table
            .select(&row_sel)
            .filter(|row| {
                row.ancestors()
                    .filter_map(ElementRef::wrap)
                    .find(|ancestor| ancestor.value().name() == "table")
                    .is_some_and(|ancestor| ancestor.id() == table.id())
            })
            .map(|row| {
                row.select(&cell_sel)
                    .filter(|cell| {
                        cell.ancestors()
                            .filter_map(ElementRef::wrap)
                            .find(|ancestor| ancestor.value().name() == "tr")
                            .is_some_and(|ancestor| ancestor.id() == row.id())
                    })
                    .map(|cell| {
                        cell.text()
                            .collect::<String>()
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .collect::<Vec<String>>()
            })
            .filter(|row| !row.is_empty())
            .collect();
        // Lagtabellen har: rank, tröjnummer, namn, position, GP, G, A, TP.
        let data_rows: Vec<Vec<String>> = rows
            .into_iter()
            .filter(|row| {
                row.len() >= 8
                    && row[0].parse::<usize>().is_ok()
                    && !row[2].is_empty()
                    && (4..=7).all(|i| row[i].parse::<i32>().is_ok())
            })
            .collect();
        if data_rows.is_empty() || data_rows.len() > 65 {
            continue;
        }
        let first_name = data_rows[0][2].clone();
        if first_players.insert(first_name) {
            player_tables.push(data_rows);
        }
    }
    if player_tables.is_empty() {
        anyhow::bail!("Hittade inga lagtabeller för spelarstatistik");
    }
    let mut players = Vec::new();
    for (team_index, rows) in player_tables
        .into_iter()
        .take(standings_teams.len())
        .enumerate()
    {
        let team = page_teams
            .get(team_index)
            .or_else(|| standings_teams.get(team_index))
            .context("Kunde inte koppla spelarlistan till SHL-lagen")?;
        for row in rows {
            let games = row[4].parse::<f64>().unwrap_or_default();
            let average = |index: usize| {
                let total = row[index].parse::<f64>().unwrap_or_default();
                if games > 0.0 {
                    format!("{:.2}", total / games)
                } else {
                    "0.00".to_owned()
                }
            };
            let goals_per_game = average(5);
            let assists_per_game = average(6);
            let points_per_game = average(7);
            players.push(Record(vec![
                (players.len() + 1).to_string(),
                row[2].clone(),
                team.clone(),
                row[4].clone(),
                row[5].clone(),
                row[6].clone(),
                row[7].clone(),
                row.get(11).cloned().unwrap_or_default(),
                row.get(8).cloned().unwrap_or_default(),
                goals_per_game,
                assists_per_game,
                points_per_game,
            ]));
        }
    }
    Ok(players)
}

fn fetch_goalies(url: &str, standings_teams: &[String]) -> Result<Vec<Record>> {
    let body = http_client()?.get(url).send()?.error_for_status()?.text()?;
    let document = Html::parse_document(&body);
    let table_sel = Selector::parse("table").unwrap();
    let row_sel = Selector::parse("tr").unwrap();
    let cell_sel = Selector::parse("th, td").unwrap();
    let team_set: std::collections::HashSet<&str> =
        standings_teams.iter().map(String::as_str).collect();
    let mut page_teams = Vec::new();
    for link in document.select(&Selector::parse("a").unwrap()) {
        let team = link.text().collect::<String>().trim().to_owned();
        if team_set.contains(team.as_str()) && !page_teams.contains(&team) {
            page_teams.push(team);
        }
    }

    let mut goalie_tables: Vec<(String, Vec<Vec<String>>, [usize; 6])> = Vec::new();
    let mut seen_goalies = std::collections::HashSet::new();
    for table in document.select(&table_sel) {
        let rows: Vec<Vec<String>> = table
            .select(&row_sel)
            .filter(|row| {
                row.ancestors()
                    .filter_map(ElementRef::wrap)
                    .find(|ancestor| ancestor.value().name() == "table")
                    .is_some_and(|ancestor| ancestor.id() == table.id())
            })
            .map(|row| {
                row.select(&cell_sel)
                    .filter(|cell| {
                        cell.ancestors()
                            .filter_map(ElementRef::wrap)
                            .find(|ancestor| ancestor.value().name() == "tr")
                            .is_some_and(|ancestor| ancestor.id() == row.id())
                    })
                    .map(|cell| {
                        cell.text()
                            .collect::<String>()
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|row| !row.is_empty())
            .collect();
        if rows.len() > 60 {
            continue;
        }
        let header_pos = rows.iter().position(|row| {
            let h: Vec<String> = row.iter().map(|s| s.to_ascii_lowercase()).collect();
            ["name", "svs", "sog", "svs%", "w"]
                .iter()
                .all(|key| h.iter().any(|item| item == key))
        });
        let Some(header_pos) = header_pos else {
            continue;
        };
        let h: Vec<String> = rows[header_pos]
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect();
        let find = |key: &str| h.iter().position(|item| item == key);
        let indices = [
            find("name").unwrap(),
            find("gpi")
                .or_else(|| find("gp"))
                .context("Saknar spelade matcher i målvaktsstatistiken")?,
            find("sog").unwrap(),
            find("svs%").unwrap(),
            find("w").unwrap(),
            find("svs").unwrap(),
        ];
        let name = rows
            .iter()
            .skip(header_pos + 1)
            .find_map(|row| row.get(indices[0]).filter(|n| !n.is_empty()).cloned());
        let Some(first_name) = name else {
            continue;
        };
        if seen_goalies.insert(first_name.clone()) {
            goalie_tables.push((
                first_name,
                rows.into_iter().skip(header_pos + 1).collect(),
                indices,
            ));
        }
    }
    if goalie_tables.is_empty() {
        anyhow::bail!("Hittade inga målvaktstabeller");
    }

    let mut goalies = Vec::new();
    for (team_index, (_, rows, indices)) in goalie_tables
        .into_iter()
        .take(standings_teams.len())
        .enumerate()
    {
        let team = page_teams
            .get(team_index)
            .or_else(|| standings_teams.get(team_index))
            .context("Kunde inte koppla målvaktslistan till SHL-lagen")?;
        for row in rows {
            let value = |index: usize| row.get(index).cloned().unwrap_or_default();
            if row.len() <= *indices.iter().max().unwrap() {
                continue;
            }
            let name = value(indices[0]);
            if name.is_empty() || name.eq_ignore_ascii_case("name") {
                continue;
            }
            goalies.push(Record(vec![
                (goalies.len() + 1).to_string(),
                name,
                team.clone(),
                value(indices[1]),
                value(indices[2]),
                value(indices[3]),
                value(indices[4]),
                value(indices[5]),
            ]));
        }
    }
    Ok(goalies)
}

fn normalize_standings(rows: Vec<Record>) -> Vec<Record> {
    rows.into_iter()
        .filter(|row| {
            row.0
                .first()
                .is_some_and(|rank| rank.parse::<usize>().is_ok())
        })
        .take(SHL_TEAM_COUNT)
        .map(|row| {
            let c = &row.0;
            let (goals_for, goals_against, goal_diff, points) = if c.len() >= 9 {
                let (for_goals, against_and_diff) = c[6].split_once(':').unwrap_or((&c[6], ""));
                let against = against_and_diff.split_whitespace().next().unwrap_or("—");
                (
                    for_goals.trim().to_owned(),
                    against.to_owned(),
                    c[7].clone(),
                    c[8].clone(),
                )
            } else if c.len() >= 8 {
                ("—".to_owned(), "—".to_owned(), c[6].clone(), c[7].clone())
            } else {
                (
                    "—".to_owned(),
                    "—".to_owned(),
                    "—".to_owned(),
                    "—".to_owned(),
                )
            };
            let games = c[2].parse::<f64>().unwrap_or_default();
            let points_avg = points
                .parse::<f64>()
                .ok()
                .map(|points| {
                    if games > 0.0 {
                        format!("{:.2}", points / games)
                    } else {
                        "0.00".to_owned()
                    }
                })
                .unwrap_or_else(|| "—".to_owned());
            Record(vec![
                c[0].clone(),
                c[1].clone(),
                c[2].clone(),
                c[3].clone(),
                c[4].clone(),
                c[5].clone(),
                goals_for,
                goals_against,
                goal_diff,
                points,
                points_avg,
            ])
        })
        .collect()
}

fn is_date(value: &str) -> bool {
    let date = value.get(..10).unwrap_or("");
    date.len() == 10
        && date.as_bytes()[4] == b'-'
        && date.as_bytes()[7] == b'-'
        && date
            .chars()
            .enumerate()
            .all(|(i, c)| matches!(i, 4 | 7) || c.is_ascii_digit())
}

fn is_time(value: &str) -> bool {
    let Some((hours, minutes)) = value.split_once(':') else {
        return false;
    };
    hours.len() == 2
        && minutes.len() == 2
        && hours.chars().all(|ch| ch.is_ascii_digit())
        && minutes.chars().all(|ch| ch.is_ascii_digit())
}

fn is_score(value: &str) -> bool {
    let Some((home, away)) = value.split_once('-') else {
        return false;
    };
    home.trim().parse::<u16>().is_ok() && away.trim().parse::<u16>().is_ok()
}

fn all_season_games(rows: Vec<Record>) -> (Vec<Record>, usize) {
    let mut matches = Vec::new();
    let mut first_upcoming = None;
    let mut current_date = String::new();

    for row in rows {
        if row.0.is_empty() {
            continue;
        }
        let offset = if is_date(&row.0[0]) {
            current_date = row.0[0].chars().take(10).collect();
            1
        } else {
            0
        };
        if current_date.is_empty() || row.0.len() <= offset + 1 {
            continue;
        }
        // Schedule pages group games underneath a date row. Some seasons instead
        // repeat the date on every game row, which the offset handles as well.
        let date = current_date.clone();
        let has_time = row
            .0
            .get(offset)
            .is_some_and(|value| value.contains(':') && value.len() <= 5);
        let game_index = offset + usize::from(has_time);
        let score_index = row
            .0
            .iter()
            .enumerate()
            .skip(game_index + 1)
            .find(|(_, value)| is_score(value))
            .map(|(i, _)| i);
        let game_end = score_index.unwrap_or_else(|| row.0.len().saturating_sub(1));
        let team_names = row
            .0
            .iter()
            .take(game_end)
            .skip(game_index)
            .filter_map(|value| {
                let value = value.trim();
                let value = if let (Some(date), Some(rest)) = (value.get(..10), value.get(10..)) {
                    if is_date(date) { rest } else { value }
                } else {
                    value
                };
                let cleaned = value
                    .split_whitespace()
                    .filter(|part| !is_date(part) && !is_time(part))
                    .collect::<Vec<_>>()
                    .join(" ");
                if cleaned.is_empty() || matches!(cleaned.as_str(), "-" | "–" | "—") {
                    None
                } else {
                    Some(cleaned)
                }
            })
            .collect::<Vec<_>>();
        let game = if team_names.len() == 1 {
            if let Some((home, away)) = team_names[0].split_once(" - ") {
                format!("{home} - {away}")
            } else {
                team_names[0].clone()
            }
        } else if team_names.len() >= 2 {
            format!("{} - {}", team_names[0], team_names[1])
        } else {
            String::new()
        };
        let (score, details, played) = if let Some(index) = score_index {
            (
                row.0[index].clone(),
                row.0.get(index + 1).cloned().unwrap_or_default(),
                true,
            )
        } else {
            first_upcoming.get_or_insert(matches.len());
            (
                "—".to_owned(),
                row.0.last().cloned().unwrap_or_default(),
                false,
            )
        };
        let attendance = if played {
            // Schedule rows may include attendance as a numeric field after venue.
            row.0
                .iter()
                .skip(3)
                .find(|value| {
                    let digits = value.replace([',', '.', ' '], "");
                    digits.len() >= 3 && digits.chars().all(|ch| ch.is_ascii_digit())
                })
                .cloned()
                .unwrap_or_else(|| "—".to_owned())
        } else {
            "—".to_owned()
        };
        if game.chars().all(|ch| ch.is_ascii_digit() || ch == ':') {
            continue;
        }
        matches.push((
            date,
            Record(vec![current_date.clone(), game, score, details, attendance]),
        ));
    }
    let games = matches.into_iter().map(|(_, row)| row).collect::<Vec<_>>();
    let boundary = first_upcoming.unwrap_or(games.len());
    (games, boundary)
}

struct App {
    data: Data,
    seasons: Vec<Season>,
    season_index: usize,
    tab: usize,
    selected: usize,
    sort: Option<(usize, bool, bool)>,
    search_query: String,
    searching: bool,
    searching_team: bool,
    team_filter: String,
    team_query: String,
    status: String,
}

fn run_ui(
    mut terminal: Terminal<CrosstermBackend<io::Stdout>>,
    data: Data,
    seasons: Vec<Season>,
    season_index: usize,
) -> Result<()> {
    let initial_game = data.games_boundary;
    let mut app = App {
        data,
        seasons,
        season_index,
        tab: 0,
        selected: initial_game,
        sort: None,
        search_query: String::new(),
        searching: false,
        searching_team: false,
        team_filter: String::new(),
        team_query: String::new(),
        status: "Redo.".into(),
    };
    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(5),
                    Constraint::Length(4),
                ])
                .split(area);
            let tabs = ["MATCHER", "SPELARE", "SHL-TABELL", "MÅLVAKTER"]
                .iter()
                .enumerate()
                .map(|(i, label)| {
                    if i == app.tab {
                        format!("  [{label}]  ")
                    } else {
                        format!("   {label}   ")
                    }
                })
                .collect::<String>();
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        " SHL ",
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::LightCyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("  {}  ", app.data.season),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(tabs, Style::default().fg(Color::Cyan)),
                ]))
                .block(Block::default().borders(Borders::BOTTOM)),
                chunks[0],
            );
            let filtered_players: Vec<Record> = app
                .data
                .players
                .iter()
                .filter(|player| {
                    let name_matches = player.0.get(1).is_some_and(|name| {
                        name.to_lowercase()
                            .contains(&app.search_query.to_lowercase())
                    });
                    let team_matches = player.0.get(2).is_some_and(|team| {
                        team.to_lowercase()
                            .contains(&app.team_filter.to_lowercase())
                    });
                    name_matches && team_matches
                })
                .cloned()
                .collect();
            let filtered_games: Vec<Record> = app
                .data
                .games
                .iter()
                .filter(|game| {
                    game.0.get(1).is_some_and(|teams| {
                        teams.to_lowercase().contains(&app.team_filter.to_lowercase())
                    })
                })
                .cloned()
                .collect();
            let (headers, records): (Vec<&str>, &Vec<Record>) = match app.tab {
                0 => (
                    vec!["Datum", "Match", "Resultat", "Arena / perioder", "Publik"],
                    &filtered_games,
                ),
                1 => (
                    vec![
                        "#", "Spelare", "Lag", "GP", "G", "A", "P", "+/-", "PIM", "G/MP", "A/MP", "P/MP",
                    ],
                    &filtered_players,
                ),
                2 => (
                    vec![
                        "#",
                        "Lag",
                        "SP",
                        "V",
                        "O",
                        "F",
                        "Gjorda",
                        "Insläppta",
                        "Målskillnad",
                        "P",
                        "P/MP",
                    ],
                    &app.data.standings,
                ),
                _ => (
                    vec![
                        "#",
                        "Målvakt",
                        "Lag",
                        "GP",
                        "Skott emot",
                        "Räddnings%",
                        "Vinster",
                        "Räddade skott",
                    ],
                    &app.data.goalies,
                ),
            };
            let widths = match app.tab {
                0 => vec![
                    Constraint::Length(18),
                    Constraint::Min(28),
                    Constraint::Length(12),
                    Constraint::Min(20),
                    Constraint::Length(9),
                ],
                1 => vec![
                    Constraint::Length(5),
                    Constraint::Fill(1),
                    Constraint::Length(22),
                    Constraint::Length(5),
                    Constraint::Length(5),
                    Constraint::Length(5),
                    Constraint::Length(5),
                    Constraint::Length(5),
                    Constraint::Length(5),
                    Constraint::Length(6),
                    Constraint::Length(6),
                    Constraint::Length(6),
                ],
                2 => vec![
                    Constraint::Length(5),
                    Constraint::Min(26),
                    Constraint::Length(5),
                    Constraint::Length(5),
                    Constraint::Length(5),
                    Constraint::Length(5),
                    Constraint::Length(8),
                    Constraint::Length(11),
                    Constraint::Length(11),
                    Constraint::Length(5),
                    Constraint::Length(7),
                ],
                _ => vec![
                    Constraint::Length(5),
                    Constraint::Fill(1),
                    Constraint::Length(22),
                    Constraint::Length(5),
                    Constraint::Length(11),
                    Constraint::Length(11),
                    Constraint::Length(8),
                    Constraint::Length(13),
                ],
            };
            let mut rows = Vec::with_capacity(records.len() + 3);
            for (record_index, r) in records.iter().enumerate() {
                if app.tab == 2 && [6, 10, 12].contains(&record_index) {
                    let separator = widths.iter().map(|_| {
                        Cell::from("─".repeat(32)).style(Style::default().fg(Color::DarkGray))
                    }).collect::<Vec<_>>();
                    rows.push(Row::new(separator));
                }
                let cells =
                    r.0.iter()
                        .enumerate()
                        .map(|(index, value)| {
                            let cell = Cell::from(value.clone());
                            if (app.tab == 1 || app.tab == 2 || app.tab == 3)
                                && app.sort.is_some_and(|(col, _, _)| col == index)
                            {
                                cell.style(Style::default().add_modifier(Modifier::BOLD))
                            } else {
                                cell
                            }
                        })
                        .collect::<Vec<_>>();
                rows.push(Row::new(cells));
            }
            let player_title = format!(
                " SPELARE ({}) {} · g/a/p/x stats · i PIM · m/b/e snitt ",
                filtered_players.len(),
                if app.team_filter.is_empty() {
                    "alla lag".to_owned()
                } else {
                    app.team_filter.clone()
                }
            );
            let title = match app.tab {
                0 => " MATCHER · hela säsongen · start vid senaste/kommande ",
                1 => player_title.as_str(),
                2 => " SHL-TABELL · grundserien ",
                _ => " MÅLVAKTER · SHL ",
            };
            let header = Row::new(
                headers
                    .iter()
                    .enumerate()
                    .map(|(index, label)| {
                        let cell = Cell::from(*label);
                        if (app.tab == 1 || app.tab == 2 || app.tab == 3)
                            && app.sort.is_some_and(|(col, _, _)| col == index)
                        {
                            cell.style(
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                            )
                        } else {
                            cell.style(
                                Style::default()
                                    .fg(Color::Cyan)
                                    .add_modifier(Modifier::BOLD),
                            )
                        }
                    })
                    .collect::<Vec<_>>(),
            );
            let table = Table::new(rows, widths)
                .header(header)
                .block(Block::default().title(title).borders(Borders::ALL))
                .row_highlight_style(
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                );
            let mut state = TableState::default();
            if !records.is_empty() {
                let selected = app.selected.min(records.len() - 1);
                let separators_before = if app.tab == 2 {
                    [6, 10, 12].iter().filter(|boundary| selected >= **boundary).count()
                } else { 0 };
                state.select(Some(selected + separators_before));
            }
            frame.render_stateful_widget(table, chunks[1], &mut state);
            let footer = if app.searching_team {
                format!(
                    "Sök lag: {}▏  Enter: filtrera  Esc: rensa lagfilter",
                    app.team_query
                )
            } else if app.searching {
                format!(
                    "Sök spelare: {}▏  Enter: klart  Esc: rensa/avbryt",
                    app.search_query
                )
            } else {
                app.status.clone()
            };
            let help = format!(
                "{footer}\nTab flikar | ↑↓ navigera | PgUp äldre / PgDn nyare säsong | / namn | f matchfilter | l spelarfilter | r uppdatera | q avsluta\nSpelare: g/a/p/x stats, i PIM, m/b/e snitt; versaler = lagvis | SHL: p poäng, s snitt\nMålvakt: g GP, s skott emot, v räddnings%, w vinster, c räddade; versaler = lagvis"
            );
            frame.render_widget(
                Paragraph::new(help).style(Style::default().fg(Color::Gray)),
                chunks[2],
            );
        })?;
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if app.searching {
                    match key.code {
                        KeyCode::Esc => {
                            app.search_query.clear();
                            app.searching = false;
                            app.selected = 0;
                            app.status = "Sökningen rensad.".into();
                        }
                        KeyCode::Enter => {
                            app.searching = false;
                            app.selected = 0;
                        }
                        KeyCode::Backspace => {
                            app.search_query.pop();
                            app.selected = 0;
                        }
                        KeyCode::Char(c) => {
                            app.search_query.push(c);
                            app.selected = 0;
                        }
                        _ => {}
                    }
                    continue;
                }
                if app.searching_team {
                    match key.code {
                        KeyCode::Esc => {
                            app.team_filter.clear();
                            app.team_query.clear();
                            app.searching_team = false;
                            app.selected = 0;
                            app.status = "Lagfilter rensat; visar alla lag.".into();
                        }
                        KeyCode::Enter => {
                            app.team_filter = app.team_query.trim().to_owned();
                            app.searching_team = false;
                            app.selected = 0;
                            app.status = if app.team_filter.is_empty() {
                                "Visar spelare från alla lag.".into()
                            } else {
                                format!("Filtrerat på lag: {}", app.team_filter)
                            };
                        }
                        KeyCode::Backspace => {
                            app.team_query.pop();
                        }
                        KeyCode::Char(c) => {
                            app.team_query.push(c);
                        }
                        _ => {}
                    }
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('/') if app.tab == 1 => {
                        app.searching = true;
                        app.search_query.clear();
                        app.selected = 0;
                    }
                    KeyCode::Char('l') if app.tab == 1 => {
                        app.searching_team = true;
                        app.team_query = app.team_filter.clone();
                    }
                    KeyCode::Char('f') if app.tab == 0 => {
                        app.searching_team = true;
                        app.team_query = app.team_filter.clone();
                    }
                    KeyCode::Tab | KeyCode::Right => {
                        app.tab = (app.tab + 1) % 4;
                        app.selected = 0;
                    }
                    KeyCode::Left => {
                        app.tab = (app.tab + 3) % 4;
                        app.selected = 0;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let len = match app.tab {
                            0 => app
                                .data
                                .games
                                .iter()
                                .filter(|game| {
                                    game.0.get(1).is_some_and(|teams| {
                                        teams
                                            .to_lowercase()
                                            .contains(&app.team_filter.to_lowercase())
                                    })
                                })
                                .count(),
                            1 => app.data.players.len(),
                            2 => app.data.standings.len(),
                            _ => app.data.goalies.len(),
                        };
                        app.selected = app.selected.saturating_add(1).min(len.saturating_sub(1));
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.selected = app.selected.saturating_sub(1)
                    }
                    KeyCode::PageUp
                    | KeyCode::Char('[')
                    | KeyCode::PageDown
                    | KeyCode::Char(']') => {
                        let next = if matches!(key.code, KeyCode::PageUp | KeyCode::Char('[')) {
                            (app.season_index + 1).min(app.seasons.len().saturating_sub(1))
                        } else {
                            app.season_index.saturating_sub(1)
                        };
                        if next != app.season_index {
                            match fetch_data(&app.seasons[next]) {
                                Ok(data) => {
                                    app.data = data;
                                    app.season_index = next;
                                    app.selected = app.data.games_boundary;
                                    app.sort = None;
                                    app.status = format!("Visar {}.", app.data.season);
                                }
                                Err(error) => {
                                    app.status = format!("Kunde inte byta säsong: {error:#}")
                                }
                            }
                        } else {
                            app.status = if matches!(key.code, KeyCode::PageUp | KeyCode::Char('['))
                            {
                                "Redan på den äldsta säsongen.".into()
                            } else {
                                "Redan på den nyaste säsongen.".into()
                            };
                        }
                    }
                    KeyCode::Char('r') => match fetch_data(&app.seasons[app.season_index]) {
                        Ok(data) => {
                            app.data = data;
                            app.selected = app.data.games_boundary;
                            app.sort = None;
                            app.status = "Data uppdaterad.".into();
                        }
                        Err(error) => app.status = format!("Uppdatering misslyckades: {error:#}"),
                    },
                    KeyCode::Char('g')
                    | KeyCode::Char('s')
                    | KeyCode::Char('v')
                    | KeyCode::Char('w')
                    | KeyCode::Char('c')
                    | KeyCode::Char('G')
                    | KeyCode::Char('S')
                    | KeyCode::Char('V')
                    | KeyCode::Char('W')
                    | KeyCode::Char('C')
                        if app.tab == 3 =>
                    {
                        let (col, by_team) = match key.code {
                            KeyCode::Char('g') => (3, false),
                            KeyCode::Char('s') => (4, false),
                            KeyCode::Char('v') => (5, false),
                            KeyCode::Char('w') => (6, false),
                            KeyCode::Char('c') => (7, false),
                            KeyCode::Char('G') => (3, true),
                            KeyCode::Char('S') => (4, true),
                            KeyCode::Char('V') => (5, true),
                            KeyCode::Char('W') => (6, true),
                            _ => (7, true),
                        };
                        let descending = match app.sort {
                            Some((old_col, desc, old_team))
                                if old_col == col && old_team == by_team =>
                            {
                                !desc
                            }
                            _ => true,
                        };
                        app.data.goalies.sort_by(|a, b| {
                            let av =
                                a.0.get(col)
                                    .and_then(|v| v.trim_end_matches('%').parse::<f64>().ok())
                                    .unwrap_or_default();
                            let bv =
                                b.0.get(col)
                                    .and_then(|v| v.trim_end_matches('%').parse::<f64>().ok())
                                    .unwrap_or_default();
                            let metric = if descending {
                                bv.total_cmp(&av)
                            } else {
                                av.total_cmp(&bv)
                            };
                            if by_team {
                                a.0.get(2).cmp(&b.0.get(2)).then(metric)
                            } else {
                                metric
                            }
                        });
                        for (rank, row) in app.data.goalies.iter_mut().enumerate() {
                            if !row.0.is_empty() {
                                row.0[0] = (rank + 1).to_string();
                            }
                        }
                        app.sort = Some((col, descending, by_team));
                        app.selected = 0;
                        let stat = match col {
                            3 => "spelade matcher",
                            4 => "skott emot",
                            5 => "räddningsprocent",
                            6 => "vinster",
                            _ => "räddade skott",
                        };
                        app.status = format!(
                            "Sorterat {} {stat} {}.",
                            if by_team { "lagvis på" } else { "på" },
                            if descending { "fallande" } else { "stigande" }
                        );
                    }
                    KeyCode::Char('p') | KeyCode::Char('s') if app.tab == 2 => {
                        let col = if key.code == KeyCode::Char('p') {
                            9
                        } else {
                            10
                        };
                        let descending = match app.sort {
                            Some((old_col, desc, _)) if old_col == col => !desc,
                            _ => true,
                        };
                        app.data.standings.sort_by(|a, b| {
                            let av =
                                a.0.get(col)
                                    .and_then(|v| v.parse::<f64>().ok())
                                    .unwrap_or_default();
                            let bv =
                                b.0.get(col)
                                    .and_then(|v| v.parse::<f64>().ok())
                                    .unwrap_or_default();
                            if descending {
                                bv.total_cmp(&av)
                            } else {
                                av.total_cmp(&bv)
                            }
                        });
                        for (rank, row) in app.data.standings.iter_mut().enumerate() {
                            if !row.0.is_empty() {
                                row.0[0] = (rank + 1).to_string();
                            }
                        }
                        app.sort = Some((col, descending, false));
                        app.selected = 0;
                        app.status = format!(
                            "Sorterat {} {}.",
                            if col == 9 {
                                "på poäng"
                            } else {
                                "på poängsnitt"
                            },
                            if descending { "fallande" } else { "stigande" }
                        );
                    }
                    KeyCode::Char('g')
                    | KeyCode::Char('a')
                    | KeyCode::Char('p')
                    | KeyCode::Char('x')
                    | KeyCode::Char('m')
                    | KeyCode::Char('b')
                    | KeyCode::Char('e')
                    | KeyCode::Char('i')
                    | KeyCode::Char('G')
                    | KeyCode::Char('A')
                    | KeyCode::Char('P')
                    | KeyCode::Char('X')
                    | KeyCode::Char('M')
                    | KeyCode::Char('B')
                    | KeyCode::Char('E')
                    | KeyCode::Char('I')
                        if app.tab == 1 =>
                    {
                        let (col, by_team) = match key.code {
                            KeyCode::Char('g') => (4, false),
                            KeyCode::Char('a') => (5, false),
                            KeyCode::Char('p') => (6, false),
                            KeyCode::Char('x') => (7, false),
                            KeyCode::Char('i') => (8, false),
                            KeyCode::Char('m') => (9, false),
                            KeyCode::Char('b') => (10, false),
                            KeyCode::Char('e') => (11, false),
                            KeyCode::Char('G') => (4, true),
                            KeyCode::Char('A') => (5, true),
                            KeyCode::Char('P') => (6, true),
                            KeyCode::Char('X') => (7, true),
                            KeyCode::Char('I') => (8, true),
                            KeyCode::Char('M') => (9, true),
                            KeyCode::Char('B') => (10, true),
                            _ => (11, true),
                        };
                        let descending = match app.sort {
                            Some((old_col, desc, old_team))
                                if old_col == col && old_team == by_team =>
                            {
                                !desc
                            }
                            _ => true,
                        };
                        app.data.players.sort_by(|a, b| {
                            let av =
                                a.0.get(col)
                                    .and_then(|v| v.parse::<f64>().ok())
                                    .unwrap_or_default();
                            let bv =
                                b.0.get(col)
                                    .and_then(|v| v.parse::<f64>().ok())
                                    .unwrap_or_default();
                            let metric = if descending {
                                bv.total_cmp(&av)
                            } else {
                                av.total_cmp(&bv)
                            };
                            if by_team {
                                a.0.get(2).cmp(&b.0.get(2)).then(metric)
                            } else {
                                metric
                            }
                        });
                        for (rank, row) in app.data.players.iter_mut().enumerate() {
                            if !row.0.is_empty() {
                                row.0[0] = (rank + 1).to_string();
                            }
                        }
                        app.sort = Some((col, descending, by_team));
                        app.selected = 0;
                        let category = match col {
                            4 => "mål",
                            5 => "assist",
                            6 => "poäng",
                            7 => "+/-",
                            8 => "utvisningsminuter",
                            9 => "mål per match",
                            10 => "assist per match",
                            _ => "poäng per match",
                        };
                        app.status = format!(
                            "Sorterat {} {category} {}. Tryck samma tangent igen för motsatt ordning.",
                            if by_team { "lagvis på" } else { "globalt på" },
                            if descending { "fallande" } else { "stigande" }
                        );
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let current_id = current_shl_id()?;
    let seasons = available_shl_seasons(&current_id)?;
    let season_index = seasons
        .iter()
        .position(|season| season.id == current_id)
        .unwrap_or(0);
    let data = fetch_data(&seasons[season_index])?;
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    let result = run_ui(terminal, data, seasons, season_index);
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    result
}
