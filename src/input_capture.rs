use crate::state::{AppState, InputEvent, SharedState};
use crate::win_api;
use flume::Sender;
use rdev::{listen, Event, EventType, Key};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

pub fn start_input_capture(state: Arc<SharedState>, tx: Sender<InputEvent>) {
    std::thread::spawn(move || {
        let (screen_w, screen_h) = win_api::get_primary_screen_size();
        let center_x = screen_w / 2;
        let center_y = screen_h / 2;

        let mut last_teleport = Instant::now();
        let mut last_edge_switch = Instant::now();

        let callback = move |event: Event| {
            let current_state = state.get_state();
            let config = state.get_config(); 
            let mi_stick_pos = config.display.mi_stick_position.clone();

            match event.event_type {
                EventType::KeyPress(Key::ControlLeft) | EventType::KeyPress(Key::ControlRight) => {
                    state.ctrl_pressed.store(true, Ordering::SeqCst);
                }
                EventType::KeyRelease(Key::ControlLeft) | EventType::KeyRelease(Key::ControlRight) => {
                    state.ctrl_pressed.store(false, Ordering::SeqCst);
                }
                EventType::KeyPress(Key::ShiftLeft) | EventType::KeyPress(Key::ShiftRight) => {
                    state.shift_pressed.store(true, Ordering::SeqCst);
                }
                EventType::KeyRelease(Key::ShiftLeft) | EventType::KeyRelease(Key::ShiftRight) => {
                    state.shift_pressed.store(false, Ordering::SeqCst);
                }
                _ => {}
            }

            let ctrl = state.ctrl_pressed.load(Ordering::SeqCst);
            let shift = state.shift_pressed.load(Ordering::SeqCst);

            if ctrl && shift {
                let now = Instant::now();
                match event.event_type {
                    EventType::KeyPress(Key::LeftArrow) => {
                        if now.duration_since(last_edge_switch).as_millis() > 500 {
                            last_edge_switch = now;
                            let _ = tx.send(InputEvent::SaveCursorPos);
                            let _ = tx.send(InputEvent::SwitchToMiStick);
                        }
                        return;
                    }
                    EventType::KeyPress(Key::RightArrow) => {
                        if now.duration_since(last_edge_switch).as_millis() > 500 {
                            last_edge_switch = now;
                            let _ = tx.send(InputEvent::SwitchToWindows);
                        }
                        return;
                    }
                    _ => {}
                }
            }

            if current_state == AppState::Windows {
                if let EventType::MouseMove { x, .. } = event.event_type {
                    let ix = x as i32;
                    let now = Instant::now();
                    if mi_stick_pos == "left" && ix <= 2 {
                        if now.duration_since(last_edge_switch).as_millis() > 500 {
                            last_edge_switch = now;
                            let _ = tx.send(InputEvent::SaveCursorPos);
                            let _ = tx.send(InputEvent::SwitchToMiStick);
                        }
                    } else if mi_stick_pos == "right" && ix >= screen_w - 2 {
                        if now.duration_since(last_edge_switch).as_millis() > 500 {
                            last_edge_switch = now;
                            let _ = tx.send(InputEvent::SaveCursorPos);
                            let _ = tx.send(InputEvent::SwitchToMiStick);
                        }
                    }
                }
                return;
            }

            match event.event_type {
                EventType::MouseMove { x, y } => {
                    if Instant::now().duration_since(last_teleport).as_millis() < 10 {
                        return;
                    }
                    let dx = (x as i32) - center_x;
                    let dy = (y as i32) - center_y;
                    if dx != 0 || dy != 0 {
                        let _ = tx.send(InputEvent::MouseMove { dx, dy });
                        win_api::set_cursor_pos(center_x, center_y);
                        last_teleport = Instant::now();
                    }
                }
                EventType::ButtonPress(btn) => {
                    let b = match btn {
                        rdev::Button::Left => 1,
                        rdev::Button::Right => 3,
                        _ => return,
                    };
                    let _ = tx.send(InputEvent::MouseClick { button: b, pressed: true });
                }
                EventType::ButtonRelease(btn) => {
                    let b = match btn {
                        rdev::Button::Left => 1,
                        rdev::Button::Right => 3,
                        _ => return,
                    };
                    let _ = tx.send(InputEvent::MouseClick { button: b, pressed: false });
                }
                EventType::KeyPress(key) => {
                    let code = key_to_code(key);
                    if code > 0 { let _ = tx.send(InputEvent::KeyPress { code }); }
                }
                EventType::KeyRelease(key) => {
                    let code = key_to_code(key);
                    if code > 0 { let _ = tx.send(InputEvent::KeyRelease { code }); }
                }
                _ => {}
            }
        };

        if let Err(e) = listen(callback) {
            tracing::error!("rdev error: {:?}", e);
        }
    });
}

fn key_to_code(key: Key) -> i16 {
    match key {
        Key::Escape => 158, Key::UpArrow => 103, Key::DownArrow => 108,
        Key::LeftArrow => 105, Key::RightArrow => 106, Key::Return => 28,
        Key::Space => 57, Key::Backspace => 14, Key::Tab => 15,
        Key::KeyA => 30, Key::KeyB => 48, Key::KeyC => 46, Key::KeyD => 32,
        Key::KeyE => 18, Key::KeyF => 33, Key::KeyG => 34, Key::KeyH => 35,
        Key::KeyI => 23, Key::KeyJ => 36, Key::KeyK => 37, Key::KeyL => 38,
        Key::KeyM => 50, Key::KeyN => 49, Key::KeyO => 24, Key::KeyP => 25,
        Key::KeyQ => 16, Key::KeyR => 19, Key::KeyS => 31, Key::KeyT => 20,
        Key::KeyU => 22, Key::KeyV => 47, Key::KeyW => 17, Key::KeyX => 45,
        Key::KeyY => 21, Key::KeyZ => 44,
        Key::Num1 => 2, Key::Num2 => 3, Key::Num3 => 4, Key::Num4 => 5,
        Key::Num5 => 6, Key::Num6 => 7, Key::Num7 => 8, Key::Num8 => 9,
        Key::Num9 => 10, Key::Num0 => 11,
        _ => 0,
    }
}