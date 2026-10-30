fn main() {
    // tauri.conf.json takes its version from ../package.json, which tauri_build does not watch.
    println!("cargo:rerun-if-changed=../package.json");
    tauri_build::build()
}
