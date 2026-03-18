pub fn run(reindex: bool) -> anyhow::Result<()> {
    let data_dir = mxr_config::data_dir();
    let db_path = data_dir.join("mxr.db");
    let index_path = data_dir.join("search_index");
    let socket_path = crate::state::AppState::socket_path();

    println!("Data dir:     {}", data_dir.display());
    println!(
        "Database:     {} (exists: {})",
        db_path.display(),
        db_path.exists()
    );
    println!(
        "Search index: {} (exists: {})",
        index_path.display(),
        index_path.exists()
    );
    println!(
        "Socket:       {} (exists: {})",
        socket_path.display(),
        socket_path.exists()
    );
    println!("Config:       {}", mxr_config::config_file_path().display());

    if reindex {
        println!("\nReindex requested - this requires daemon restart to take effect.");
        if index_path.exists() {
            std::fs::remove_dir_all(&index_path)?;
            println!("Removed search index directory. Restart daemon to rebuild.");
        }
    }

    Ok(())
}
