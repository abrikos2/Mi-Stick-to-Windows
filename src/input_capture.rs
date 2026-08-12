use crate::config::AppConfig;
use crate::state::{AppState, InputEvent, SharedState};
use crate::win_api;
use flume::Sender;
use rdev::{listen, Event, EventType, Key};
use std::sync::Arc;

pub fn start_input_capture(state: Arc<SharedState>, config: AppConfig, tx: Sender<InputEvent>) {
    std::thread::spawn(move || {
        let (screen_w, _screen_h) = win_api::get_primary_screen_size();
        let center_x = screen_w / 2;
        let center_y = win_api::get_primary_screen_size().1 / 2;
        let mi_stick_pos = config.display.mi_stick_position.clone();

        let callback = move |event: Event| {
            let current_state = state.get_state();

            // Обработка модификаторов
            match event.event_type {
                EventType::KeyPress(Key::ControlLeft) | EventType::KeyPress(Key::ControlRight) => {
                    state.ctrl_pressed.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                EventType::KeyRelease(Key::ControlLeft) | EventType::KeyRelease(Key::ControlRight) => {
                    state.ctrl_pressed.store(false, std::sync::atomic::Ordering::SeqCst);
                }
                EventType::KeyPress(Key::ShiftLeft) | EventType::KeyPress(Key::ShiftRight) => {
                    state.shift_pressed.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                EventType::KeyRelease(Key::ShiftLeft) | EventType::KeyRelease(Key::ShiftRight) => {
                    state.shift_pressed.store(false, std::sync::atomic::Ordering::SeqCst);
                }
                _ => {}
            }

            // Горячие клавиши: Ctrl+Shift+Left (на Mi Stick) / Ctrl+Shift+Right (в Windows)
            if state.ctrl_pressed.load(std::sync::atomic::Ordering::SeqCst)
                && state.shift_pressed.load(std::sync::atomic::Ordering::SeqCst)
            {
                match event.event_type {
                    // Ctrl+Shift+Left → переключение на Mi Stick (Mi Stick слева)
                    EventType::KeyPress(Key::LeftArrow) => {
                        let _ = tx.send(InputEvent::SaveCursorPos);
                        let _ = tx.send(InputEvent::SwitchToMiStick);
                        return;
                    }
                    // Ctrl+Shift+Right → возврат в Windows
                    EventType::KeyPress(Key::RightArrow) => {
                        let _ = tx.send(InputEvent::SwitchToWindows);
                        return;
                    }
                    _ => {}
                }
            }

            match current_state {
                AppState::Windows => {
                    if let EventType::MouseMove { x, y: _ } = event.event_type {
                        let ix = x as i32;

                        // Проверяем края в зависимости от положения Mi Stick
                        if mi_stick_pos == "left" && ix <= 2 {
                            let _ = tx.send(InputEvent::SaveCursorPos);
                            let _ = tx.send(InputEvent::SwitchToMiStick);
                            return;
                        } else if mi_stick_pos == "right" && ix >= screen_w - 2 {
                            let _ = tx.send(InputEvent::SaveCursorPos);
                            let _ = tx.send(InputEvent::SwitchToMiStick);
                            return;
                        }
                    }
                }
                AppState::MiStick => {
                    match event.event_type {
                        EventType::MouseMove { x, y } => {
                            let dx = (x as i32) - center_x;
                            let dy = (y as i32) - center_y;

                            if dx != 0 || dy != 0 {
                                let _ = tx.send(InputEvent::MouseMove { dx, dy });
                                win_api::set_cursor_pos(center_x, center_y);
                            }
                        }
                        EventType::ButtonPress(button) => {
                            let btn = match button {
                                rdev::Button::Left => 1,
                                rdev::Button::Right => 3,
                                _ => return, // Игнорируем среднюю и другие кнопки
                            };
                            let _ = tx.send(InputEvent::MouseClick { button: btn, pressed: true });
                        }
                        EventType::ButtonRelease(button) => {
                            let btn = match button {
                                rdev::Button::Left => 1,
                                rdev::Button::Right => 3,
                                _ => return,
                            };
                            let _ = tx.send(InputEvent::MouseClick { button: btn, pressed: false });
                        }
                        EventType::KeyPress(key) => {
                            // Escape = кнопка "Назад" на Mi Stick
                            let code = key_to_linux_code(key);
                            if code > 0 {
                                let _ = tx.send(InputEvent::KeyPress { code });
                            }
                        }
                        EventType::KeyRelease(key) => {
                            let code = key_to_linux_code(key);
                            if code > 0 {
                                let _ = tx.send(InputEvent::KeyRelease { code });
                            }
                        }
                        _ => {}
                    }
                }
            }
        };

        if let Err(error) = listen(callback) {
            tracing::error!("Ошибка rdev: {:?}", error);
        }
    });
}

// Маппинг клавиш rdev -> Linux keycode
fn key_to_linux_code(key: Key) -> i16 {
    match key {
        Key::Escape => 158,       // KEY_BACK (кнопка "Назад" на Android TV)
        Key::UpArrow => 103,      // KEY_UP
        Key::DownArrow => 108,    // KEY_DOWN
        Key::LeftArrow => 105,    // KEY_LEFT
        Key::RightArrow => 106,   // KEY_RIGHT
        Key::Return => 28,        // KEY_ENTER
        Key::Space => 57,         // KEY_SPACE
        Key::Backspace => 14,     // KEY_BACKSPACE
        Key::Tab => 15,           // KEY_TAB
        Key::KeyA => 30,
        Key::KeyB => 48,
        Key::KeyC => 46,
        Key::KeyD => 32,
        Key::KeyE => 18,
        Key::KeyF => 33,
        Key::KeyG => 34,
        Key::KeyH => 35,
        Key::KeyI => 23,
        Key::KeyJ => 36,
        Key::KeyK => 37,
        Key::KeyL => 38,
        Key::KeyM => 50,
        Key::KeyN => 49,
        Key::KeyO => 24,
        Key::KeyP => 25,
        Key::KeyQ => 16,
        Key::KeyR => 19,
        Key::KeyS => 31,
        Key::KeyT => 20,
        Key::KeyU => 22,
        Key::KeyV => 47,
        Key::KeyW => 17,
        Key::KeyX => 45,
        Key::KeyY => 21,
        Key::KeyZ => 44,
        Key::Num1 => 2,
        Key::Num2 => 3,
        Key::Num3 => 4,
        Key::Num4 => 5,
        Key::Num5 => 6,
        Key::Num6 => 7,
        Key::Num7 => 8,
        Key::Num8 => 9,
        Key::Num9 => 10,
        Key::Num0 => 11,
        _ => 0,
    }
}