extern crate clap;
extern crate console;
extern crate regex;

use clap::{App, Arg};
use regex::Regex;
use std::fs::File;
use std::io::Write;

struct Options {
    restart_on_find: bool,
    use_lines: bool,
    lines_to_use: u16,
    regexes: Vec<Regex>,
}

fn u16_validator(s: String) -> Result<(), String> {
    match s.parse::<u16>() {
        Ok(_) => Ok(()),
        Err(_) => Err(String::from("Argument must be a non-negative integer")),
    }
}

fn regex_validator(s: String) -> Result<(), String> {
    match Regex::new(&s) {
        Ok(_) => Ok(()),
        Err(_) => Err(String::from("Invalid regular expression")),
    }
}

fn main() {
    // Parse the command line using clap
    let matches = App::new("fa")
        .version("0.1.0")
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
        .arg(Arg::with_name("restart_on_find")
             .help("Restart display each time REGEX is found again, without waiting for the screen to fill")
             .long("restart_on_find")
             .short("r"))
        .arg(Arg::with_name("LINES")
             .help("Use the specified number of lines to display, instead of clearing the screen and using it all")
             .long("use_lines")
             .short("l")
             .validator(u16_validator)
             .takes_value(true))
        .get_matches();

    // Unwrapping is appropriate here because REGEX is a required
    // argument and we shouldn't get here if it's not present.
    let regexes: Vec<Regex> = matches
        .values_of("REGEX")
        .unwrap()
        .map(|s| Regex::new(s).unwrap())
        .collect();

    let restart_on_find = matches.is_present("restart_on_find");

    let use_lines = matches.is_present("LINES");
    let lines: u16 = if use_lines {
        // Both unwraps are safe because we know use_lines is present, and
        // the argument is validated by clap.
        matches.value_of("LINES").unwrap().parse().unwrap()
    } else {
        0
    };

    let opt = Options {
        restart_on_find: restart_on_find,
        use_lines: use_lines,
        lines_to_use: lines,
        regexes: regexes,
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

#[derive(PartialEq, Clone)]
enum State {
    Finding,
    Printing,
}

struct Cursor {
    row: i32, // positive is down.  zero is home position
}

impl Cursor {
    fn move_down(&mut self, term: &mut console::Term, rows: i32) -> std::io::Result<()> {
        self.row += rows;
        if rows >= 0 {
            term.move_cursor_down(rows as usize)
        } else {
            term.move_cursor_up(-rows as usize)
        }
    }

    fn move_up(&mut self, term: &mut console::Term, rows: i32) -> std::io::Result<()> {
        self.move_down(term, -rows)
    }

    fn move_to(&mut self, term: &mut console::Term, target: i32) -> std::io::Result<()> {
        let delta = target - self.row;
        self.move_down(term, delta)
    }

    fn home(&mut self, term: &mut console::Term) -> std::io::Result<()> {
        self.move_to(term, 0)
    }
}

#[derive(Debug)]
struct Space {
    start: i32,   // Starting row of this display space
    end: i32,     // Ending (inclusive) row
    current: i32, // Current row, relative to start
}

impl Space {
    /// Move to a particular row within the space (relative to start)
    fn move_to(
        &mut self,
        term: &mut console::Term,
        c: &mut Cursor,
        target: i32,
    ) -> std::io::Result<()> {
        let row = self.start + target;
        self.current = target;
        c.move_to(term, row)
    }

    /// bump up the current row
    fn advance(&mut self) {
        self.current += 1;
    }

    /// Go to the current row
    fn restore(&mut self, term: &mut console::Term, c: &mut Cursor) -> std::io::Result<()> {
        c.move_to(term, self.current + self.start)
    }

    /// Have we passed the end?
    fn past_end(&mut self) -> bool {
        self.current > (self.end - self.start)
    }
}

fn search_and_display<T: std::io::BufRead>(input: &mut T, opt: Options) {
    let mut term = console::Term::stdout();
    let (rows, cols) = term.size();

    if !opt.use_lines {
        term.clear_screen().unwrap();
    }

    let mut crsr = Cursor { row: 0 };

    let mut st: Vec<State> = Vec::new();
    st.resize(opt.regexes.len(), State::Finding);

    let rows_to_use = if opt.use_lines {
        opt.lines_to_use
    } else {
        // Using rows-1 prevents the screen from scrolling when we reach the last line
        (rows - 1)
    };

    // Divide the available space up if there's more than one regex
    let mut display_spaces: Vec<Space> = Vec::new();
    let lines_per_space = rows_to_use / (opt.regexes.len() as u16);
    let mut next_line = 0;
    for i in 0..opt.regexes.len() {
        display_spaces.push(Space {
            start: next_line,
            end: next_line + (lines_per_space as i32) + if i == opt.regexes.len() - 1 { 0 } else { -1 },
            current: 0,
        });
        next_line += lines_per_space as i32;
    }

    // Draw dashed lines, if needed, to separate spaces
    if display_spaces.len() > 1 {
        let mut dashed_line = String::new();
        for _ in 0..cols {
            dashed_line.push('-');
        }
        for i in 0..display_spaces.len() - 1 {
            crsr.move_to(&mut term, display_spaces[i].end + 1).unwrap();
            term.write(dashed_line.as_bytes()).unwrap();
        }
    }

    loop {
        let mut l = String::new();
        match input.read_line(&mut l) {
            Ok(n) => {
                if n == 0 {
                    // This indicates EOF
                    break;
                } else {
                    // Got a line.
                    for (i, r) in opt.regexes.iter().enumerate() {
                        // If finding, or restarting on new finds, check for match
                        if ((st[i] == State::Finding) || opt.restart_on_find) && r.is_match(&l) {
                            // Move back up to the row where we started
                            display_spaces[i].move_to(&mut term, &mut crsr, 0).unwrap();
                            st[i] = State::Printing;
                        }

                        if st[i] == State::Printing {
                            display_spaces[i].restore(&mut term, &mut crsr).unwrap();
                            term.clear_line().unwrap();
                            let print_string: String = if l.chars().count() >= cols as usize {
                                l.chars().take((cols - 1) as usize).collect::<String>() + "\n"
                            } else {
                                l.clone()
                            };
                            term.write(print_string.as_bytes()).unwrap();
                            display_spaces[i].advance();
                            // Have we reached the end of the usable space?
                            if display_spaces[i].past_end() {
                                // Go back to finding
                                st[i] = State::Finding;
                            }
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
