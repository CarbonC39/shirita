#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};

/// 由 app data 基目录推出 (db_path, assets_dir)。纯函数，便于单测。
#[cfg_attr(not(test), allow(dead_code))]
fn data_paths(base: &Path) -> (PathBuf, PathBuf) {
    (base.join("shirita.db"), base.join("assets"))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_paths_derives_db_and_assets() {
        let (db, assets) = data_paths(Path::new("/data"));
        assert_eq!(db, Path::new("/data/shirita.db"));
        assert_eq!(assets, Path::new("/data/assets"));
    }
}
