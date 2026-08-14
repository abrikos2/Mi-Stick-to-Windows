use crate::win_api;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::info;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, KBDLLHOOKSTRUCT, MSG, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

const HC_ACTION: i32 = 0;
const VK_SHIFT: u32 = 0x10;
const VK_CONTROL: u32 = 0x11;
const VK_LEFT: u32 = 0x25;
const VK_RIGHT: u32 = 0x27;

static BLOCKING: AtomicBool = AtomicBool::new(false);
static CTRL_HELD: AtomicBool = AtomicBool::new(false);
static SHIFT_HELD: AtomicBool = AtomicBool::new(false);

pub fn set_blocking(block: bool) {
    BLOCKING.store(block, Ordering::SeqCst);
    if block {
        win_api::hide_cursor();
        win_api::lock_cursor_to_center();
        info!("🔒 Полная блокировка включена");
    } else {
        win_api::show_cursor();
        win_api::unlock_cursor();
        CTRL_HELD.store(false, Ordering::SeqCst);
        SHIFT_HELD.store(false, Ordering::SeqCst);
        info!("🔓 Блокировка выключена");
    }
}

pub fn start_hooks() {
    std::thread::spawn(|| {
        unsafe {
            let kb_hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb_proc), None, 0);
            let mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0);

            if let Err(e) = &kb_hook {
                tracing::error!("Keyboard hook failed: {}", e);
            }
            if let Err(e) = &mouse_hook {
                tracing::error!("Mouse hook failed: {}", e);
            }

            info!("Hooks установлены (полная блокировка)");

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            if let Ok(h) = kb_hook {
                let _ = UnhookWindowsHookEx(h);
            }
            if let Ok(h) = mouse_hook {
                let _ = UnhookWindowsHookEx(h);
            }
        }
    });
}

unsafe extern "system" fn kb_proc(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if n_code == HC_ACTION {
        let kb = unsafe { &*(l_param.0 as *const KBDLLHOOKSTRUCT) };
        let vk = kb.vkCode;
        let down = w_param.0 == WM_KEYDOWN as usize || w_param.0 == WM_SYSKEYDOWN as usize;

        if vk == VK_CONTROL {
            CTRL_HELD.store(down, Ordering::SeqCst);
        }
        if vk == VK_SHIFT {
            SHIFT_HELD.store(down, Ordering::SeqCst);
        }

        if BLOCKING.load(Ordering::SeqCst) {
            if CTRL_HELD.load(Ordering::SeqCst)
                && SHIFT_HELD.load(Ordering::SeqCst)
                && (vk == VK_LEFT || vk == VK_RIGHT)
            {
                return CallNextHookEx(None, n_code, w_param, l_param);
            }
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, n_code, w_param, l_param)
}

unsafe extern "system" fn mouse_proc(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if n_code == HC_ACTION && BLOCKING.load(Ordering::SeqCst) {
        return LRESULT(1);
    }
    CallNextHookEx(None, n_code, w_param, l_param)
}