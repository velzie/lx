use faccess::PathExt;
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
    input::{MouseTerminal, TermRead},
    raw::IntoRawMode,
    terminal_size,
};

use getopts::{Matches, Options};
use std::env;
const CELL_WIDTH: u16 = 20;
const BOX_OFFSET: u16 = 2;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let mut opts = Options::new();
    let program = args[0].clone();

    opts.optflag("a", "all", "show hidden files (starting with a dot)");
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

    let mut stdout = MouseTerminal::from(term.lock());

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

    let mut scrolly = 0;
    loop {
        let readdir = fs::read_dir(pwd.clone())?.map(|f| f.unwrap());

        let contents: Vec<fs::DirEntry> = sort_dir_entries(if !opt_matches.opt_present("all") {
            readdir
                .filter(|f| !f.file_name().to_str().unwrap().starts_with("."))
                .collect()
        } else {
            readdir.collect()
        });

        let menu_items: Vec<MenuItem> = vec![MenuItem::Back]
            .into_iter()
            .chain(contents.into_iter().map(|f| MenuItem::DirEntry(f)))
            .chain(vec![MenuItem::Close].into_iter())
            .collect();
        // rust one-liners hit different for some reason

        let entries_per_row = (menu_items.len() as u16).min(size.0 / CELL_WIDTH - 1);
        default_color(stdout)?;
        // TODO:
        // don't rerender everything
        // panics upon entering a dir without permission
        let totalwidth = CELL_WIDTH * entries_per_row;

        dbg!(menu_items.len() as u16 / entries_per_row);
        let scrollable = (menu_items.len() as u16 / entries_per_row + BOX_OFFSET) > size.1;
        dbg!(scrollable);
        let extrachars = 4;
        let pwdstr = pwd
            .to_str()
            .unwrap()
            .chars()
            .take((totalwidth as i32 - extrachars as i32).max(0) as usize)
            .collect::<String>();
        if !scrollable {
            term_cursor::set_pos(0, start_y).unwrap();
        } else {
            print!("{}", termion::clear::All);
            start_y = 1;
            term_cursor::set_pos(0, 0).unwrap();
        }
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
        for idx in (scrolly * entries_per_row as usize)..menu_items.len() {
            let item = &menu_items[idx];
            let row_y = idx / entries_per_row as usize;
            if idx % entries_per_row as usize == 0 {
                if row_y as u16 - scrolly as u16 + BOX_OFFSET >= size.1 {
                    break;
                }

                if !scrollable && start_y + row_y as i32 >= size.1 as i32 {
                    start_y -= 1;
                    write!(stdout, "\n")?;
                }

                let end = CELL_WIDTH as i32 * entries_per_row as i32 + entries_per_row as i32 + 1;
                term_cursor::set_pos(0, start_y - scrolly as i32 + row_y as i32 + 1).unwrap();
                write!(stdout, "{}", " ".repeat(end as usize))?;
                term_cursor::set_pos(end, start_y - scrolly as i32 + row_y as i32 + 1).unwrap();
                write!(stdout, "│")?;
                term_cursor::set_pos(0, start_y - scrolly as i32 + row_y as i32 + 1).unwrap();
                write!(stdout, "│")?;
            }

            draw_item(
                stdout,
                item,
                opt_matches.opt_present("use-unicode"),
                idx == selected_idx,
            )?;
            default_color(stdout)?;
            if idx % entries_per_row as usize != entries_per_row as usize - 1 {
                write!(stdout, " ")?;
            }
        }
        let endy =
            start_y + ((menu_items.len().max(1) - 1) / entries_per_row.max(1) as usize) as i32 + 2;
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
        let selected_item = menu_items.get(selected_idx);
        let Ok(e) = events.next().unwrap() else { continue };
        match e {
            Event::Key(Key::Backspace) => {
                let newdir = pwd.parent().unwrap().to_path_buf();
                pwd = newdir;
                selected_idx = 0;
                true_clear(stdout, start_y, size, totalwidth)?;
            }
            Event::Key(Key::Esc) => {
                break;
            }
            Event::Key(Key::Char('\n')) => {
                if let Some(item) = selected_item {
                    match item {
                        MenuItem::DirEntry(entry) => {
                            if let Some(pathbuf) = getentry_dir_path(entry) {
                                let accessable =
                                    Path::new(&pathbuf).access(faccess::AccessMode::READ);
                                if matches!(accessable, Ok(_)) {
                                    pwd = pathbuf;
                                    selected_idx = 0;
                                    true_clear(stdout, start_y, size, totalwidth)?;
                                }
                            }
                        }
                        MenuItem::Close => return Ok(pwd),
                        MenuItem::Back => {
                            let newdir = pwd.parent().unwrap().to_path_buf();
                            pwd = newdir;
                            selected_idx = 0;
                            true_clear(stdout, start_y, size, totalwidth)?;
                        }
                    }
                }
            }
            Event::Key(Key::Right) => {
                selected_idx = (selected_idx + 1).min(menu_items.len().max(1) - 1);
            }
            Event::Key(Key::Left) => {
                if selected_idx > 0 {
                    selected_idx -= 1;
                }
            }
            Event::Key(Key::Up) => {
                selected_idx = (selected_idx as i32 - entries_per_row as i32).max(0) as usize;
                if scrollable
                    && ((selected_idx as u16 / entries_per_row) + BOX_OFFSET - scrolly as u16
                        <= BOX_OFFSET)
                    && scrolly > 0
                {
                    scrolly -= 1;
                }
            }
            Event::Key(Key::Down) => {
                selected_idx =
                    (selected_idx + entries_per_row as usize).min(menu_items.len().max(1) - 1);
                if scrollable
                    && ((selected_idx as u16 / entries_per_row) + BOX_OFFSET - scrolly as u16
                        >= size.1)
                {
                    scrolly += 1;
                }
            }
            Event::Key(Key::Char(ch)) => {
                // move idx to next of ch
                let mut i = selected_idx + 1;
                while i != selected_idx {
                    if let MenuItem::DirEntry(entry) = &menu_items[i] {
                        let fname = entry.file_name();
                        let name = fname.as_os_str().to_str().unwrap().to_lowercase();
                        if name.starts_with(ch) {
                            selected_idx = i;
                            break;
                        }
                    }
                    i += 1;
                    if i >= menu_items.len() {
                        i = 0;
                    }
                }
            }
            Event::Mouse(m) => match m {
                termion::event::MouseEvent::Release(x, y) => {
                    // dbg!(x, y);
                }
                _ => {}
            },
            _ => {}
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

fn getentry_dir_path(entry: &DirEntry) -> Option<PathBuf> {
    let entrytype = entry.file_type().unwrap();
    if entrytype.is_dir() {
        Some(entry.path().clone())
    } else if entrytype.is_symlink() {
        let cannonical = entry.path().canonicalize().unwrap();
        if cannonical.is_dir() {
            Some(cannonical)
        } else {
            None
        }
    } else {
        None
    }
}

fn true_clear(
    stdout: &mut StdoutLock,
    start_y: i32,
    size: (u16, u16),
    width: u16,
) -> Result<(), io::Error> {
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
fn draw_item(
    stdout: &mut StdoutLock,
    item: &MenuItem,
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

    let max_width = CELL_WIDTH;

    let formatted = match item {
        MenuItem::DirEntry(entry) => fmt_dir_entry(entry, unicode, max_width),
        MenuItem::Back => String::from("Go Back"),
        MenuItem::Close => String::from("Exit"),
    };

    write!(
        stdout,
        "{}{}",
        formatted,
        " ".repeat(max_width as usize - formatted.chars().count())
    )?;

    Ok(())
}
fn fmt_dir_entry(entry: &DirEntry, unicode: bool, max_width: u16) -> String {
    let fname = entry.file_name();

    let alloted_len = max_width - 3;

    let entry_str = fname
        .to_str()
        .unwrap()
        .chars()
        .take(alloted_len as usize)
        .collect::<String>();

    // TODO: FLAGS FOR COLOR OPTIONS

    if !unicode {
        return entry_str;
    }
    let entry_type = entry.file_type().unwrap();

    let entry_icon = if entry_type.is_file() {
        ""
    } else if entry_type.is_dir() {
        let accessable =
                                    Path::new(&entry.path()).access(faccess::AccessMode::READ);
        match accessable{
            Ok(_)=>{
                ""

            }
            Err(_)=>{
                "󱪨"   
            }
        }
    } else if entry_type.is_symlink() {
        ""
    } else {
        "󰐧"
    };
    format!(" {} {}", entry_icon, entry_str)
}

enum MenuItem {
    DirEntry(DirEntry),
    Back,
    Close,
}
