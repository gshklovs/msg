use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use std::io::stdout;

use crate::matcher::Ranker;
use crate::model::{Kind, Person};

/// What a keypress did to the picker.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Continue,
    Cancel,
    Select(usize),
}

/// Query box + result list, shared by the terminal and window front ends.
pub struct Picker {
    pub people: Vec<Person>,
    pub query: String,
    pub matches: Vec<usize>,
    pub cursor: usize,
    ranker: Ranker,
}

impl Picker {
    pub fn new(people: Vec<Person>) -> Self {
        let mut p = Self {
            people,
            query: String::new(),
            matches: Vec::new(),
            cursor: 0,
            ranker: Ranker::new(),
        };
        p.refilter();
        p
    }

    pub fn refilter(&mut self) {
        self.matches = self.ranker.rank(&self.people, &self.query);
        self.cursor = 0;
    }

    pub fn move_cursor(&mut self, delta: isize) {
        if self.matches.is_empty() {
            return;
        }
        let last = self.matches.len() - 1;
        self.cursor = (self.cursor as isize + delta).clamp(0, last as isize) as usize;
    }

    /// Apply one key. Typing refilters and resets the selection to the top match.
    pub fn on_key(&mut self, key: KeyEvent) -> Outcome {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => Outcome::Cancel,
            (KeyCode::Enter, _) => match self.matches.get(self.cursor) {
                Some(i) => Outcome::Select(*i),
                None => Outcome::Continue,
            },
            (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                self.move_cursor(1);
                Outcome::Continue
            }
            (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                self.move_cursor(-1);
                Outcome::Continue
            }
            (KeyCode::Backspace, _) => {
                self.query.pop();
                self.refilter();
                Outcome::Continue
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                self.query.clear();
                self.refilter();
                Outcome::Continue
            }
            (KeyCode::Char(c), m) if m.is_empty() || m == KeyModifiers::SHIFT => {
                self.query.push(c);
                self.refilter();
                Outcome::Continue
            }
            _ => Outcome::Continue,
        }
    }
}

pub fn label(p: &Person) -> String {
    match p.kind {
        Kind::Group => format!("{}  (group)", p.name),
        Kind::Person => p.name.clone(),
    }
}

/// Run the terminal picker. Returns the chosen person, or None on Esc.
pub fn run(people: Vec<Person>) -> Result<Option<Person>> {
    let mut picker = Picker::new(people);

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let result = (|| -> Result<Option<Person>> {
        loop {
            terminal.draw(|f| draw(f, &picker))?;
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != event::KeyEventKind::Press {
                continue;
            }
            match picker.on_key(key) {
                Outcome::Continue => {}
                Outcome::Cancel => return Ok(None),
                Outcome::Select(i) => return Ok(Some(picker.people[i].clone())),
            }
        }
    })();

    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    result
}

fn draw(f: &mut Frame, picker: &Picker) {
    let area = f.area();
    let chunks = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(area);

    let prompt = Paragraph::new(Line::from(vec![
        Span::styled("› ", Style::new().fg(Color::Cyan)),
        Span::raw(&picker.query),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" msg  {} ", picker.matches.len())),
    );
    f.render_widget(prompt, chunks[0]);

    let rows = chunks[1].height as usize;
    let start = picker.cursor.saturating_sub(rows.saturating_sub(1));
    let items: Vec<ListItem> = picker
        .matches
        .iter()
        .skip(start)
        .take(rows)
        .map(|i| ListItem::new(label(&picker.people[*i])))
        .collect();
    let mut state = ListState::default().with_selected(Some(picker.cursor - start));
    f.render_stateful_widget(
        List::new(items).highlight_style(Style::new().fg(Color::Black).bg(Color::Cyan)),
        chunks[1],
        &mut state,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn people() -> Vec<Person> {
        ["Ada Lovelace", "Grace Hopper", "Alan Turing"]
            .iter()
            .enumerate()
            .map(|(i, n)| Person {
                name: (*n).into(),
                handles: vec![format!("+1555010000{i}")],
                last_message_ts: 100 - i as i64,
                kind: Kind::Person,
                guid: None,
                best_handle: None,
                members: Vec::new(),
                named: false,
            })
            .collect()
    }

    fn press(p: &mut Picker, c: char) -> Outcome {
        p.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
    }

    #[test]
    fn top_match_is_selected_while_typing() {
        let mut p = Picker::new(people());
        assert_eq!(p.people[p.matches[p.cursor]].name, "Ada Lovelace");
        press(&mut p, 'g');
        assert_eq!(p.cursor, 0);
        assert_eq!(p.people[p.matches[p.cursor]].name, "Grace Hopper");
    }

    #[test]
    fn moving_then_typing_resets_to_the_top() {
        let mut p = Picker::new(people());
        p.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(p.cursor, 1);
        press(&mut p, 'a');
        assert_eq!(p.cursor, 0);
    }

    #[test]
    fn cursor_stays_in_range() {
        let mut p = Picker::new(people());
        for _ in 0..10 {
            p.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        assert_eq!(p.cursor, 2);
        for _ in 0..10 {
            p.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        }
        assert_eq!(p.cursor, 0);
    }

    #[test]
    fn enter_selects_and_esc_cancels() {
        let mut p = Picker::new(people());
        press(&mut p, 'a');
        press(&mut p, 'l');
        let out = p.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let Outcome::Select(i) = out else {
            panic!("expected a selection, got {out:?}");
        };
        assert_eq!(p.people[i].name, "Alan Turing");
        assert_eq!(
            p.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Outcome::Cancel
        );
    }

    #[test]
    fn backspace_widens_the_result_set() {
        let mut p = Picker::new(people());
        press(&mut p, 'z');
        assert!(p.matches.is_empty());
        assert_eq!(
            p.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Outcome::Continue,
            "enter on an empty list does nothing"
        );
        p.on_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(p.matches.len(), 3);
    }
}
