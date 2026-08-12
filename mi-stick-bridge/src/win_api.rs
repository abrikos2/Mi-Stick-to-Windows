use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    ClipCursor, GetCursorPos, GetSystemMetrics, SetCursorPos, ShowCursor,
    SM_CXSCREEN, SM_CYSCREEN,
};

pub fn get_primary_screen_size() -> (i32, i32) {
    unsafe {
        let width = GetSystemMetrics(SM_CXSCREEN);
        let height = GetSystemMetrics(SM_CYSCREEN);
        (width, height)
    }
}

pub fn get_cursor_pos() -> (i32, i32) {
    unsafe {
        let mut point = POINT::default();
        let _ = GetCursorPos(&mut point);
        (point.x, point.y)
    }
}

pub fn set_cursor_pos(x: i32, y: i32) {
    unsafe {
        let _ = SetCursorPos(x, y);
    }
}

pub fn hide_cursor() {
    unsafe {
        while ShowCursor(false) >= 0 {}
    }
}

pub fn show_cursor() {
    unsafe {
        while ShowCursor(true) < 0 {}
    }
}

pub fn lock_cursor_to_center() {
    let (w, h) = get_primary_screen_size();
    let cx = w / 2;
    let cy = h / 2;

    let rect = RECT {
        left: cx,
        top: cy,
        right: cx + 1,
        bottom: cy + 1,
    };

    unsafe {
        let _ = SetCursorPos(cx, cy);
        let _ = ClipCursor(Some(&rect));
    }
}

pub fn unlock_cursor() {
    unsafe {
        let _ = ClipCursor(None);
    }
}

pub fn teleport_cursor_to_edge(position: &str) {
    let (w, h) = get_primary_screen_size();
    let y = h / 2;
    // Если Mi Stick слева, то при возврате курсор должен быть у левого края
    // Если Mi Stick справа, то при возврате курсор должен быть у правого края
    let x = if position == "left" { 10 } else { w - 10 };
    set_cursor_pos(x, y);
}

pub fn teleport_cursor_to_opposite_edge(position: &str) {
    let (w, h) = get_primary_screen_size();
    let y = h / 2;
    // При переключении НА Mi Stick: курсор должен появиться на противоположной стороне Mi Stick
    // Если Mi Stick слева, то курсор на Mi Stick должен быть у его ПРАВОГО края
    // (чтобы можно было вернуться, двигая мышь вправо)
    let x = if position == "left" { 10 } else { w - 10 };
    set_cursor_pos(x, y);
}