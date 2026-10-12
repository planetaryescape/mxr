use crate::cli::BackupAction;
use anyhow::Context;

pub async fn run(action: BackupAction) -> anyhow::Result<()> {
    let manifest = match action {
        BackupAction::Create {
            source_data,
            source_config,
            output,
            confirm,
        } => crate::backup::create(&source_data, source_config.as_deref(), &output, !confirm)
            .await
            .with_context(|| {
                format!(
                    "{} backup at {}",
                    if confirm { "create" } else { "preview" },
                    output.display()
                )
            })?,
        BackupAction::Inspect { bundle } => crate::backup::inspect(&bundle)
            .await
            .with_context(|| format!("inspect backup at {}", bundle.display()))?,
        BackupAction::Restore {
            bundle,
            destination,
            confirm,
        } => crate::backup::restore(&bundle, &destination, !confirm)
            .await
            .with_context(|| {
                format!(
                    "{} restore into {}",
                    if confirm { "perform" } else { "preview" },
                    destination.display()
                )
            })?,
    };
    println!("{}", serde_json::to_string_pretty(&manifest)?);
    Ok(())
}
