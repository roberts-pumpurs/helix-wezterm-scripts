use std::{env, ops::Index, process::Command, usize};

use clap::{Parser, Subcommand};
use color_eyre::owo_colors::OwoColorize;
use eyre::OptionExt;
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
    let current_pane_id = std::env::var("WEZTERM_PANE")?.parse()?;

    match args.command {
        Commands::Blame => {
            let ParsedHelx {
                filename,
                line_number,
                ..
            } = parse_helix(&sh)?;
            let pane_id = get_or_split_pane(&sh, Direction::Right, current_pane_id)?;
            let command = format!("tig blame {filename} +{line_number}");
            run_command(&sh, pane_id, command)?;
            focus_pane(&sh, pane_id)?;
        }
        Commands::Explorer => {
            let pane_id = get_or_split_pane(&sh, Direction::Left, current_pane_id)?;
            let command = format!("bo");
            run_command(&sh, pane_id, command)?;
            focus_pane(&sh, pane_id)?;
        }
        Commands::Fzf => {
            let pane_id = get_or_split_pane(&sh, Direction::Right, current_pane_id)?;
            let current_exe = env::current_exe()?.to_str().unwrap().to_owned();

            let command_1 = format!("rg --line-number --column --no-heading --smart-case .");
            let command_2 = format!("fzf --delimiter : --preview 'bat --style=full --color=always --highlight-line {{2}} {{1}}' --preview-window '~3,+{{2}}+3/2'");
            let command = [command_1, command_2].join(" | ");
            let command_3 = format!(r#"{current_exe} fzf-callback "$({command})""#);
            // output is in format of "crates/helix-commands/src/main.rs:159:1:fn resize_panes<const N: usize>("
            run_command(&sh, pane_id, command_3)?;
            focus_pane(&sh, pane_id)?;
        }
        Commands::FzfCallback { output } => {
            // output is in format of "crates/helix-commands/src/main.rs:159:1:fn resize_panes<const N: usize>("
            let output = output.split(':').take(3).collect::<Vec<_>>();
            let output = output.join(":");
            // we are focused on the terminal, therefore helix is to the left
            let pane_id = get_or_split_pane(&sh, Direction::Left, current_pane_id)?;

            let command = format!(":open {output}\r");
            run_command(&sh, pane_id, command)?;
            focus_pane(&sh, pane_id)?;
        }
        Commands::Open => {
            let ParsedHelx {
                filename,
                line_number,
                ..
            } = parse_helix(&sh)?;
            cmd!(sh, "gh browse {filename}:{line_number}").run()?;
        }
        Commands::WezSetupPanes => {
            setup(&sh, current_pane_id)?;
        }
        Commands::WezFormatPanes => {
            let panes = setup_initial_panes(&sh, current_pane_id)?;
            let (current_size, total_cells) = get_pane_sizes(&sh, &panes)?;
            resize_panes(&sh, DEFAULT_PANES_SIZES, total_cells, current_size, panes)?;
        }
        Commands::WezLargeTerminal => {
            let panes = setup_initial_panes(&sh, current_pane_id)?;
            let (current_size, total_cells) = get_pane_sizes(&sh, &panes)?;
            resize_panes(&sh, LARGE_TERMINAL_LAYOUT, total_cells, current_size, panes)?;
        }
        Commands::WezSmallTerminal => {
            let panes = setup_initial_panes(&sh, current_pane_id)?;
            let (current_size, total_cells) = get_pane_sizes(&sh, &panes)?;
            resize_panes(&sh, SMALL_TERMINAL_LAYOUT, total_cells, current_size, panes)?;
        }
        Commands::Gitui => {
            let pane_id = get_or_split_pane(&sh, Direction::Right, current_pane_id)?;
            let command = format!("gitui");
            run_command(&sh, pane_id, command)?;
            focus_pane(&sh, pane_id)?;
        }
        Commands::GitTree => {
            let pane_id = get_or_split_pane(&sh, Direction::Right, current_pane_id)?;
            let command = format!("git-igitt");
            run_command(&sh, pane_id, command)?;
            focus_pane(&sh, pane_id)?;
        }
        Commands::Serpl => {
            let pane_id = get_or_split_pane(&sh, Direction::Right, current_pane_id)?;
            let command = format!("serpl");
            run_command(&sh, pane_id, command)?;
            focus_pane(&sh, pane_id)?;
        }
    }

    Ok(())
}

fn get_or_split_pane(
    sh: &Shell,
    direction: Direction,
    current_pane: u64,
) -> Result<u64, eyre::Error> {
    let direction = direction.as_ref();
    let current_pane = current_pane.to_string();
    let pane_id = cmd!(
        sh,
        "wezterm cli get-pane-direction --pane-id {current_pane} {direction}"
    )
    .read()?;
    let pane_id = if pane_id.is_empty() {
        cmd!(
            sh,
            "wezterm cli split-pane --{direction} --pane-id {current_pane}"
        )
        .read()?
    } else {
        pane_id
    };
    Ok(pane_id.parse()?)
}

fn setup(sh: &Shell, current_pane_id: u64) -> eyre::Result<()> {
    let panes = setup_initial_panes(sh, current_pane_id)?;
    let (current_size, total_cells) = get_pane_sizes(sh, &panes)?;

    // split panes
    resize_panes(sh, DEFAULT_PANES_SIZES, total_cells, current_size, panes)?;

    // open bo on left
    let pane_id = get_or_split_pane(&sh, Direction::Left, current_pane_id)?;
    let command = format!("bo");
    run_command(&sh, pane_id, command)?;

    // focus on middle
    focus_pane(&sh, current_pane_id)?;
    Ok(())
}

fn resize_panes<const N: usize>(
    sh: &Shell,
    sizes_in_percent: [u64; N],
    total_cells: u64,
    current_size: [u64; N],
    panes: [u64; N],
) -> Result<(), eyre::Error> {
    let cell_in_percent = (total_cells / 100).max(1);
    let desired_sizes = sizes_in_percent.map(|x| x * cell_in_percent);
    let diff = desired_sizes
        .iter()
        .zip(current_size)
        .map(|(desired, current)| (current as i128) - (*desired as i128))
        .collect::<Vec<_>>();
    let mut shrink_direction = [Direction::Left].repeat(N - 1);
    shrink_direction.push(Direction::Right);

    let mut grow_directoin = [Direction::Right].repeat(N - 1);
    grow_directoin.push(Direction::Left);
    Ok(
        for (((pane_id, diff), shrink_dir), grow_dir) in panes
            .iter()
            .zip(diff)
            .zip(shrink_direction)
            .zip(grow_directoin)
            // we skip the last one
            .take(N - 1)
        {
            let direction = if diff.is_negative() {
                grow_dir
            } else {
                shrink_dir
            };
            let desired_size = diff.abs().to_string();
            let pane_id = pane_id.to_string();
            cmd!(sh, "wezterm cli activate-pane --pane-id {pane_id}")
                .quiet()
                .run()?;
            let direction = direction.as_ref();
            cmd!(
            sh,
            "wezterm cli adjust-pane-size --pane-id {pane_id} --amount {desired_size} {direction}"
        )
            .run()?;
        },
    )
}

fn get_pane_sizes(sh: &Shell, panes: &[u64; 3]) -> Result<([u64; 3], u64), eyre::Error> {
    let mut current_size = [0_u64; 3];
    let pane_info = cmd!(sh, "wezterm cli list").read()?;
    let pane_info = extract_pane_id_and_size(&pane_info);
    let pane_info = pane_info
        .into_iter()
        .filter(|(pane_id, _)| panes.contains(&pane_id))
        .collect::<Vec<_>>();
    let total_cells = pane_info.iter().map(|x| x.1 as u64).sum::<u64>();
    for (pane_id, size) in pane_info.iter() {
        let idx = panes.iter().position(|x| x == pane_id).unwrap();
        current_size[idx] = *size as u64;
    }
    Ok((current_size, total_cells))
}

fn setup_initial_panes(sh: &Shell, current_pane_id: u64) -> Result<[u64; 3], eyre::Error> {
    let pane_id_left = get_or_split_pane(&sh, Direction::Left, current_pane_id)?;
    focus_pane(sh, current_pane_id)?;
    let pane_id_right = get_or_split_pane(&sh, Direction::Right, current_pane_id)?;
    focus_pane(sh, current_pane_id)?;
    let panes = [pane_id_left, current_pane_id, pane_id_right];
    Ok(panes)
}

fn focus_pane(sh: &Shell, pane_id: u64) -> Result<(), eyre::Error> {
    let pane_id = pane_id.to_string();
    cmd!(sh, "wezterm cli activate-pane --pane-id {pane_id}")
        .quiet()
        .run()?;
    Ok(())
}

fn extract_pane_id_and_size(input: &str) -> Vec<(u64, u64)> {
    input
        .lines()
        .skip(1) // Skip the header line
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 5 {
                return None;
            }
            let pane_id = parts[2].to_string().parse::<u64>().unwrap();
            let size = parts[4].to_string();
            let (x, _y) = size.split_once('x')?;
            let x = x.parse::<u64>().unwrap();
            Some((pane_id, x))
        })
        .collect()
}

fn get_status_line(sh: &Shell) -> eyre::Result<(String, String)> {
    // Execute the wezterm cli get-text command to get the text output
    let output = cmd!(sh, "wezterm cli get-text").read()?;

    // Define the regex pattern
    let re = Regex::new(
        r"(?x)
        (?:NOR\s+|NORMAL|INS\s+|INSERT|SEL\s+|SELECT)\s+  
        [\x{2800}-\x{28FF}]*\s+                           
        (\S*)\s[^│]*                                      
        (\d+):*.*                                         
    ",
    )?;

    // Apply the regex pattern
    if let Some(caps) = re.captures(&output) {
        let filename = caps.get(1).map_or("", |m| m.as_str()).to_string();
        let line_number = caps.get(2).map_or("", |m| m.as_str()).to_string();
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

fn run_command(sh: &Shell, pane_id: u64, mut command: String) -> Result<(), eyre::Error> {
    command += "\n";
    let pane_id = pane_id.to_string();
    cmd!(
        sh,
        "wezterm cli send-text --pane-id {pane_id} --no-paste {command}"
    )
    .quiet()
    .run()?;
    Ok(())
}

#[derive(Debug, Copy, Clone)]
enum Direction {
    Left,
    Right,
}

impl AsRef<str> for Direction {
    fn as_ref(&self) -> &str {
        match self {
            Direction::Left => "left",
            Direction::Right => "right",
        }
    }
}
