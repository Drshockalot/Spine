use clap::Command;
use clap_complete::{Shell, generate};
use anyhow::Result;
use crate::config::Config;

pub fn generate_completions(
    shell: Shell,
    cmd: &mut Command,
    bin_name: &str,
    output: &mut dyn std::io::Write,
) {
    // First generate the base completion
    generate(shell, cmd, bin_name, output);
    
    // Add custom completion information
    match shell {
        Shell::Bash => {
            writeln!(output, "\n# Custom completion for package names").unwrap();
            writeln!(output, "_spine_packages() {{").unwrap();
            writeln!(output, "    local packages").unwrap();
            writeln!(output, "    packages=$(spine list-packages-for-completion 2>/dev/null || echo \"\")").unwrap();
            writeln!(output, "    COMPREPLY=($(compgen -W \"$packages\" -- \"${{COMP_WORDS[COMP_CWORD]}}\"))").unwrap();
            writeln!(output, "}}").unwrap();
            writeln!(output, "").unwrap();
            writeln!(output, "# Override completion for link, unlink, and remove commands").unwrap();
            writeln!(output, "complete -F _spine_packages spine link").unwrap();
            writeln!(output, "complete -F _spine_packages spine unlink").unwrap();
            writeln!(output, "complete -F _spine_packages spine remove").unwrap();
        }
        Shell::Zsh => {
            writeln!(output, "\n# Custom completion for package names").unwrap();
            writeln!(output, "_spine_packages() {{").unwrap();
            writeln!(output, "    local packages").unwrap();
            writeln!(output, "    packages=($(spine list-packages-for-completion 2>/dev/null))").unwrap();
            writeln!(output, "    _describe 'packages' packages").unwrap();
            writeln!(output, "}}").unwrap();
            writeln!(output, "").unwrap();
            writeln!(output, "# Override completion for specific commands").unwrap();
            writeln!(output, "compdef _spine_packages 'spine link'").unwrap();
            writeln!(output, "compdef _spine_packages 'spine unlink'").unwrap();
            writeln!(output, "compdef _spine_packages 'spine remove'").unwrap();
        }
        Shell::Fish => {
            writeln!(output, "\n# Custom completion for package names").unwrap();
            writeln!(output, "function __spine_packages").unwrap();
            writeln!(output, "    spine list-packages-for-completion 2>/dev/null").unwrap();
            writeln!(output, "end").unwrap();
            writeln!(output, "").unwrap();
            writeln!(output, "complete -c spine -n '__fish_seen_subcommand_from link' -a '(__spine_packages)'").unwrap();
            writeln!(output, "complete -c spine -n '__fish_seen_subcommand_from unlink' -a '(__spine_packages)'").unwrap();
            writeln!(output, "complete -c spine -n '__fish_seen_subcommand_from remove' -a '(__spine_packages)'").unwrap();
        }
        _ => {
            // For other shells, just generate basic completion
        }
    }
}

pub fn list_packages_for_completion() -> Result<()> {
    let config = Config::load_or_create()?;
    for package_name in config.links.keys() {
        println!("{}", package_name);
    }
    Ok(())
}