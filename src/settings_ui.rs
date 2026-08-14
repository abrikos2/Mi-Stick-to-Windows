use crate::config::{AppConfig, DisplayConfig, InputConfig, MiStickConfig, ScrcpyConfig};
use eframe::egui;
use std::sync::{Arc, Mutex};
use tracing::{error, info};

#[derive(Clone)]
pub struct SettingsState {
    pub ip: String,
    pub adb_port: String,
    pub tunnel_port: String,
    pub adb_path: String,
    pub companion_path: String, 
    pub sensitivity: f32,
    pub position: String,
    pub status_message: String,
    pub show_window: bool,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            ip: String::new(),
            adb_port: String::new(),
            tunnel_port: String::new(),
            adb_path: String::new(),
            companion_path: String::new(),  
            sensitivity: 1.0,
            position: "left".to_string(),
            status_message: String::new(),
            show_window: false,
        }
    }
}

impl SettingsState {
    pub fn from_config(config: &AppConfig) -> Self {
        Self {
            ip: config.mi_stick.ip.clone(),
            adb_port: config.mi_stick.adb_port.to_string(),
            tunnel_port: config.mi_stick.tunnel_port.to_string(),
            adb_path: config.mi_stick.adb_path.clone(),
            companion_path: config.mi_stick.companion_path.clone(), 
            sensitivity: config.input.mouse_sensitivity,
            position: config.display.mi_stick_position.clone(),
            status_message: String::new(),
            show_window: false,
        }
    }

    pub fn to_config(&self) -> Result<AppConfig, String> {
        let adb_port = self.adb_port.parse::<u16>().map_err(|_| "Неверный ADB порт")?;
        let tunnel_port = self.tunnel_port.parse::<u16>().map_err(|_| "Неверный порт туннеля")?;

        Ok(AppConfig {
            mi_stick: MiStickConfig {
                ip: self.ip.clone(),
                adb_port,
                adb_path: self.adb_path.clone(),
                tunnel_port,
                companion_path: self.companion_path.clone(),  
            },
            display: DisplayConfig {
                mi_stick_position: self.position.clone(),
            },
            input: InputConfig {
                mouse_sensitivity: self.sensitivity,
            },
            scrcpy: ScrcpyConfig::default()
        })
    }
}

pub struct SettingsApp {
    state: Arc<Mutex<SettingsState>>,
    shared_state: Arc<crate::state::SharedState>,
    adb: Arc<tokio::sync::Mutex<crate::adb_bridge::AdbBridge>>,
    tray_tx: flume::Sender<crate::tray::TrayStatus>,
    input_tx: flume::Sender<crate::state::InputEvent>,
    runtime_handle: tokio::runtime::Handle,
    egui_ctx: Mutex<Option<egui::Context>>,  
}

impl SettingsApp {
    pub fn new(
        state: Arc<Mutex<SettingsState>>,
        shared_state: Arc<crate::state::SharedState>,
        adb: Arc<tokio::sync::Mutex<crate::adb_bridge::AdbBridge>>,
        tray_tx: flume::Sender<crate::tray::TrayStatus>,
        input_tx: flume::Sender<crate::state::InputEvent>,
        runtime_handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            state,
            shared_state,
            adb,
            tray_tx,
            input_tx,
            runtime_handle,
            egui_ctx: Mutex::new(None),
        }
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        {
            let mut stored_ctx = self.egui_ctx.lock().unwrap();
            if stored_ctx.is_none() {
                *stored_ctx = Some(ctx.clone());
            }
        }

        let mut state = self.state.lock().unwrap();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("⚙️ Mi Stick Bridge - Настройки");
            ui.separator();

            ui.label("🌐 Подключение");
            ui.horizontal(|ui| {
                ui.label("IP Mi Stick:");
                ui.text_edit_singleline(&mut state.ip);
            });
            ui.horizontal(|ui| {
                ui.label("ADB порт:");
                ui.add(egui::TextEdit::singleline(&mut state.adb_port).desired_width(80.0));
            });
            ui.horizontal(|ui| {
                ui.label("Порт туннеля:");
                ui.add(egui::TextEdit::singleline(&mut state.tunnel_port).desired_width(80.0));
            });
            ui.horizontal(|ui| {
                ui.label("Путь к ADB:");
                ui.add(egui::TextEdit::singleline(&mut state.adb_path).desired_width(300.0));
                if ui.button("📁").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Исполняемые файлы", &["exe"])
                        .pick_file()
                    {
                        state.adb_path = path.to_string_lossy().to_string();
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Companion:");
                ui.add(egui::TextEdit::singleline(&mut state.companion_path).desired_width(300.0));
                if ui.button("📁##companion").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Все файлы", &["*"])
                        .pick_file()
                    {
                        state.companion_path = path.to_string_lossy().to_string();
                    }
                }
            });

            ui.add_space(10.0);
            ui.label("🖥 Экран");
            ui.horizontal(|ui| {
                ui.label("Положение Mi Stick:");
                egui::ComboBox::from_id_source("position")
                    .selected_text(if state.position == "left" { "Слева" } else { "Справа" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.position, "left".to_string(), "Слева");
                    });
            });

            ui.add_space(10.0);
            ui.label("🖱 Ввод");
            ui.horizontal(|ui| {
                ui.label("Чувствительность мыши:");
                ui.add(egui::Slider::new(&mut state.sensitivity, 0.1..=5.0).show_value(true));
            });

            ui.add_space(20.0);
            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("💾 Сохранить и применить").clicked() {
                    match state.to_config() {
                        Ok(new_config) => {
                            let old_config = self.shared_state.get_config();

                            if let Err(e) = save_config(&new_config) {
                                state.status_message = format!("❌ Ошибка файла: {}", e);
                                error!("Ошибка сохранения: {}", e);
                            } else {
                                self.shared_state.update_config(new_config.clone());

                                let need_reconnect = old_config.mi_stick.ip != new_config.mi_stick.ip
                                    || old_config.mi_stick.adb_port != new_config.mi_stick.adb_port
                                    || old_config.mi_stick.tunnel_port != new_config.mi_stick.tunnel_port;

                                if need_reconnect {
                                    state.status_message = "✅ Сохранено! Переподключение...".to_string();
                                    info!("Настройки подключения изменены, переподключаемся...");

                                    let adb = self.adb.clone();
                                    let tray_tx = self.tray_tx.clone();
                                    let input_tx = self.input_tx.clone();
                                    let handle = self.runtime_handle.clone();
                                    let status_state = self.state.clone();
                                    let ctx_clone = self.egui_ctx.lock().unwrap().clone();

                                    handle.spawn(async move {
                                        let _ = tray_tx.send(crate::tray::TrayStatus::Disconnected);
                                        let mut guard = adb.lock().await;
                                        *guard = crate::adb_bridge::AdbBridge::new(new_config.mi_stick);
                                        match guard.reconnect().await {
                                            Ok(reader) => {
                                                guard.start_receiver(input_tx, reader, adb.clone(), tray_tx.clone());
                                                let _ = tray_tx.send(crate::tray::TrayStatus::Windows);
                                                info!("✓ Переподключено с новыми настройками!");
                                                if let Ok(mut s) = status_state.lock() {
                                                    s.status_message = "✅ Переподключено!".to_string();
                                                }
                                            }
                                            Err(e) => {
                                                error!("Переподключение не удалось: {}", e);
                                                if let Ok(mut s) = status_state.lock() {
                                                    s.status_message = format!("❌ Ошибка: {}", e);
                                                }
                                            }
                                        }
                                        if let Some(ctx) = ctx_clone {
                                            ctx.request_repaint();
                                        }
                                    });
                                } else {
                                    state.status_message = "✅ Сохранено и применено!".to_string();
                                    info!("Настройки применены на лету");
                                }
                            }
                        }
                        Err(e) => {
                            state.status_message = format!("❌ {}", e);
                        }
                    }
                }

                if ui.button("🔌 Тест подключения").clicked() {
                    state.status_message = "🔄 Проверяем...".to_string();
                    let ip = state.ip.clone();
                    let state_clone = self.state.clone();
                    let ctx_clone = self.egui_ctx.lock().unwrap().clone();

                    std::thread::spawn(move || {
                        let result = std::process::Command::new("ping")
                            .args(["-n", "2", "-w", "1000", &ip])
                            .output();

                        let status = match result {
                            Ok(output) if output.status.success() => {
                                format!("✅ {} доступен (ping OK)", ip)
                            }
                            _ => format!("❌ {} недоступен", ip),
                        };

                        info!("{}", status);

                        if let Ok(mut s) = state_clone.lock() {
                            s.status_message = status;
                        }

                        if let Some(ctx) = ctx_clone {
                            ctx.request_repaint();
                        }
                    });
                }

                if ui.button("🔄 Сбросить").clicked() {
                    match crate::config::load_config() {
                        Ok(config) => {
                            *state = SettingsState::from_config(&config);
                            state.status_message = "↩️ Загружено из файла".to_string();
                        }
                        Err(e) => {
                            state.status_message = format!("❌ {}", e);
                        }
                    }
                }
            });

            if !state.status_message.is_empty() {
                ui.add_space(10.0);
                ui.label(&state.status_message);
            }
        });
    }
}

fn save_config(config: &AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    crate::config::save_config(config)
}
pub fn open_settings_window(
    settings_state: Arc<Mutex<SettingsState>>,
    shared_state: Arc<crate::state::SharedState>,
    adb: Arc<tokio::sync::Mutex<crate::adb_bridge::AdbBridge>>,
    tray_tx: flume::Sender<crate::tray::TrayStatus>,
    input_tx: flume::Sender<crate::state::InputEvent>,
    runtime_handle: tokio::runtime::Handle,
) {
    {
        let mut s = settings_state.lock().unwrap();
        if s.show_window {
            return;
        }
        s.show_window = true;
    }

    std::thread::spawn(move || {
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([550.0, 500.0])
                .with_resizable(false),
            event_loop_builder: Some(Box::new(|builder| {
                #[cfg(target_os = "windows")]
                {
                    use winit::platform::windows::EventLoopBuilderExtWindows;
                    builder.with_any_thread(true);
                }
            })),
            ..Default::default()
        };

        let ss = settings_state.clone();
        let shared = shared_state.clone();
        let adb_clone = adb.clone();
        let tray_tx_clone = tray_tx.clone();
        let handle_clone = runtime_handle.clone();

        if let Err(e) = eframe::run_native(
            "Mi Stick Bridge - Настройки",
            options,
            Box::new(move |_cc| {
                Ok(Box::new(SettingsApp::new(
                    ss, shared, adb_clone, tray_tx_clone, input_tx, handle_clone,
                )))
            }),
        ) {
            error!("Ошибка окна настроек: {}", e);
        }

        let mut s = settings_state.lock().unwrap();
        s.show_window = false;
    });
}