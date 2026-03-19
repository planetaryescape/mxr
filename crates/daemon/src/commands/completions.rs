use clap::CommandFactory;
use clap_complete::{generate, Shell};

pub fn run(shell: String) -> anyhow::Result<()> {
    let shell: Shell = shell.parse().map_err(|_| {
        anyhow::anyhow!(
            "Unknown shell '{}'. Supported: bash, zsh, fish, powershell, elvish",
            shell
        )
    })?;
    let mut cmd = crate::cli::Cli::command();
    generate(shell, &mut cmd, "mxr", &mut std::io::stdout());
    Ok(())
}
