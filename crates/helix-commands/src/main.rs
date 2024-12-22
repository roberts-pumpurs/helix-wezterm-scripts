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
    Blame,
    Explorer,
    Fzf,
    FzfCallback { output: String },
    Open,
    Gitui,
    GitTree,
    Serpl,
    WezSetupPanes,
    WezFormatPanes,
    WezLargeTerminal,
    WezSmallTerminal,
}

const DEFAULT_PANE_COUNT: usize = 3;
const DEFAULT_PANES_SIZES: [u64; DEFAULT_PANE_COUNT] = [10, 60, 30];
const LARGE_TERMINAL_LAYOUT: [u64; DEFAULT_PANE_COUNT] = [10, 40, 50];
const SMALL_TERMINAL_LAYOUT: [u64; DEFAULT_PANE_COUNT] = [10, 95, 10];

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    let sh = Shell::new()?;
    let args = Args::parse();
    let current_pane_id = std::env::var("TMUX_PANE")
        .wrap_err("TMUX_PANE environment variable not set. Are you inside a tmux session?")?;

    match args.command {
        Commands::Blame => {
            let parsed = parse_helix(&sh)?;
            let pane_id = get_or_split_pane(&sh, Direction::Right, &current_pane_id)?;
            let command = format!("tig blame {} +{}", parsed.filename, parsed.line_number);
            run_command(&sh, &pane_id, &command)?;
            focus_pane(&sh, &pane_id)?;
        }
        Commands::Explorer => {
            let pane_id = get_or_split_pane(&sh, Direction::Left, &current_pane_id)?;
            let command = "bo".to_string();
            run_command(&sh, &pane_id, &command)?;
            focus_pane(&sh, &pane_id)?;
        }
        Commands::Fzf => {
            let pane_id = get_or_split_pane(&sh, Direction::Right, &current_pane_id)?;
            let current_exe = env::current_exe()?.to_str().unwrap().to_owned();

            let command_1 = "rg --line-number --column --no-heading --smart-case .".to_string();
            let command_2 =
                "fzf --delimiter : --preview 'bat --style=full --color=always --highlight-line {2} {1}' --preview-window '~3,+{2}+3/2'"
                    .to_string();
            let command = format!("{} | {}", command_1, command_2);
            let command_3 = format!(r#"{} fzf-callback "$({})""#, current_exe, command);
            // output is in format of "crates/helix-commands/src/main.rs:159:1:fn resize_panes<const N: usize>("
            run_command(&sh, &pane_id, &command_3)?;
            focus_pane(&sh, &pane_id)?;
        }
        Commands::FzfCallback { output } => {
            // output is in format of "crates/helix-commands/src/main.rs:159:1:fn resize_panes<const N: usize>("
            let output = output.split(':').take(3).collect::<Vec<_>>().join(":");
            // we are focused on the terminal, therefore helix is to the left
            let pane_id = get_or_split_pane(&sh, Direction::Left, &current_pane_id)?;

            let command = format!(":open {}\r", output);
            run_command(&sh, &pane_id, &command)?;
            focus_pane(&sh, &pane_id)?;
        }
        Commands::Open => {
            let parsed = parse_helix(&sh)?;
            let line_number = parsed.line_number;
            let filename = parsed.filename;
            cmd!(sh, "gh browse {filename}:{line_number}").run()?;
        }
        Commands::WezSetupPanes => {
            setup(&sh, &current_pane_id)?;
        }
        Commands::WezFormatPanes => {
            let panes = setup_initial_panes(&sh, &current_pane_id)?;
            let (current_size, total_cells) = get_pane_sizes(&sh, &panes)?;
            resize_panes(&sh, DEFAULT_PANES_SIZES, total_cells, current_size, &panes)?;
        }
        Commands::WezLargeTerminal => {
            let panes = setup_initial_panes(&sh, &current_pane_id)?;
            let (current_size, total_cells) = get_pane_sizes(&sh, &panes)?;
            resize_panes(
                &sh,
                LARGE_TERMINAL_LAYOUT,
                total_cells,
                current_size,
                &panes,
            )?;
        }
        Commands::WezSmallTerminal => {
            let panes = setup_initial_panes(&sh, &current_pane_id)?;
            let (current_size, total_cells) = get_pane_sizes(&sh, &panes)?;
            resize_panes(
                &sh,
                SMALL_TERMINAL_LAYOUT,
                total_cells,
                current_size,
                &panes,
            )?;
        }
        Commands::Gitui => {
            let pane_id = get_or_split_pane(&sh, Direction::Right, &current_pane_id)?;
            let command = "gitui".to_string();
            run_command(&sh, &pane_id, &command)?;
            focus_pane(&sh, &pane_id)?;
        }
        Commands::GitTree => {
            let pane_id = get_or_split_pane(&sh, Direction::Right, &current_pane_id)?;
            let command = "git-igitt".to_string();
            run_command(&sh, &pane_id, &command)?;
            focus_pane(&sh, &pane_id)?;
        }
        Commands::Serpl => {
            let pane_id = get_or_split_pane(&sh, Direction::Right, &current_pane_id)?;
            let command = "serpl".to_string();
            run_command(&sh, &pane_id, &command)?;
            focus_pane(&sh, &pane_id)?;
        }
    }

    Ok(())
}

fn get_or_split_pane(
    sh: &Shell,
    direction: Direction,
    current_pane: &str,
) -> Result<String, eyre::Error> {
    let split_flag = match direction {
        Direction::Left => "-hb",
        Direction::Up => "-vb",
        Direction::Right => "-h",
        Direction::Down => "-v",
    };
    let new_pane_id = cmd!(
        sh,
        "tmux split-window {split_flag} -t {current_pane} -P -F '#{{pane_id}}'"
    )
    .read()?;
    Ok(new_pane_id)
}

fn setup(sh: &Shell, current_pane_id: &str) -> eyre::Result<()> {
    let panes = setup_initial_panes(sh, current_pane_id)?;
    let (current_size, total_cells) = get_pane_sizes(sh, &panes)?;

    // Split panes according to DEFAULT_PANES_SIZES
    resize_panes(sh, DEFAULT_PANES_SIZES, total_cells, current_size, &panes)?;

    // Open 'bo' on left
    let pane_id = get_or_split_pane(sh, Direction::Left, current_pane_id)?;
    let command = "bo".to_string();
    run_command(sh, &pane_id, &command)?;

    // Focus on the middle pane
    focus_pane(sh, current_pane_id)?;
    Ok(())
}

fn resize_panes<const N: usize>(
    sh: &Shell,
    sizes_in_percent: [u64; N],
    total_cells: u64,
    current_size: [u64; N],
    panes: &[String; N],
) -> Result<(), eyre::Error> {
    // Calculate desired sizes based on percentages
    let desired_sizes = sizes_in_percent.map(|x| (x * total_cells) / 100);
    let diffs: Vec<i64> = desired_sizes
        .iter()
        .zip(current_size.iter())
        .map(|(&desired, &current)| (desired as i64) - (current as i64))
        .collect();

    let shrink_directions = [Direction::Left, Direction::Left, Direction::Left];
    let grow_directions = [Direction::Right, Direction::Right, Direction::Right];

    for i in 0..N - 1 {
        let diff = diffs[i] as i64;
        let direction = if diff < 0 {
            grow_directions[i]
        } else {
            shrink_directions[i]
        };
        let amount = diff.abs().to_string();
        let pane_id = &panes[i];
        let direction_str = direction.as_ref();
        // Tmux resize-pane uses positive for increasing size and negative for decreasing
        let resize_amount = if direction == Direction::Right || direction == Direction::Down {
            format!("{}", amount)
        } else {
            format!("-{}", amount)
        };
        cmd!(
            sh,
            "tmux resize-pane {direction_str} -t {pane_id} {resize_amount}"
        )
        .run()?;
    }
    Ok(())
}

fn get_pane_sizes(sh: &Shell, panes: &[String; 3]) -> Result<([u64; 3], u64), eyre::Error> {
    let mut current_size = [0_u64; 3];
    let output = cmd!(sh, "tmux list-panes -F '#{{pane_id}} #{pane_width}'").read()?;
    let pane_info = extract_pane_id_and_size(&output);
    let total_cells: u64 = pane_info
        .iter()
        .filter(|(pane_id, _)| panes.contains(pane_id))
        .map(|(_, size)| *size)
        .sum();
    for (pane_id, size) in pane_info {
        if let Some(idx) = panes.iter().position(|p| p == &pane_id) {
            current_size[idx] = size;
        }
    }
    Ok((current_size, total_cells))
}

fn setup_initial_panes(sh: &Shell, current_pane_id: &str) -> Result<[String; 3], eyre::Error> {
    let pane_id_left = get_or_split_pane(sh, Direction::Left, current_pane_id)?;
    focus_pane(sh, current_pane_id)?;
    let pane_id_right = get_or_split_pane(sh, Direction::Right, current_pane_id)?;
    focus_pane(sh, current_pane_id)?;
    let panes = [pane_id_left, current_pane_id.to_string(), pane_id_right];
    Ok(panes)
}

fn focus_pane(sh: &Shell, pane_id: &str) -> Result<(), eyre::Error> {
    cmd!(sh, "tmux select-pane -t {pane_id}").run()?;
    Ok(())
}

fn extract_pane_id_and_size(input: &str) -> Vec<(String, u64)> {
    input
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                return None;
            }
            let pane_id = parts[0].to_string();
            let size = parts[1].parse::<u64>().ok()?;
            Some((pane_id, size))
        })
        .collect()
}

fn get_status_line(sh: &Shell) -> eyre::Result<(String, String)> {
    // Execute a command to get the status, adjust as needed for Tmux
    // For example, you might use `tmux display-message` with a specific format
    let output = cmd!(
        sh,
        "tmux display-message -p '#{{pane_current_path}}:#{{pane_current_command}}'"
    )
    .read()?;

    // Define the regex pattern
    let re = Regex::new(
        r"(?x)
        (?P<filename>\S+):   # Capture the filename
        (?P<line_number>\d+) # Capture the line number
    ",
    )?;

    // Apply the regex pattern
    if let Some(caps) = re.captures(&output) {
        let filename = caps.name("filename").map_or("", |m| m.as_str()).to_string();
        let line_number = caps
            .name("line_number")
            .map_or("", |m| m.as_str())
            .to_string();
        Ok((filename, line_number))
    } else {
        Err(eyre::eyre!("Failed to parse status line"))
    }
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
