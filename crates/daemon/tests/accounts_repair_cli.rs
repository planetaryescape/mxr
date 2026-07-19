use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn repair_reaches_password_prompt_without_starting_daemon() {
    let temp = TempDir::new().expect("temp dir");
    let data_dir = temp.path().join("data");
    let config_dir = temp.path().join("config");
    let socket_path = temp.path().join("runtime").join("mxr.sock");

    std::fs::create_dir_all(&data_dir).expect("data dir");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    std::fs::write(
        config_dir.join("config.toml"),
        r#"
[accounts.consulting]
name = "Consulting"
email = "consulting@example.com"

[accounts.consulting.sync]
type = "imap"
host = "imap.example.com"
port = 993
username = "consulting@example.com"
password_ref = "mxr/consulting-imap"
use_tls = true
"#,
    )
    .expect("write config");

    Command::cargo_bin("mxr")
        .expect("mxr binary")
        .env("MXR_DATA_DIR", &data_dir)
        .env("MXR_CONFIG_DIR", &config_dir)
        .env("MXR_SOCKET_PATH", &socket_path)
        .env("MXR_INSTANCE", "accounts-repair-cli-test")
        .args(["accounts", "repair", "consulting"])
        .write_stdin("\n")
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "IMAP password for consulting@example.com:",
        ))
        .stderr(predicate::str::contains("Starting daemon").not());
}
