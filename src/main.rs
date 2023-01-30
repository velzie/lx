use std::{
    cmp::Ordering,
    ffi::CStr,
    fs::{self, DirEntry, File},
    io::{self, Stdin, StdinLock, StdoutLock, Write},
    path::{Path, PathBuf},
};
use termion::{
    cursor::DetectCursorPos,
    event::{Event, Key},
    input::TermRead,
    raw::IntoRawMode,
    terminal_size,
};

use getopts::{Matches, Options};
use std::env;
const CELL_WIDTH: u16 = 20;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let mut opts = Options::new();
    let program = args[0].clone();

    opts.optflag("u", "use-unicode", "use nerdfont symbols");
    opts.optflag("h", "help", "print this help menu");
    let getopt_matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(f) => {
            panic!("{}", f.to_string())
        }
    };
    if getopt_matches.opt_present("help") {
        print_usage(&program, opts);
        return Ok(());
    }

    let mut tmp_file = make_temp_file()?;

    let startpos = term_cursor::get_pos().unwrap();
    let term = io::stdout().into_raw_mode()?;
    term.activate_raw_mode()?;

    let mut stdout = term.lock();

    let mut stdin = io::stdin().lock();
    let finished_pwd = do_loop(&getopt_matches, &mut stdin, &mut stdout, startpos)?;

    write!(tmp_file, "{}", finished_pwd.to_str().unwrap())?;
    Ok(())
}
fn make_temp_file() -> Result<File, io::Error> {
    let mut ttyname = unsafe { CStr::from_ptr(libc::ttyname(1)) }
        .to_str()
        .unwrap()
        .to_string();
    ttyname.remove(0);
    let tmp_path = Path::new("/tmp/lx/").join(ttyname);
    fs::create_dir_all(tmp_path.parent().unwrap())?;
    File::create(tmp_path)
}
fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} [options]", program);
    print!("{}", opts.usage(&brief));
}
fn do_loop(
    opt_matches: &Matches,
    stdin: &mut StdinLock,
    stdout: &mut StdoutLock,
    startpos: (i32, i32),
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let pwdtmp = &std::env::current_dir()?;
    let mut pwd = Path::new(pwdtmp).to_path_buf();

    // let mut stdin = stdin.lock();
    let mut events = stdin.events();
    let size = terminal_size()?;
    write!(stdout, "{}", termion::cursor::Hide)?;
    let mut selected_idx = 0;
    let mut start_y = startpos.1 as i32;
    loop {
        let contents: Vec<fs::DirEntry> = sort_dir_entries(
            fs::read_dir(pwd.clone())?
                .map(|f| f.unwrap())
                .filter(|f| !f.file_name().to_str().unwrap().starts_with("."))
                .collect(),
        );
        let entries_per_row = (contents.len() as u16).min(size.0 / CELL_WIDTH - 1);
        default_color(stdout)?;
        /// TODO:
        /// big dirs can go  off screen
        /// don't rerender everything
        let totalwidth = CELL_WIDTH * entries_per_row;

        term_cursor::set_pos(0, start_y).unwrap();

        let extrachars = 4;
        let pwdstr = pwd
            .to_str()
            .unwrap()
            .chars()
            .take((totalwidth as i32 - extrachars as i32).max(0) as usize)
            .collect::<String>();
        write!(
            stdout,
            "╭┤{}├{}╮", // this is 4 extra chars
            pwdstr,
            "─".repeat(
                (totalwidth as i32 - pwdstr.len() as i32 + entries_per_row as i32
                    - extrachars as i32
                    + 1)
                .max(0) as usize
            )
        )?;
        for (idx, file) in contents.iter().enumerate() {
            let row_y = idx / entries_per_row as usize;
            if idx % entries_per_row as usize == 0 {
                if start_y + row_y as i32 >= size.1 as i32 {
                    start_y -= 1;
                    write!(stdout, "\n")?;
                }

                let end = CELL_WIDTH as i32 * entries_per_row as i32 + entries_per_row as i32 + 1;
                term_cursor::set_pos(0, start_y + row_y as i32 + 1).unwrap();
                write!(stdout, "{}", " ".repeat(end as usize))?;
                term_cursor::set_pos(end, start_y + row_y as i32 + 1).unwrap();
                write!(stdout, "│")?;
                term_cursor::set_pos(0, start_y + row_y as i32 + 1).unwrap();
                write!(stdout, "│")?;
            }

            draw_entry(
                stdout,
                file,
                opt_matches.opt_present("use-unicode"),
                idx == selected_idx,
            )?;
            default_color(stdout)?;
            if idx % entries_per_row as usize != entries_per_row as usize - 1 {
                write!(stdout, " ")?;
            }
        }
        let endy =
            start_y + ((contents.len().max(1) - 1) / entries_per_row.max(1) as usize) as i32 + 2;
        if endy - 1 >= size.1 as i32 {
            write!(stdout, "\n")?;
            start_y -= 1;
        }
        term_cursor::set_pos(0, endy).unwrap();
        write!(
            stdout,
            "╰{}╯",
            "─".repeat(totalwidth as usize + entries_per_row.max(1) as usize - 1)
        )?;

        stdout.flush()?;
        let selectedentry = contents.get(selected_idx);
        if let Ok(e) = events.next().unwrap() {
            match e {
                Event::Key(Key::Backspace) => {
                    pwd = pwd.parent().unwrap().to_path_buf();
                    selected_idx = 0;
                    true_clear(stdout, start_y, size)?;
                }
                Event::Key(Key::Esc) => {
                    break;
                }
                Event::Key(Key::Char('\n')) => {
                    if let Some(entry) = selectedentry {
                        if entry.file_type().unwrap().is_dir() {
                            let dir = entry.path().clone();
                            pwd = dir;
                            selected_idx = 0;
                            true_clear(stdout, start_y, size)?;
                        }
                    }
                }
                Event::Key(Key::Char('l')) | Event::Key(Key::Right) => {
                    selected_idx = (selected_idx + 1).min(contents.len().max(1) - 1);
                }
                Event::Key(Key::Char('h')) | Event::Key(Key::Left) => {
                    if selected_idx > 0 {
                        selected_idx -= 1;
                    }
                }
                Event::Key(Key::Char('k')) | Event::Key(Key::Up) => {
                    selected_idx = (selected_idx as i32 - entries_per_row as i32).max(0) as usize;
                }
                Event::Key(Key::Char('j')) | Event::Key(Key::Down) => {
                    selected_idx =
                        (selected_idx + entries_per_row as usize).min(contents.len().max(1) - 1);
                }
                _ => {}
            }
        }
    }
    term_cursor::set_pos(0, start_y).unwrap();
    write!(
        stdout,
        "{}{}{}",
        termion::color::Bg(termion::color::Reset),
        termion::clear::AfterCursor,
        termion::cursor::Show
    )?;
    Ok(pwd)
}

fn true_clear(stdout: &mut StdoutLock, start_y: i32, size: (u16, u16)) -> Result<(), io::Error> {
    write!(stdout, "{}", termion::color::Bg(termion::color::Reset))?;
    Ok(for i in start_y..size.1 as i32 + 1 {
        term_cursor::set_pos(0, i).unwrap();
        write!(stdout, "{}", " ".repeat(size.0 as usize))?;
    })
}
fn default_color(stdout: &mut StdoutLock) -> Result<(), io::Error> {
    write!(stdout, "{}", termion::color::Bg(termion::color::Black))?;
    write!(stdout, "{}", termion::color::Fg(termion::color::White))?;
    Ok(())
}
fn sort_dir_entries(mut entry: Vec<DirEntry>) -> Vec<DirEntry> {
    entry.sort_by(|a, b| {
        if a.file_type().unwrap().is_dir() && !b.file_type().unwrap().is_dir() {
            Ordering::Less
        } else if !a.file_type().unwrap().is_dir() && b.file_type().unwrap().is_dir() {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    });
    entry
}
fn draw_entry(
    stdout: &mut StdoutLock,
    entry: &DirEntry,
    unicode: bool,
    highlighted: bool,
) -> Result<(), io::Error> {
    if highlighted {
        write!(stdout, "{}", termion::color::Bg(termion::color::White))?;
        write!(stdout, "{}", termion::color::Fg(termion::color::Black))?;
    } else {
        write!(stdout, "{}", termion::color::Bg(termion::color::Black))?;
        write!(stdout, "{}", termion::color::Fg(termion::color::White))?;
    }

    let fname = entry.file_name();

    let alloted_len = CELL_WIDTH - 3;

    let str = fname
        .to_str()
        .unwrap()
        .chars()
        .take(alloted_len as usize)
        .collect::<String>();

    if unicode {
        let entry_type = entry.file_type().unwrap();

        let entry_icon = if entry_type.is_file() {
            ""
        } else if entry_type.is_dir() {
            ""
        } else if entry_type.is_symlink() {
            ""
        } else {
            "󰐧"
        };
        write!(
            stdout,
            " {} {}{}",
            entry_icon,
            str,
            " ".repeat(CELL_WIDTH as usize - str.chars().count() - 3)
        )?;
    } else {
        write!(
            stdout,
            "{}{}",
            str,
            " ".repeat(CELL_WIDTH as usize - str.chars().count())
        )?;
    }

    Ok(())
}
