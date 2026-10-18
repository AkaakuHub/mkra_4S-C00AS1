use std::{env, path::Path};

fn main() {
    println!("cargo:rerun-if-env-changed=YAKISOBA_LIB_DIR");
    if env::var_os("CARGO_FEATURE_SOFTWARE_CAS").is_some() {
        let directory = env::var("YAKISOBA_LIB_DIR")
            .expect("software-cas requires YAKISOBA_LIB_DIR containing libyakisoba.a");
        let archive = Path::new(&directory).join("libyakisoba.a");
        assert!(archive.is_file(), "libyakisoba.a is missing");
        println!("cargo:rerun-if-changed={}", archive.display());
        println!("cargo:rustc-link-search=native={directory}");
        println!("cargo:rustc-link-lib=static=yakisoba");
    }
}
