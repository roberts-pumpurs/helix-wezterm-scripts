use std::{env, process::Command, usize};

use clap::{Parser, Subcommand};
use color_eyre::owo_colors::OwoColorize;
use eyre::{Context, OptionExt};
use regex::Regex;
use xshell::{cmd, Shell};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    BrootOpen { file: String },
    FzfCallback { output: String },
    Open,
}

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    let sh = Shell::new()?;
    let args = Args::parse();

    match args.command {
        Commands::FzfCallback { output } => {
            // output is in format of "crates/helix-commands/src/main.rs:159:1:fn resize_panes<const N: usize>("
            let output = output.split(':').take(3).collect::<Vec<_>>().join(":");
            let pane = "{left}";
            let command = format!(":open {output}");
            cmd!(sh, "tmux send-keys -t {pane} {command} Enter").run()?;
        }
        Commands::Open => {
            let parsed = parse_helix(&sh)?;
            let line_number = parsed.line_number;
            let filename = parsed.filename;
            cmd!(sh, "gh browse {filename}:{line_number}").run()?;
        }
        Commands::BrootOpen { file } => {
            let pane = "{left}";
            let command = format!(":open {file}");
            cmd!(sh, "tmux send-keys -t {pane} {command} Enter").run()?;
        }
    }

    Ok(())
}

fn get_status_line(sh: &Shell) -> eyre::Result<(String, String)> {
    // Execute a command to get the status, adjust as needed for Tmux
    // For example, you might use `tmux display-message` with a specific format
    let pane = "{left}";
    let output = cmd!(sh, "tmux capture-pane -p -t {pane}").read()?;

    // Define the regex pattern
    output
        .lines()
        .find(|x| x.starts_with(" NOR"))
        .map(|x| {
            let mut items = x.split_whitespace();
            let file_name = items.nth(1).unwrap().to_string();
            let (row, _col) = items.last().unwrap().split_once(":").unwrap();
            (file_name, row.to_string())
        })
        .ok_or_eyre("could not parse")
}

struct ParsedHelx {
    filename: String,
    line_number: String,
    file_extension: String,
    file_name_without_extension: String,
}

fn parse_helix(sh: &Shell) -> Result<ParsedHelx, eyre::Error> {
    let (filename, line_number) = get_status_line(sh)?;
    eprintln!("Filename: {}", filename);
    eprintln!("Line Number: {}", line_number);
    let basename = std::path::Path::new(&filename)
        .file_name()
        .unwrap_or_default();
    let basename_without_extension = basename
        .to_str()
        .unwrap_or("")
        .split('.')
        .next()
        .unwrap_or("");
    let extension = std::path::Path::new(&filename)
        .extension()
        .unwrap_or_default()
        .to_str()
        .unwrap_or("");
    Ok(ParsedHelx {
        file_name_without_extension: basename_without_extension.to_string(),
        file_extension: extension.to_string(),
        filename,
        line_number,
    })
}

fn run_command(sh: &Shell, pane_id: &str, command: &str) -> Result<(), eyre::Error> {
    // Tmux send-keys sends the command followed by Enter
    cmd!(sh, "tmux send-keys -t {pane_id} \"{command}\" Enter").run()?;
    Ok(())
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl AsRef<str> for Direction {
    fn as_ref(&self) -> &str {
        match self {
            Direction::Left => "left",
            Direction::Right => "right",
            Direction::Up => "up",
            Direction::Down => "down",
        }
    }
}
