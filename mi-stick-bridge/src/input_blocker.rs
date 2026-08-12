use std::sync::atomic::{AtomicBool, Ordering};
use tracing::info;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, KBDLLHOOKSTRUCT, MSG,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WH_MOUSE_LL,
};
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

// Виртуальные коды клавиш
const VK_SHIFT: i32 = 0x10;
const VK_CONTROL: i32 = 0x11;
const VK_LEFT: u32 = 0x25;
const VK_RIGHT: u32 = 0x27;

// HC_ACTION всегда равен 0
const HC_ACTION: i32 = 0;

// Глобальный флаг блокировки
static BLOCKING: AtomicBool = AtomicBool::new(false);

pub fn set_blocking(block: bool) {
    BLOCKING.store(block, Ordering::SeqCst);
    if block {
        info!("Ввод Windows заблокирован");
    } else {
        info!("Ввод Windows разблокирован");
    }
}

pub fn is_blocking() -> bool {
    BLOCKING.load(Ordering::SeqCst)
}

pub fn start_hooks() {
    std::thread::spawn(|| {
        unsafe {
            info!("Установка WinAPI hooks для блокировки ввода...");

            let kb_hook = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_hook_proc),
                None,
                0,
            );

            let mouse_hook = SetWindowsHookExW(
                WH_MOUSE_LL,
                Some(mouse_hook_proc),
                None,
                0,
            );

            if let Err(e) = kb_hook {
                tracing::error!("Не удалось установить keyboard hook: {}", e);
                return;
            }
            if let Err(e) = mouse_hook {
                tracing::error!("Не удалось установить mouse hook: {}", e);
                return;
            }

            info!("Hooks установлены");

            // Message loop для обработки hooks
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            // Снимаем hooks при выходе
            if let Ok(h) = kb_hook {
                let _ = UnhookWindowsHookEx(h);
            }
            if let Ok(h) = mouse_hook {
                let _ = UnhookWindowsHookEx(h);
            }
        }
    });
}

unsafe extern "system" fn keyboard_hook_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code == HC_ACTION && BLOCKING.load(Ordering::SeqCst) {
        // Проверяем, не нажаты ли горячие клавиши Ctrl+Shift+Left/Right
        let ctrl_pressed = (GetAsyncKeyState(VK_CONTROL) as u32 & 0x8000) != 0;
        let shift_pressed = (GetAsyncKeyState(VK_SHIFT) as u32 & 0x8000) != 0;

        if ctrl_pressed && shift_pressed {
            // Получаем виртуальный код клавиши
            let vk_code = {
                let kb_struct = &*(l_param.0 as *const KBDLLHOOKSTRUCT);
                kb_struct.vkCode
            };

            // Пропускаем стрелки влево/вправо (для возврата/переключения)
            if vk_code == VK_LEFT || vk_code == VK_RIGHT {
                return CallNextHookEx(None, n_code, w_param, l_param);
            }
        }

        // Блокируем все остальные клавиши
        return LRESULT(1);
    }

    CallNextHookEx(None, n_code, w_param, l_param)
}

unsafe extern "system" fn mouse_hook_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code == HC_ACTION && BLOCKING.load(Ordering::SeqCst) {
        // Блокируем все события мыши
        return LRESULT(1);
    }

    CallNextHookEx(None, n_code, w_param, l_param)
}