use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::info;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    #[serde(default)]
    pub mi_stick: MiStickConfig,
    #[serde(default)]
    pub display: DisplayConfig,
    #[serde(default)]
    pub input: InputConfig,
    #[serde(default)]
    pub scrcpy: ScrcpyConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MiStickConfig {
    pub ip: String,
    pub adb_port: u16,
    pub adb_path: String,
    pub tunnel_port: u16,
    pub companion_path: String,
}

impl Default for MiStickConfig {
    fn default() -> Self {
        Self {
            ip: "192.168.0.250".to_string(),
            adb_port: 5555,
            adb_path: "adb\\adb.exe".to_string(),
            tunnel_port: 7878,
            companion_path: "companion".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DisplayConfig {
    pub mi_stick_position: String,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self { mi_stick_position: "left".to_string() }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InputConfig {
    pub mouse_sensitivity: f32,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self { mouse_sensitivity: 1.0 }
    }
}



#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScrcpyConfig {
    pub enabled: bool,
    pub path: String,
    pub device: String,
    pub extra_args: Vec<String>,
}

impl Default for ScrcpyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: "C:\\scrcpy\\scrcpy.exe".to_string(),
            device: "192.168.0.201:5555".to_string(),
            extra_args: vec![
                "--no-video".to_string(),
                "--no-window".to_string(),
                "--audio-codec=opus".to_string(), 
                "--audio-buffer=50".to_string(),
            ],
        }
    }
}

fn get_app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn get_config_path() -> PathBuf {
    get_app_dir().join("config.toml")
}

fn default_config() -> AppConfig {
    AppConfig::default()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            mi_stick: MiStickConfig::default(),
            display: DisplayConfig::default(),
            input: InputConfig::default(),
            scrcpy: ScrcpyConfig::default(),
        }
    }
}

pub fn load_config() -> Result<AppConfig, Box<dyn std::error::Error>> {
    let path = get_config_path();

    if !path.exists() {
        let config = default_config();
        let content = toml::to_string_pretty(&config)?;
        fs::write(&path, content)?;
        info!("Создан дефолтный конфиг: {:?}", path);
        return Ok(config);
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