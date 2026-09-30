fn main() {
    // build.rs는 호스트에서 실행되므로 교차 빌드도 대상 OS를 기준으로 판단한다.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=app.manifest");
        let attrs = tauri_build::Attributes::new().windows_attributes(
            tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest")),
        );
        tauri_build::try_build(attrs).expect("failed to run tauri-build");
    } else {
        tauri_build::build();
    }
}
