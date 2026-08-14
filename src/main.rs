#![windows_subsystem = "windows"]
mod adb_bridge;
mod config;
mod input_blocker;
mod input_capture;
mod settings_ui;
mod state;
mod tray;
mod win_api;
mod scrcpy_launcher;


use adb_bridge::AdbBridge;
use config::load_config;
use flume::unbounded;
use input_blocker::{set_blocking, start_hooks};
use settings_ui::SettingsState;
use state::{AppState, InputEvent, SharedState};
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as TokioMutex;
use tracing::{error, info, warn};
use tray::{TrayCommand, TrayStatus};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("=== Mi Stick Bridge v0.8 ===");

    let config = match load_config() {
        Ok(c) => c,
        Err(e) => { error!("Config error: {}", e); return; }
    };

    let runtime_handle = tokio::runtime::Handle::current();

    let (tray_cmd_rx, tray_status_tx) = tray::start_tray();
    let (tx, rx) = unbounded::<InputEvent>();
    let adb = Arc::new(TokioMutex::new(AdbBridge::new(config.mi_stick.clone())));

    let shared_state = Arc::new(SharedState::new(config.clone()));
    let settings_state = Arc::new(Mutex::new(SettingsState::from_config(&config)));

    {
        let mut guard = adb.lock().await;
        match guard.connect().await {
            Ok(reader) => {
                guard.start_receiver(tx.clone(), reader, adb.clone(), tray_status_tx.clone());
                let _ = tray_status_tx.send(TrayStatus::Windows);
            }
            Err(e) => {
                error!("Автоподключение не удалось: {}", e);
                let _ = tray_status_tx.send(TrayStatus::Disconnected);

                let adb_clone = adb.clone();
                let tx_clone = tx.clone();
                let tray_clone = tray_status_tx.clone();
                let handle = runtime_handle.clone();
                handle.spawn(async move {
                    loop {
                        info!("Попытка подключения через 5 сек...");
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        let mut g = adb_clone.lock().await;
                        match g.connect().await {
                            Ok(reader) => {
                                g.start_receiver(tx_clone.clone(), reader, adb_clone.clone(), tray_clone.clone());
                                let _ = tray_clone.send(TrayStatus::Windows);
                                info!("✓ Подключено!");
                                break;
                            }
                            Err(e) => warn!("Не удалось: {}", e),
                        }
                    }
                });
            }
        }
    }
    // Запуск scrcpy параллельно
    scrcpy_launcher::start_scrcpy(&config.scrcpy);

    start_hooks();
    input_capture::start_input_capture(shared_state.clone(), tx.clone());

    info!("Система готова!");
    run_loop(
        rx, tray_cmd_rx, tray_status_tx, adb, shared_state, tx, settings_state, runtime_handle
    ).await;
}

async fn run_loop(
    rx: flume::Receiver<InputEvent>,
    tray_rx: flume::Receiver<TrayCommand>,
    tray_tx: flume::Sender<TrayStatus>,
    adb: Arc<TokioMutex<AdbBridge>>,
    state: Arc<SharedState>,
    tx: flume::Sender<InputEvent>,
    settings_state: Arc<Mutex<SettingsState>>,
    runtime_handle: tokio::runtime::Handle,
) {
    let mut last_switch = std::time::Instant::now();

    loop {
        tokio::select! {
            Ok(ev) = rx.recv_async() => {
                match &ev {
                    InputEvent::SwitchToMiStick | InputEvent::SwitchToWindows => {
                        if last_switch.elapsed().as_millis() < 500 { continue; }
                        last_switch = std::time::Instant::now();
                    }
                    _ => {}
                }
                handle_input(ev, &adb, &state, &tray_tx).await;
            }
            Ok(cmd) = tray_rx.recv_async() => {
                handle_tray(cmd, &tx, &adb, &tray_tx, &settings_state, &state, &runtime_handle).await;
            }
        }
    }
}

async fn handle_input(
    ev: InputEvent,
    adb: &Arc<TokioMutex<AdbBridge>>,
    state: &Arc<SharedState>,
    tray_tx: &flume::Sender<TrayStatus>,
) {
    match ev {
        InputEvent::SaveCursorPos => {
            let (x, y) = win_api::get_cursor_pos();
            state.save_cursor(x, y);
        }
        InputEvent::SwitchToMiStick => {
            state.set_state(AppState::MiStick);
            set_blocking(true);
            let _ = tray_tx.send(TrayStatus::MiStick);
            info!(">>> Mi Stick");
        }
        InputEvent::SwitchToWindows => {
            state.set_state(AppState::Windows);
            set_blocking(false);
            if let Some((x, y)) = state.get_saved_cursor() {
                win_api::set_cursor_pos(x, y);
            }
            let _ = tray_tx.send(TrayStatus::Windows);
            info!("<<< Windows");
        }
        InputEvent::MouseMove { dx, dy } if state.get_state() == AppState::MiStick => {
            let config = state.get_config();
            let s = config.input.mouse_sensitivity;
            let sdx = (dx as f32 * s) as i32;
            let sdy = (dy as f32 * s) as i32;
            if sdx != 0 || sdy != 0 {
                adb.lock().await.send_event(InputEvent::MouseMove { dx: sdx, dy: sdy }).await;
            }
        }
        InputEvent::MouseClick { button, pressed } if state.get_state() == AppState::MiStick => {
            adb.lock().await.send_event(InputEvent::MouseClick { button, pressed }).await;
        }
        InputEvent::KeyPress { code } if state.get_state() == AppState::MiStick => {
            adb.lock().await.send_event(InputEvent::KeyPress { code }).await;
        }
        InputEvent::KeyRelease { code } if state.get_state() == AppState::MiStick => {
            adb.lock().await.send_event(InputEvent::KeyRelease { code }).await;
        }
        _ => {}
    }
}

async fn handle_tray(
    cmd: TrayCommand,
    tx: &flume::Sender<InputEvent>,
    adb: &Arc<TokioMutex<AdbBridge>>,
    tray_tx: &flume::Sender<TrayStatus>,
    settings_state: &Arc<Mutex<SettingsState>>,
    state: &Arc<SharedState>,
    runtime_handle: &tokio::runtime::Handle,
) {
    match cmd {
        TrayCommand::SwitchToMiStick => {
            let _ = tx.send(InputEvent::SaveCursorPos);
            let _ = tx.send(InputEvent::SwitchToMiStick);
        }
        TrayCommand::Reconnect => {
            info!("Ручное переподключение...");
            let _ = tray_tx.send(TrayStatus::Disconnected);
            let mut guard = adb.lock().await;
            match guard.reconnect().await {
                Ok(reader) => {
                    guard.start_receiver(tx.clone(), reader, adb.clone(), tray_tx.clone());
                    let _ = tray_tx.send(TrayStatus::Windows);
                    info!("✓ Переподключено!");
                }
                Err(e) => error!("Не удалось: {}", e),
            }
        }
        TrayCommand::OpenSettings => {
            info!("Открытие окна настроек...");
            settings_ui::open_settings_window(
                settings_state.clone(),
                state.clone(),
                adb.clone(),
                tray_tx.clone(),
                tx.clone(),
                runtime_handle.clone(),
            );
        }
        TrayCommand::Quit => {
            info!("Выход");
            set_blocking(false);
            scrcpy_launcher::stop_scrcpy();
            std::process::exit(0);
        }
    }
}