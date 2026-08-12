use crate::config::AppConfig;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq)]
pub enum AppState {
    Windows,
    MiStick,
}

#[derive(Debug, Clone)]
pub enum InputEvent {
    MouseMove { dx: i32, dy: i32 },
    MouseClick { button: u8, pressed: bool },
    KeyPress { code: i16 },
    KeyRelease { code: i16 },
    SwitchToMiStick,
    SwitchToWindows,
    SaveCursorPos,
}

pub struct SharedState {
    pub current: Mutex<AppState>,
    pub ctrl_pressed: AtomicBool,
    pub shift_pressed: AtomicBool,
    pub saved_cursor_pos: Mutex<Option<(i32, i32)>>,
    pub config: Arc<Mutex<AppConfig>>,
}

impl SharedState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            current: Mutex::new(AppState::Windows),
            ctrl_pressed: AtomicBool::new(false),
            shift_pressed: AtomicBool::new(false),
            saved_cursor_pos: Mutex::new(None),
            config: Arc::new(Mutex::new(config)),
        }
    }

    pub fn set_state(&self, state: AppState) {
        let mut current = self.current.lock().unwrap();
        if *current != state {
            tracing::info!("{:?} -> {:?}", *current, state);
            *current = state;
        }
    }

    pub fn get_state(&self) -> AppState {
        self.current.lock().unwrap().clone()
    }

    pub fn save_cursor(&self, x: i32, y: i32) {
        *self.saved_cursor_pos.lock().unwrap() = Some((x, y));
    }

    pub fn get_saved_cursor(&self) -> Option<(i32, i32)> {
        *self.saved_cursor_pos.lock().unwrap()
    }

    pub fn get_config(&self) -> AppConfig {
        self.config.lock().unwrap().clone()
    }

    pub fn update_config(&self, new_config: AppConfig) {
        *self.config.lock().unwrap() = new_config;
    }
}