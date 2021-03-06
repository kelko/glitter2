#![warn(rust_2018_idioms)]

#[macro_use]
extern crate clap;

use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};

use exitcode;

use glitter::{process, report};

fn main() {
    let options = clap_app!(rodata =>
        (version: "0.1")
        (author: ":kelko:")
        (about: "glitter Template Processor")
        (@arg INPUT: +required "The glitter file to process")
        (@arg OUTPUT: +required "The path to the output file")
    )
    .get_matches();

    let input_path = options
        .value_of("INPUT")
        .expect("Missing required parameter INPUT")
        .to_string();
    let output_path = options
        .value_of("OUTPUT")
        .expect("Missing required parameter OUTPUT")
        .to_string();

    let stdin = io::stdin();
    let stdout = io::stdout();
    let starting_directory: String;
    let filename: String;

    let cwd = std::env::current_dir()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let mut input_reader: Box<dyn BufRead> = if input_path == "-" {
        filename = "-".to_owned();
        starting_directory = cwd;
        Box::new(stdin.lock())
    } else {
        let path = std::path::Path::new(&input_path);
        filename = if let Some(file) = path.file_name() {
            file.to_str().unwrap().to_owned()
        } else {
            eprintln!("INPUT must point to a valid file");
            std::process::exit(exitcode::CONFIG);
        };

        starting_directory = if let Some(parent_dir) = path.parent() {
            parent_dir
                .to_str()
                .expect(
                    "No clue how this can happen that there is a file without any parent directory",
                )
                .to_owned()
        } else {
            cwd
        };

        let input_file = if let Ok(file) = File::open(input_path) {
            file
        } else {
            eprintln!("Could not open input file");
            std::process::exit(exitcode::NOINPUT);
        };

        Box::new(BufReader::new(input_file))
    };

    let mut output_writer: Box<dyn Write> = if output_path == "-" {
        Box::new(stdout.lock())
    } else {
        let output_file = if let Ok(file) = File::create(output_path) {
            file
        } else {
            eprintln!("Could not open output file");
            std::process::exit(exitcode::CANTCREAT);
        };

        Box::new(BufWriter::new(output_file))
    };

    if let Err(error) = process(
        &mut input_reader,
        filename,
        starting_directory,
        &mut output_writer,
    ) {
        report(&error);
        std::process::exit(exitcode::SOFTWARE);
    }

    if let Err(_) = output_writer.flush() {
        eprintln!("Could not flush output. File might not contain all content");
        std::process::exit(exitcode::IOERR);
    }
}
