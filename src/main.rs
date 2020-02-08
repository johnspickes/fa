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
    lines_to_use : i32,
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
             .takes_value(true))
        .get_matches();

    // Unwrapping is appropriate here because REGEX is a required
    // argument and we shouldn't get here if it's not present.
    let regex_str = matches.value_of("REGEX").unwrap();
    let regex = Regex::new(regex_str).unwrap();

    let restart_on_find = matches.is_present("restart_on_find");

    let use_lines = matches.is_present("LINES");
    let lines: i32 = if use_lines {
        // TODO Better error handling
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
                println!("Unable to open {}", filename)
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
    let (rows, _cols) = term.size();

    if !opt.use_lines {
        term.clear_screen().unwrap();
    }

    let mut current_row = 0;

    let mut st = State::Finding;

    let rows_to_use = if opt.use_lines {
        opt.lines_to_use as usize
    } else {
        rows as usize
    };

    loop {
        let mut l = String::new();
        match input.read_line(&mut l) {
            Ok(n) => {
                if n == 0 {
                    break;
                } else {
                    if st == State::Finding {
                        // Finding
                        if regex.is_match(&l) {
                            // Found.  Go to top of screen and print.
                            if opt.use_lines {
                                term.move_cursor_up(current_row).unwrap();
                            } else {
                                term.move_cursor_to(0, 0).unwrap();
                            }
                            current_row = 0;
                            term.clear_line().unwrap();
                            term.write(l.as_bytes()).unwrap();
                            // Change to Printing state
                            st = State::Printing;
                            current_row += 1;
                        } 
                    } else {
                        // Printing
                        if opt.restart_on_find && regex.is_match(&l) {
                            if opt.use_lines {
                                term.move_cursor_up(current_row).unwrap();
                            } else {
                                term.move_cursor_to(0, 0).unwrap();
                            }
                            current_row = 0;
                        }
                        term.clear_line().unwrap();
                        term.write(l.as_bytes()).unwrap();
                        current_row += 1;
                        // Have we reached the end of the screen?
                        if current_row >= rows_to_use {
                            // Go back to finding
                            st = State::Finding;
                        }
                    }
                }
            },
            Err(e) => {
                println!("Error: {:?}", e);
            }
        }
    }
}
