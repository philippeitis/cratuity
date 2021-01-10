use std::{cmp, time::Duration};

use crossbeam_channel::{Receiver, Sender};
use tui::{
    backend::Backend,
    layout::{Constraint, Layout},
    text::Text,
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

#[cfg(not(feature = "no-copy"))]
use clipboard::{ClipboardContext, ClipboardProvider};

#[cfg(not(feature = "no-copy"))]
use crate::crates_io::CrateSearch;

use crate::{
    crates_io::{CrateSearchResponse, CrateSearcher, CratesSort},
    input::InputEvent,
    widgets::{CrateWidget, InputWidget, SortingWidget},
};

pub struct SortingField {
    pub(crate) selection: usize,
    pub(crate) items: Vec<CratesSort>,
    pub(crate) strs: Vec<String>,
}

impl From<&'_ CratesSort> for SortingField {
    fn from(sort: &'_ CratesSort) -> Self {
        let mut items = Vec::with_capacity(4);
        items.push(CratesSort::Relevance);
        items.push(CratesSort::AllTimeDownload);
        items.push(CratesSort::RecentDownload);
        items.push(CratesSort::RecentUpdate);
        items.push(CratesSort::NewlyAdded);

        let selection = items.iter().position(|item| sort.eq(item)).unwrap();
        let strs = items.iter().map(|item| format!("{}", item)).collect();

        Self {
            selection,
            items,
            strs,
        }
    }
}

pub enum AppMode {
    Normal,
    Input(String, u64),
    Sorting(SortingField),
}

pub struct App {
    input_rx: Receiver<InputEvent>,
    client: CrateSearcher,
    pub crates: Option<CrateSearchResponse>,
    pub quit: bool,
    inpt: Option<String>,
    page: u32,
    sort: CratesSort,
    mode: AppMode,
    selection: Option<usize>,
}

impl App {
    pub fn new(input_tx: Sender<InputEvent>, input_rx: Receiver<InputEvent>) -> Self {
        Self {
            input_rx,
            client: CrateSearcher::new(input_tx).unwrap(),
            crates: None,
            quit: false,
            inpt: Some("".to_string()),
            page: 1,
            mode: AppMode::Input("".to_string(), 0),
            sort: CratesSort::Relevance,
            selection: None,
        }
    }

    pub fn draw<T: Backend>(&self, f: &mut Frame<T>) {
        let size = f.size();
        let block = Block::default()
            .title("Cratuity (A crates.io quick search TUI)")
            .borders(Borders::ALL)
            .border_type(BorderType::Thick);

        let area = block.inner(f.size());

        // render the top message
        let splits = Layout::default()
            .constraints([Constraint::Length(2), Constraint::Min(5)].as_ref())
            .split(area);

        let top = splits[0];
        let area = splits[1];
        let message = match self.mode {
            AppMode::Normal => {
                Text::raw("Press N/P to move between pages.  Press f to search for a term\nPress J/K to change the highlighted Crate and press C to copy it's Cargo.toml string") 
            }
            AppMode::Input(_, _) => {
                "Type to enter your search term.  Press Enter to confirm.  Press ESC to cancel".into()
            }
            AppMode::Sorting(_) => {
                "Press J/K to move between options.  Press Enter to confirm.  Press ESC to cancel".into()
            }
        };
        let message = Paragraph::new(message);
        f.render_widget(message, top);

        // render the bottom message with page details
        let splits = Layout::default()
            .constraints([Constraint::Min(5), Constraint::Length(1)].as_ref())
            .split(area);

        let bot = splits[1];
        let area = splits[0];

        let message = Paragraph::new(format!("Page {}", self.page));
        f.render_widget(message, bot);

        if let Some(CrateSearchResponse { ref crates }) = self.crates {
            let mut widgets = Vec::new();
            for (i, crte) in crates.iter().enumerate() {
                if let Some(selection) = self.selection {
                    widgets.push(CrateWidget::new(crte, selection == i));
                } else {
                    widgets.push(CrateWidget::new(crte, false));
                }
            }

            let splits = Layout::default()
                .horizontal_margin(1)
                .constraints(
                    [
                        Constraint::Percentage(20),
                        Constraint::Percentage(20),
                        Constraint::Percentage(20),
                        Constraint::Percentage(20),
                        Constraint::Percentage(20),
                    ]
                    .as_ref(),
                )
                .split(area);
            widgets
                .into_iter()
                .zip(splits)
                .for_each(|(w, a)| f.render_widget(w, a));
        }

        f.render_widget(block, size);
        self.draw_mode(f);
    }

    fn draw_mode<T: Backend>(&self, f: &mut Frame<T>) {
        match &self.mode {
            AppMode::Input(msg, ticks) => {
                let show_cursor = (ticks & 1) == 0;
                let inpt = InputWidget::new("Enter you search term", msg.as_str(), show_cursor);
                f.render_widget(inpt, f.size());
            }
            AppMode::Normal => {}
            AppMode::Sorting(state) => {
                let widget = SortingWidget::new(state, "Select you sorting method");
                f.render_widget(widget, f.size());
            }
        }
    }

    pub fn await_input(&mut self) {
        if let Ok(inpt) = self.input_rx.recv_timeout(Duration::from_secs(1)) {
            match &mut self.mode {
                AppMode::Normal => match inpt {
                    InputEvent::Char(c) => match c {
                        'f' | 'F' => {
                            self.mode = AppMode::Input("".to_string(), 0);
                        }
                        'q' | 'Q' => {
                            self.quit = true;
                        }
                        'n' | 'N' => {
                            if self.crates.as_ref().map(|c| c.crates.len()).unwrap_or(0) > 0 {
                                self.page += 1;
                                self.do_search();
                            }
                        }
                        'p' | 'P' => {
                            if self.page > 1 {
                                self.page -= 1;
                                self.do_search();
                            }
                        }
                        'j' | 'J' => {
                            if let Some(selection) = self.selection {
                                self.selection = Some(cmp::min(
                                    selection + 1,
                                    self.crates
                                        .as_ref()
                                        .map(|resp| &resp.crates)
                                        .map(|crates| crates.len() - 1)
                                        .unwrap_or(0),
                                ));
                            }
                        }
                        'k' | 'K' => {
                            if let Some(selection) = self.selection {
                                if selection > 0 {
                                    self.selection = Some(selection - 1);
                                }
                            }
                        }
                        's' | 'S' => {
                            self.mode = AppMode::Sorting(SortingField::from(&self.sort));
                        }
                        'c' | 'C' => {
                            self.copy_selection();
                        }
                        _ => {}
                    },
                    InputEvent::Results(results) => {
                        self.crates = Some(results);
                        self.selection = if let Some(ref crates) = self.crates {
                            if crates.crates.len() > 0 {
                                Some(0)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    _ => {}
                },
                AppMode::Input(ref mut msg, ref mut ticks) => match inpt {
                    InputEvent::Esc => self.mode = AppMode::Normal,
                    InputEvent::Enter => {
                        let replaced = std::mem::take(msg);
                        self.page = 1;
                        self.inpt = Some(replaced);
                        self.do_search();
                        self.mode = AppMode::Normal;
                    }
                    InputEvent::Backspace => {
                        let _ = msg.pop();
                    }
                    InputEvent::Char(c) => msg.push(c),
                    InputEvent::Tick => {
                        *ticks = ticks.wrapping_add(1);
                    }
                    InputEvent::Results(results) => {
                        self.crates = Some(results);
                        self.selection = if let Some(ref crates) = self.crates {
                            if crates.crates.len() > 0 {
                                Some(0)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                },
                AppMode::Sorting(SortingField {
                    selection,
                    items,
                    strs: _,
                }) => match inpt {
                    InputEvent::Esc => self.mode = AppMode::Normal,
                    InputEvent::Enter => {
                        self.sort = items[*selection].clone();
                        self.page = 1;
                        self.mode = AppMode::Normal;
                        self.do_search();
                    }
                    InputEvent::Char(c) => match c {
                        'k' | 'K' => {
                            *selection = selection.saturating_sub(1);
                        }
                        'j' | 'J' => {
                            *selection = cmp::min(*selection + 1, 4);
                        }
                        _ => {}
                    },
                    InputEvent::Results(results) => {
                        self.crates = Some(results);
                        self.selection = if let Some(ref crates) = self.crates {
                            if crates.crates.len() > 0 {
                                Some(0)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    _ => {}
                },
            }
        }
    }

    fn do_search(&mut self) {
        let search = self.inpt.as_ref();
        self.client
            .search_sorted(search.unwrap(), self.page, &self.sort);
    }

    #[cfg(not(feature = "no-copy"))]
    fn copy_selection(&self) {
        if let Some(selection) = self.selection {
            if let Some(ref crates) = self.crates {
                let crte = crates.crates.get(selection);
                let toml = crte.map(CrateSearch::get_toml_str);
                let mut clipboard: ClipboardContext = ClipboardProvider::new().unwrap();

                toml.map(|toml| clipboard.set_contents(toml).unwrap());
            }
        }
    }

    #[cfg(feature = "no-copy")]
    fn copy_selection(&self) {}
}
