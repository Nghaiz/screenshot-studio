fn main() {
    // Deterministic link (phase 4.3): MSVC link.exe stamps a time and a random PDB GUID into every exe, so a relink
    // of unchanged code (a `vite build` rewrites dist/, the build script reruns) gave a different file and the
    // Accept scripts' installed-exe check failed. /Brepro derives both from the content instead.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/Brepro");
    }
    tauri_build::build()
}
