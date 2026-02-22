fn main() {
    // Embed Windows manifest to request admin elevation
    // WTG Assistant needs admin for DISM, diskpart, bcdboot, etc.
    #[cfg(target_os = "windows")]
    {
        let mut res = tauri_build::WindowsAttributes::new();
        res = res.app_manifest(include_str!("wtg-tauri.exe.manifest"));
        let attrs = tauri_build::Attributes::new().windows_attributes(res);
        tauri_build::try_build(attrs).expect("failed to run tauri build");
    }

    #[cfg(not(target_os = "windows"))]
    {
        tauri_build::build();
    }
}
