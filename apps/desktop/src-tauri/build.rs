fn main() {
    // Windows: 커스텀 SxS 매니페스트를 임베드한다(app.manifest).
    // 그 안의 <msix> 요소가 exe 를 Sparse Package(신원 패키지)와 연결 → 패키지 ID 부여.
    #[cfg(windows)]
    {
        let attrs = tauri_build::Attributes::new().windows_attributes(
            tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest")),
        );
        tauri_build::try_build(attrs).expect("failed to run tauri-build");
    }
    #[cfg(not(windows))]
    {
        tauri_build::build();
    }
}
