use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::info;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub mi_stick: MiStickConfig,
    pub display: DisplayConfig,
    pub input: InputConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MiStickConfig {
    pub ip: String,
    pub adb_port: u16,
    pub adb_path: String,
    pub tunnel_port: u16,
    pub companion_path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DisplayConfig {
    pub mi_stick_position: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InputConfig {
    pub mouse_sensitivity: f32,
}

/// Получает путь к файлу рядом с exe
fn get_app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn get_config_path() -> PathBuf {
    get_app_dir().join("config.toml")
}

pub fn load_config() -> Result<AppConfig, Box<dyn std::error::Error>> {
    let path = get_config_path();

    // Если конфига нет — создаём дефолтный
    if !path.exists() {
        let default_config = AppConfig {
            mi_stick: MiStickConfig {
                ip: "192.168.0.250".to_string(),
                adb_port: 5555,
                adb_path: "adb.exe".to_string(),
                tunnel_port: 7878,
                companion_path: "companion".to_string(),
            },
            display: DisplayConfig {
                mi_stick_position: "left".to_string(),
            },
            input: InputConfig {
                mouse_sensitivity: 1.0,
            },
        };
        let content = toml::to_string_pretty(&default_config)?;
        fs::write(&path, content)?;
        info!("Создан дефолтный конфиг: {:?}", path);
        return Ok(default_config);
    }

    let content = fs::read_to_string(&path)?;
    let config: AppConfig = toml::from_str(&content)?;
    info!("Конфиг: Mi Stick @ {}:{}", config.mi_stick.ip, config.mi_stick.adb_port);
    Ok(config)
}

pub fn save_config(config: &AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    let path = get_config_path();
    let content = toml::to_string_pretty(config)?;
    fs::write(&path, content)?;
    Ok(())
}