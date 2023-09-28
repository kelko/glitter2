extern crate clap;

use clap::Parser;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;

use glitter::{process, report};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// File name of the Input. `-` for stdin (default)
    input: Option<PathBuf>,

    /// File name of the Output. `-` for stdout (default)
    output: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();

    let input_path = cli.input.unwrap_or_else(|| PathBuf::from("-"));
    let output_path = cli.output.unwrap_or_else(|| PathBuf::from("-"));

    let stdin = io::stdin();
    let stdout = io::stdout();
    let starting_directory: String;
    let filename: String;

    let cwd = std::env::current_dir()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let mut input_reader: Box<dyn BufRead> = if input_path == PathBuf::from("-") {
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

    let mut output_writer: Box<dyn Write> = if output_path == PathBuf::from("-") {
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

    if output_writer.flush().is_err() {
        eprintln!("Could not flush output. File might not contain all content");
        std::process::exit(exitcode::IOERR);
    }
}
