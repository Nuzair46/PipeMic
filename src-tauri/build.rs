fn main() {
    if std::env::var_os("CARGO_FEATURE_APP").is_some() {
        let vb_cable_package = std::path::Path::new("vendor/vb-cable/VBCABLE_Driver_Pack45.zip");
        println!("cargo:rerun-if-changed={}", vb_cable_package.display());
        assert!(
            vb_cable_package.exists(),
            "missing VB-CABLE package: place the official VBCABLE_Driver_Pack45.zip at src-tauri/vendor/vb-cable/VBCABLE_Driver_Pack45.zip"
        );
        tauri_build::build();
    }
}
