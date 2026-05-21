use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let db_path = std::env::args()
        .nth(1)
        .expect("usage: create_db <sqlite-db-path>");
    let _store = mxr_store::Store::new(Path::new(&db_path)).await?;
    Ok(())
}
