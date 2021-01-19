use std::{
    error::Error,
    io,
    io::Write,
    sync::mpsc::{self},
    thread,
};

use app::App;

use crates_io::{CrateSearchResponse, CrateSearcher, CratesSort};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, ScrollUp,
    },
};
use input::InputMonitor;

use structopt::StructOpt;

use tui::{backend::CrosstermBackend, layout::Rect, widgets::Paragraph, Terminal};

mod app;
mod crates_io;
mod input;
mod widgets;

pub(crate) fn ceil_div(a: u32, b: u32) -> u32 {
    if b == 0 {
        panic!("attempt to divide by zero");
    } else if a == 0 {
        0
    } else {
        (a + b - 1) / b
    }
}

#[derive(Debug, StructOpt)]
#[structopt(name = "Cratuity", about = "A simple TUI for searching Crates.io")]
/// A TUI for searching crates.io in the terminal.  
///
/// Alternatively, the find option may be used to bypass the TUI and output the
/// results directly to the terminal.
pub struct AppArgs {
    #[structopt(short, long)]
    pub find: Option<String>,

    #[structopt(short, long, default_value)]
    pub sort: CratesSort,

    #[structopt(short, long, default_value = "5")]
    pub count: usize,
}

fn main() -> Result<(), Box<dyn Error>> {
    let matches = AppArgs::clap().get_matches();
    if matches.is_present("help") {
        println!("{}", matches.usage());
        return Ok(());
    }

    let args: AppArgs = AppArgs::from_clap(&matches);
    if let Some(find) = args.find {
        cli_search(find.as_str(), args.sort, args.count)?;

        return Ok(());
    }

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || InputMonitor::new(tx).monitor());
    let mut app = App::new(rx);

    let mut stdout = io::stdout();
    enable_raw_mode()?;

    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    loop {
        terminal.draw(|f| {
            app.draw(f);
        })?;

        app.await_input();
        if app.quit {
            break;
        }
    }
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.clear().unwrap();
    Ok(())
}

fn cli_search(term: &str, sort: CratesSort, count: usize) -> Result<(), Box<dyn Error>> {
    let crate_search = CrateSearcher::new()?;
    let resp = crate_search.search_sorted_count(term, 1, count as u32, &sort)?;
    print_crates_table(resp)
}

fn print_crates_table(crates: CrateSearchResponse) -> Result<(), Box<dyn Error>> {
    // Print a table with TUI
    let mut stdout = io::stdout();
    execute!(stdout, ScrollUp(10))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let (_, cursor_y) = terminal.get_cursor()?;
    terminal.set_cursor(0, cursor_y - 10)?;

    let (_, cursor_y) = terminal.get_cursor()?;
    let window = terminal.get_frame().size();
    let area = Rect::new(0, cursor_y, window.width, window.height - cursor_y);

    Ok(())
}
