use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

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
    RestoreCursorPos,
}

pub struct SharedState {
    pub current: Mutex<AppState>,
    pub ctrl_pressed: AtomicBool,
    pub shift_pressed: AtomicBool,
    pub saved_cursor_pos: Mutex<Option<(i32, i32)>>,
}

impl SharedState {
    pub fn new() -> Self {
        Self {
            current: Mutex::new(AppState::Windows),
            ctrl_pressed: AtomicBool::new(false),
            shift_pressed: AtomicBool::new(false),
            saved_cursor_pos: Mutex::new(None),
        }
    }

    pub fn set_state(&self, state: AppState) {
        let mut current = self.current.lock().unwrap();
        if *current != state {
            tracing::info!("Смена состояния: {:?} -> {:?}", *current, state);
            *current = state;
        }
    }

    pub fn get_state(&self) -> AppState {
        self.current.lock().unwrap().clone()
    }

    pub fn save_cursor(&self, x: i32, y: i32) {
        let mut pos = self.saved_cursor_pos.lock().unwrap();
        *pos = Some((x, y));
        tracing::debug!("Сохранена позиция курсора: ({}, {})", x, y);
    }

    pub fn get_saved_cursor(&self) -> Option<(i32, i32)> {
        let pos = self.saved_cursor_pos.lock().unwrap();
        *pos
    }
}