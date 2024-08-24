use std::process::Command;

use clap::{Parser, Subcommand};
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
    Check,
    Explorer,
    Fzf,
    Open,
}

fn split_pane_right(sh: &Shell) -> eyre::Result<String> {
    let pane_id = cmd!(sh, "wezterm cli get-pane-direction right")
        .read()
        .ok()
        .or_else(|| cmd!(sh, "wezterm cli split-pane --right").read().ok())
        .ok_or_eyre("could not get pane id")?;
    println!("pane id {pane_id}");

    cmd!(
        sh,
        "echo wezterm cli activate-pane-direction --pane-id {pane_id} right"
    )
    .run()?;

    Ok(pane_id)
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

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    let sh = Shell::new()?;
    let args = Args::parse();

    let (filename, line_number) = get_status_line(&sh)?;

    println!("Filename: {}", filename);
    println!("Line Number: {}", line_number);

    let pwd = sh.current_dir();
    let basedir = std::path::Path::new(&filename).parent().unwrap_or(&pwd);
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

    match args.command {
        Commands::Blame => {
            let pane_id = split_pane_right(&sh)?;
            // let command = cmd!(sh, "cd {pwd}; tig blame {filename} +{line_number}").to_string();
            // Command::new("")
            //     .args([r#"echo "ls -lha" | wezterm cli send-text --pane-id 11 --no-paste"#])
            //     .spawn()
            //     .unwrap();
            // cmd!(
            //     sh,
            //     "wezterm cli send-text --pane-id 11 --no-paste ls -lha" // "wezterm cli send-text --pane-id 11 --no-paste \"123\""
            // )
            // .run()?;
            // let pane_id = "11";
            let command = "ls -lha\n";
            cmd!(
                sh,
                "wezterm cli send-text --pane-id {pane_id} --no-paste {command}"
            )
            .run()?;
        }
        Commands::Check => {
            split_pane_right(&sh)?;
            if extension == "rs" {
                let run_command = format!(
                    "cd {}/{}; cargo check; if [ $? = 0 ]; wezterm cli activate-pane-direction up; end;",
                    pwd.display(),
                    filename.replace("src/.*$", "")
                );
                cmd!(sh, "wezterm cli send-text --no-paste {run_command}").run()?;
            }
        }
        Commands::Explorer => {
            let left_pane_id = cmd!(sh, "wezterm cli get-pane-direction left")
                .read()
                .ok()
                .or_else(|| {
                    cmd!(sh, "wezterm cli split-pane --left --percent 20")
                        .read()
                        .ok()
                });

            if let Some(ref pane_id) = left_pane_id {
                let left_program = cmd!(
                    sh,
                    "wezterm cli list | awk -v pane_id={pane_id} '$3==pane_id {{ print $6 }}'"
                )
                .read()?;
                if left_program.trim() != "br" {
                    cmd!(
                        sh,
                        "wezterm cli send-text --pane-id {pane_id} --no-paste 'bo'"
                    )
                    .run()?;
                }
                cmd!(sh, "wezterm cli activate-pane-direction left").run()?;
            }
        }
        Commands::Fzf => {
            split_pane_right(&sh)?;
            let fzf_command = format!(
                "cd {pwd}; ~/.config/helix/helix-fzf.sh $(rg --line-number --column --no-heading --smart-case . | fzf --delimiter : --preview 'bat --style=full --color=always --highlight-line {{2}} {{1}}' --preview-window '~3,+{{2}}+3/2' | awk '{{ print $1 }}' | cut -d: -f1,2,3)",
                pwd = pwd.display()
            );
            cmd!(sh, "wezterm cli send-text --no-paste {fzf_command}").run()?;
        }
        Commands::Open => {
            cmd!(sh, "gh browse {filename}:{line_number}").run()?;
        }
    }

    Ok(())
}
