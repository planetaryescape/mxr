fn main() {
    let manifest = mxr_tui::desktop_manifest::desktop_manifest();
    println!("{}", serde_json::to_string_pretty(&manifest).unwrap());
}
