extern crate clap;
extern crate console;
extern crate regex;

use clap::{App, Arg};
use regex::Regex;
use std::fs::File;
use std::io::Write;
use std::collections::VecDeque;
use std::iter::repeat;
use std::cmp;

/// Options collected from the command line
struct Options {
    /// Should display restart each time the regex is found, or should we fill out
    /// the available space before triggering again?
    restart_on_find: bool,
    /// Number of lines of history to display before matched line
    history_lines: u32,
    /// Should a space be completely cleared before starting over?
    clear_on_restart: bool,
    /// All the regexes that should be searched.  One display space will be created
    /// for each of these
    regexes: Vec<Regex>,
}

/// Validates u16 command line values
#[allow(dead_code)]
fn u16_validator(s: String) -> Result<(), String> {
    match s.parse::<u16>() {
        Ok(_) => Ok(()),
        Err(_) => Err(String::from("Argument must be a non-negative integer")),
    }
}

/// Validates regex command line values
#[allow(dead_code)]
fn regex_validator(s: String) -> Result<(), String> {
    match Regex::new(&s) {
        Ok(_) => Ok(()),
        Err(_) => Err(String::from("Invalid regular expression")),
    }
}

fn main() {
    // Parse the command line using clap
    let matches = App::new("fa")
        .version("0.2.0")
        .author("John Spickes <john@spickes.net>")
        .about("Human-friendly wall-of-text handler")
        .arg(Arg::with_name("REGEX")
             .help("Regular expression to find in the input")
             .required(true)
             .validator(regex_validator)
             .multiple(true)
             .index(1))
        .arg(Arg::with_name("INPUT")
             .help("The file/pipe to use as input.  If omitted, stdin is used")
             .takes_value(true)
             .short("f")
             .long("file"))
        .arg(Arg::with_name("history_lines")
             .help("Number of history lines to display before matched lines, default zero")
             .takes_value(true)
             .validator(u16_validator)
             .short("h")
             .long("history"))
        .arg(Arg::with_name("clear_on_restart")
             .help("When restarting display, clear the whole display region.")
             .long("clear_on_restart")
             .short("c"))
        .arg(Arg::with_name("restart_on_find")
             .help("Restart display each time REGEX is found again, without waiting for the screen to fill")
             .long("restart_on_find")
             .short("r"))
       .get_matches();

    // Unwrapping is appropriate here because REGEX is a required
    // argument and we shouldn't get here if it's not present.
    let regexes: Vec<Regex> = matches
        .values_of("REGEX")
        .unwrap()
        .map(|s| Regex::new(s).unwrap())
        .collect();

    let restart_on_find = matches.is_present("restart_on_find");
    let clear_on_restart = matches.is_present("clear_on_restart");
    let history_lines = if matches.is_present("history_lines") {
        matches.value_of("history_lines").unwrap().parse().unwrap()
    } else { 0 };

    let opt = Options {
        restart_on_find: restart_on_find,
        clear_on_restart: clear_on_restart,
        regexes: regexes,
        history_lines: history_lines,
    };

    match matches.value_of("INPUT") {
        Some(filename) => {
            if let Ok(f) = File::open(filename) {
                let mut reader = std::io::BufReader::new(f);
                search_and_display(&mut reader, opt);
            } else {
                eprintln!("Unable to open {}", filename);
            }
        }
        _ => {
            search_and_display(&mut std::io::stdin().lock(), opt);
        }
    }
}

/// State of each display space.
#[derive(PartialEq, Clone, Debug)]
enum State {
    /// Searching for a regex match
    Finding,
    /// Found a match, and printing lines - at most one space may have this state
    Printing,
}

/// Data for each display space
#[derive(Debug)]
struct Space {
    /// Starting row of this display space
    start: i32,
    /// number of rows in this space
    rows: i32,
    /// regex which when matched will cause a switch to this space
    regex: Regex,
    /// used to avoid re-starting in this space unless directed
    state: State,
    /// Header string to be printed at the top of the space
    header: String,
}

impl Space {
    /// Move to the starting row within the space
    fn move_to( &self, term: &mut console::Term,) -> i32 {
        term.move_cursor_to(0, self.start as usize).unwrap();
        self.start
    }
}

/// Do the main work of reading the input and writing to the display
fn search_and_display<T: std::io::BufRead>(input: &mut T, mut opt: Options) {
    let mut term = console::Term::stdout();
    let (rows, cols) = term.size();

    term.clear_screen().unwrap();

    let rows_to_use = rows - 2;

    let mut history : VecDeque<String> = VecDeque::new();

    // Populate history with an appropriate number of blank lines.
    for _ in 0..opt.history_lines {
        history.push_back("\n".into());
    }

    // Divide the available space up if there's more than one regex
    let mut display_spaces: Vec<Space> = Vec::new();
    let lines_per_space = rows_to_use / (opt.regexes.len() as u16);
    let mut next_line = 0;
    for r in opt.regexes.drain(..) {
        let rows = cmp::min(lines_per_space, rows_to_use - next_line);
        let header_text = format!("[ {:?} ]", &r);
        let full_header = repeat('-').take(3).chain(header_text.chars())
            .chain(repeat('-')).take((cols-1) as usize).collect::<String>() + "\n";

        display_spaces.push(Space {
            start: next_line as i32,
            rows: rows as i32,
            state: State::Finding,
            regex: r,
            header: full_header,
        });
        next_line += lines_per_space as u16;
    }

    // Draw headers, to separate spaces
    for s in display_spaces.iter() {
        s.move_to(&mut term);
        term.write(s.header.as_bytes()).unwrap();
    }

    let mut lines_printed_this_space = 0;

    loop {
        let mut changed_space = false;
        let mut l = String::new();
        match input.read_line(&mut l) {
            Ok(n) => {
                if n == 0 {
                    // This indicates EOF
                    break;
                } else {
                    // Got a line.
                    let print_string: String = if l.chars().count() >= cols as usize {
                        l.chars().take((cols - 1) as usize).collect::<String>() + "\n"
                    } else {
                        l.clone()
                    };
                    for s in display_spaces.iter_mut() {

                        // If we've changed spaces this loop (to some other space, presumably)
                        // then we're not printing in this space anymore
                        if changed_space {
                            s.state = State::Finding;
                        }

                        if (s.state == State::Finding || opt.restart_on_find) && s.regex.is_match(&l) {
                            // Swapping to a new space.
                            // Clear the space if desired
                            if opt.clear_on_restart {
                                for r in s.start..s.start+s.rows {
                                    term.move_cursor_to(0, r as usize).unwrap();
                                    term.clear_line().unwrap();
                                }
                            }
                            s.move_to(&mut term);
                            s.state = State::Printing;
                            changed_space = true;
                            lines_printed_this_space = 0;
                            term.write(s.header.as_bytes()).unwrap();
                            lines_printed_this_space += 1;
                            // Insert the history
                            for h in history.iter() {
                                term.clear_line().unwrap();
                                term.write(h.as_bytes()).unwrap();
                                lines_printed_this_space += 1;
                            }
                        }

                        if s.state == State::Printing {
                            // Have we reached the end of this space?
                            if lines_printed_this_space >= s.rows {
                                s.state = State::Finding;
                            } else {
                                term.clear_line().unwrap();
                                term.write(print_string.as_bytes()).unwrap();
                                lines_printed_this_space += 1;
                            }
                        }
                    }

                    // Put this line in the history
                    if opt.history_lines > 0 {
                        history.push_back(print_string);
                        while history.len() > opt.history_lines as usize {
                            history.pop_front();
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Error: {:?}", e);
            }
        }
    }
}
