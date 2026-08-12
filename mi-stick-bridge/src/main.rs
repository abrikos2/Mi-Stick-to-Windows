mod adb_bridge;
mod config;
mod input_blocker;
mod input_capture;
mod state;
mod win_api;

use adb_bridge::AdbBridge;
use config::load_config;
use flume::unbounded;
use input_blocker::{set_blocking, start_hooks};
use state::{AppState, InputEvent, SharedState};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    info!("=== Mi Stick Bridge v0.5 ===");

    let config = match load_config() {
        Ok(c) => c,
        Err(e) => {
            error!("Ошибка загрузки конфига: {}", e);
            return;
        }
    };

    info!("Mi Stick расположен: {}", config.display.mi_stick_position);

    let mut adb = AdbBridge::new(config.mi_stick.clone());

    // connect() теперь возвращает ReadHalf для приёмника
    let reader = match adb.connect().await {
        Ok(r) => r,
        Err(e) => {
            error!("Критическая ошибка подключения: {}", e);
            return;
        }
    };

    // Создаём канал для событий
    let (tx, rx) = unbounded::<InputEvent>();

    // Клонируем tx для приёмника
    let tx_for_receiver = tx.clone();

    // Запускаем приёмник сигналов от companion (передаём ReadHalf)
    AdbBridge::start_receiver(tx_for_receiver, reader);

    // Теперь adb содержит WriteHalf внутри, оборачиваем в Arc<Mutex>
    let adb = Arc::new(Mutex::new(adb));
    let shared_state = Arc::new(SharedState::new());

    // Запуск WinAPI hooks для блокировки ввода
    start_hooks();

    // Запуск перехвата ввода
    input_capture::start_input_capture(shared_state.clone(), config.clone(), tx.clone());

    info!("Система готова!");
    info!("Управление:");
    info!("  Ctrl+Shift+Right → переключиться на Mi Stick");
    info!("  Ctrl+Shift+Left  → вернуться в Windows");
    info!("  Движение мыши к правому краю Mi Stick → авто-возврат в Windows");
    if config.display.mi_stick_position == "left" {
        info!("  Движение мыши к ЛЕВОМУ краю → автопереключение на Mi Stick");
    } else {
        info!("  Движение мыши к ПРАВОМУ краю → автопереключение на Mi Stick");
    }

    run_event_loop(rx, adb, shared_state, config).await;
}

async fn run_event_loop(
    rx: flume::Receiver<InputEvent>,
    adb: Arc<Mutex<AdbBridge>>,
    shared_state: Arc<SharedState>,
    config: config::AppConfig,
) {
    while let Ok(event) = rx.recv() {
        match event {
            InputEvent::SaveCursorPos => {
                let (x, y) = win_api::get_cursor_pos();
                shared_state.save_cursor(x, y);
            }
            InputEvent::RestoreCursorPos => {
                if let Some((x, y)) = shared_state.get_saved_cursor() {
                    win_api::set_cursor_pos(x, y);
                    info!("Восстановлена позиция курсора: ({}, {})", x, y);
                }
            }
            InputEvent::SwitchToMiStick => {
                shared_state.set_state(AppState::MiStick);
                set_blocking(true);
                win_api::hide_cursor();
                win_api::lock_cursor_to_center();
                info!(">>> Переключение на Mi Stick");
            }
            InputEvent::SwitchToWindows => {
                shared_state.set_state(AppState::Windows);
                set_blocking(false);
                win_api::show_cursor();
                win_api::unlock_cursor();
                if let Some((x, y)) = shared_state.get_saved_cursor() {
                    win_api::set_cursor_pos(x, y);
                }
                info!("<<< Возврат в Windows");
            }
            InputEvent::MouseMove { dx, dy } => {
                if shared_state.get_state() == AppState::MiStick {
                    let s = config.input.mouse_sensitivity;
                    let mut adb_guard = adb.lock().await;
                    adb_guard
                        .send_event(InputEvent::MouseMove {
                            dx: (dx as f32 * s) as i32,
                            dy: (dy as f32 * s) as i32,
                        })
                        .await;
                }
            }
            InputEvent::MouseClick { button, pressed } => {
                if shared_state.get_state() == AppState::MiStick {
                    let mut adb_guard = adb.lock().await;
                    adb_guard
                        .send_event(InputEvent::MouseClick { button, pressed })
                        .await;
                }
            }
            InputEvent::KeyPress { code } => {
                if shared_state.get_state() == AppState::MiStick {
                    let mut adb_guard = adb.lock().await;
                    adb_guard.send_event(InputEvent::KeyPress { code }).await;
                }
            }
            InputEvent::KeyRelease { code } => {
                if shared_state.get_state() == AppState::MiStick {
                    let mut adb_guard = adb.lock().await;
                    adb_guard.send_event(InputEvent::KeyRelease { code }).await;
                }
            }
        }
    }
}