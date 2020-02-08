extern crate clap;
extern crate regex;
extern crate console;

use clap::{Arg, App};
use std::fs::File;
use regex::Regex;
use std::io::Write;

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
        .get_matches();

    // Unwrapping is appropriate here because REGEX is a required
    // argument and we shouldn't get here if it's not present.
    let regex_str = matches.value_of("REGEX").unwrap();
    let regex = Regex::new(regex_str).unwrap();

    match matches.value_of("INPUT") {
        Some(filename) => {
            if let Ok(f) = File::open(filename) {
                println!("Processing on file {}", filename);
                let mut reader = std::io::BufReader::new(f);
                search_and_display(&mut reader, &regex);
            } else {
                println!("Unable to open {}", filename)
            }
        }
        _ => {
            println!("Using stdin");
            search_and_display(&mut std::io::stdin().lock(), &regex);
        }
    }
}


#[derive(PartialEq)]
enum State {
    Finding,
    Printing
}

fn search_and_display<T: std::io::BufRead>(input: &mut T, regex: &Regex) {
    let mut term = console::Term::stdout();
    let (rows, _cols) = term.size();
    term.clear_screen().unwrap();

    let mut current_row = 0;

    let mut st = State::Finding;

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
                            // println!("match: {}", l);
                            // Found.  Go to top of screen and print.
                            term.move_cursor_to(0, 0).unwrap();
                            term.clear_line().unwrap();
                            term.write(l.as_bytes()).unwrap();
                            // Change to Printing state
                            st = State::Printing;
                            current_row += 1;
                        } 
                    } else {
                        // Printing
                        // println!("printing: {} on line {} of {}", l, current_row, rows);
                        term.clear_line().unwrap();
                        term.write(l.as_bytes()).unwrap();
                        current_row += 1;
                        // Have we reached the end of the screen?
                        if current_row >= rows {
                            // Go back to finding
                            current_row = 0;
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
