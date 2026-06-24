use crate::models::FormData;
use std::path::PathBuf;

fn config_dir() -> PathBuf {
    let mut p = std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(&std::path::Path::new("."))
        .to_path_buf();
    p.push("data");
    p
}

fn config_path() -> PathBuf {
    let mut p = config_dir();
    p.push("config.json");
    p
}

pub fn load_config() -> FormData {
    let path = config_path();
    if !path.exists() {
        return FormData::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => FormData::default(),
    }
}

pub fn save_config(data: &FormData) -> Result<(), String> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    std::fs::write(config_path(), json).map_err(|e| e.to_string())
}
