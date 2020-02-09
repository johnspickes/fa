extern crate clap;
extern crate regex;
extern crate console;

use clap::{Arg, App};
use std::fs::File;
use regex::Regex;
use std::io::Write;

struct Options {
    restart_on_find : bool,
    use_lines : bool,
    lines_to_use : u32,
}

fn u32_validator(s: String) -> Result<(), String> {
    match s.parse::<u32>() {
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
             .index(1))
        .arg(Arg::with_name("INPUT")
             .help("The file/pipe to use as input.  If omitted, stdin is used")
             .index(2))
        .arg(Arg::with_name("restart_on_find")
             .help("Restart display each time REGEX is found again, without waiting for the screen to fill")
             .long("restart_on_find")
             .short("r"))
        .arg(Arg::with_name("LINES")
             .help("Use the specified number of lines to display, instead of clearing the screen and using it all")
             .long("use_lines")
             .short("l")
             .validator(u32_validator)
             .takes_value(true))
        .get_matches();

    // Unwrapping is appropriate here because REGEX is a required
    // argument and we shouldn't get here if it's not present.
    let regex_str = matches.value_of("REGEX").unwrap();
    // regex has already been validated by clap, so unwrap is safe
    let regex = Regex::new(regex_str).unwrap();

    let restart_on_find = matches.is_present("restart_on_find");

    let use_lines = matches.is_present("LINES");
    let lines: u32 = if use_lines {
        // Both unwraps are safe because we know use_lines is present, and
        // the argument is validated by clap.
        matches.value_of("LINES").unwrap().parse().unwrap()
    } else { 0 };

    let opt = Options {
        restart_on_find: restart_on_find,
        use_lines: use_lines,
        lines_to_use: lines,
    };

    match matches.value_of("INPUT") {
        Some(filename) => {
            if let Ok(f) = File::open(filename) {
                let mut reader = std::io::BufReader::new(f);
                search_and_display(&mut reader, &regex, opt);
            } else {
                eprintln!("Unable to open {}", filename);
            }
        }
        _ => {
            search_and_display(&mut std::io::stdin().lock(), &regex, opt);
        }
    }
}


#[derive(PartialEq)]
enum State {
    Finding,
    Printing
}

fn search_and_display<T: std::io::BufRead>(input: &mut T, regex: &Regex,
                                           opt: Options) {
    let mut term = console::Term::stdout();
    let (rows, cols) = term.size();

    if !opt.use_lines {
        term.clear_screen().unwrap();
    }

    let mut current_row = 0;

    let mut st = State::Finding;

    let rows_to_use = if opt.use_lines {
        opt.lines_to_use as usize
    } else {
        // Using rows-1 prevents the screen from scrolling when we reach the last line
        (rows-1) as usize
    };

    loop {
        let mut l = String::new();
        match input.read_line(&mut l) {
            Ok(n) => {
                if n == 0 {
                    // This indicates EOF
                    break;
                } else {
                    // Got a line.
                    // If finding, or restarting on new finds, check for match
                    if ((st == State::Finding) || opt.restart_on_find) && regex.is_match(&l) {
                        // Move back up to the row where we started
                        term.move_cursor_up(current_row).unwrap();
                        current_row = 0;
                        st = State::Printing;
                    }

                    if st == State::Printing {
                        term.clear_line().unwrap();
                        let print_string: String = if l.chars().count() >= cols as usize {
                            l.chars().take((cols-1) as usize).collect::<String>() + "\n"
                        } else { l };
                        term.write(print_string.as_bytes()).unwrap();
                        current_row += 1;
                        // Have we reached the end of the usable space?
                        if current_row >= rows_to_use {
                            // Go back to finding
                            st = State::Finding;
                        }
                    }
                }
            },
            Err(e) => {
                eprintln!("Error: {:?}", e);
            }
        }
    }
}
