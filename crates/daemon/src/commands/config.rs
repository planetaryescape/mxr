use crate::cli::ConfigAction;

pub fn run(action: Option<ConfigAction>) -> anyhow::Result<()> {
    match action.unwrap_or(ConfigAction::Path) {
        ConfigAction::Path => {
            println!("{}", mxr_config::config_file_path().display());
        }
    }
    Ok(())
}
