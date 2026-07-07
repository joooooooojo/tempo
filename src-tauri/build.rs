fn main() {
    tauri_build::build();

    #[cfg(target_os = "macos")]
    embed_info_plist();
}

#[cfg(target_os = "macos")]
fn embed_info_plist() {
    use std::path::Path;

    let plist_path = Path::new("Info.plist");
    if !plist_path.exists() {
        return;
    }

    let Ok(plist_path) = plist_path.canonicalize() else {
        return;
    };

    println!(
        "cargo:rustc-link-arg=-Wl,-sectcreate,__TEXT,__info_plist,{}",
        plist_path.display()
    );
    println!("cargo:rerun-if-changed=Info.plist");
}
