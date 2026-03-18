fn main() {
    // Read .env from workspace root to pick up GMAIL_CLIENT_ID / GMAIL_CLIENT_SECRET
    // at compile time. These get baked into the binary via option_env!().
    if let Ok(contents) = std::fs::read_to_string(
        std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../.env"),
    ) {
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                if key == "GMAIL_CLIENT_ID" || key == "GMAIL_CLIENT_SECRET" {
                    println!("cargo:rustc-env={key}={value}");
                }
            }
        }
    }
    println!("cargo:rerun-if-changed=../../.env");
}
