use serde::Deserialize;
use std::fs;
use tracing::info;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub mi_stick: MiStickConfig,
    pub display: DisplayConfig,
    pub input: InputConfig,
    pub hotkeys: HotkeysConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MiStickConfig {
    pub ip: String,
    pub adb_port: u16,
    pub adb_path: String,
    pub tunnel_port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DisplayConfig {
    pub mi_stick_position: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InputConfig {
    pub mouse_sensitivity: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HotkeysConfig {
    pub switch_to_mi_stick: String,
    pub switch_to_windows: String,
}

pub fn load_config() -> Result<AppConfig, Box<dyn std::error::Error>> {
    let content = fs::read_to_string("config.toml")?;
    let config: AppConfig = toml::from_str(&content)?;
    info!("Конфигурация загружена: Mi Stick @ {}:{}", config.mi_stick.ip, config.mi_stick.adb_port);
    Ok(config)
}