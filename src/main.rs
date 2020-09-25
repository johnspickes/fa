extern crate clap;
extern crate console;
extern crate regex;

use clap::{App, Arg};
use regex::Regex;
use std::fs::File;
use std::io::Write;

// Simplifying:
// - The only time we move the cursor other than one line down is when a regex is found.  
// - Ditch the relative cursor motion and use_lines - I never use it
//    - With absolute cursor motion, we should be more resistant to stderr and other
//      potentially interfering outputs that move the cursor on us without the program
//      knowing

struct Options {
    restart_on_find: bool,
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

    let opt = Options {
        restart_on_find: restart_on_find,
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

#[derive(PartialEq, Clone, Debug)]
enum State {
    Finding,
    Printing,
}

#[derive(Debug)]
struct Space {
    start: i32,   // Starting row of this display space
    rows: i32,    // number of rows in this space
    regex: Regex, // regex which when matched will cause a switch to this space
    state: State, // used to avoid re-starting in this space unless directed
}

impl Space {
    /// Move to the starting row within the space
    fn move_to(
        &self,
        term: &mut console::Term,
    ) -> i32 {
        term.move_cursor_to(0, self.start as usize).unwrap();
        self.start
    }
}

fn search_and_display<T: std::io::BufRead>(input: &mut T, mut opt: Options) {
    let mut term = console::Term::stdout();
    let (rows, cols) = term.size();

    term.clear_screen().unwrap();

    let rows_to_use = rows - 1;

    // Divide the available space up if there's more than one regex
    let mut display_spaces: Vec<Space> = Vec::new();
    let lines_per_space = rows_to_use / (opt.regexes.len() as u16);
    let mut next_line = 0;
    for r in opt.regexes.drain(..) {
        display_spaces.push(Space {
            start: next_line,
            rows: (lines_per_space as i32),
            state: State::Finding,
            regex: r,
        });
        next_line += lines_per_space as i32;
    }

    let dashed_line = "-".repeat((cols-1) as usize) + "\n";

    // Draw dashed lines, if needed, to separate spaces
    for s in display_spaces.iter() {
        s.move_to(&mut term);
        term.write(dashed_line.as_bytes()).unwrap();
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
                    for s in display_spaces.iter_mut() {

                        // If we've changed spaces this loop (to some other space, presumably)
                        // then we're not printing in this space anymore
                        if changed_space {
                            s.state = State::Finding;
                        }

                        if (s.state == State::Finding || opt.restart_on_find) && s.regex.is_match(&l) {
                            // Swapping to a new space.
                            s.move_to(&mut term);
                            s.state = State::Printing;
                            changed_space = true;
                            term.write(dashed_line.as_bytes()).unwrap();
                            lines_printed_this_space = 1;
                        }

                        if s.state == State::Printing {
                            term.clear_line().unwrap();
                            let print_string: String = if l.chars().count() >= cols as usize {
                                l.chars().take((cols - 1) as usize).collect::<String>() + "\n"
                            } else {
                                l.clone()
                            };
                            term.write(print_string.as_bytes()).unwrap();
                            lines_printed_this_space += 1;

                            // Have we reached the end of this space?
                            if lines_printed_this_space >= s.rows {
                                s.state = State::Finding;
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
