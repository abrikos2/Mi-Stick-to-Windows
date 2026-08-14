use std::fs::File;
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{debug, error, info, warn};


#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct InputPacket {
    event_type: u8,
    param1: i16,
    param2: i16,
}

#[cfg(target_pointer_width = "32")]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct LinuxInputEvent {
    tv_sec: i32,
    tv_usec: i32,
    type_: u16,
    code: u16,
    value: i32,
}

#[cfg(target_pointer_width = "64")]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct LinuxInputEvent {
    tv_sec: u64,
    tv_usec: u64,
    type_: u16,
    code: u16,
    value: i32,
}

impl LinuxInputEvent {
    fn new(type_: u16, code: u16, value: i32) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        Self {
            #[cfg(target_pointer_width = "32")]
            tv_sec: now.as_secs() as i32,
            #[cfg(target_pointer_width = "32")]
            tv_usec: now.subsec_micros() as i32,
            #[cfg(target_pointer_width = "64")]
            tv_sec: now.as_secs(),
            #[cfg(target_pointer_width = "64")]
            tv_usec: now.subsec_micros() as u64,
            type_,
            code,
            value,
        }
    }

    fn syn() -> Self {
        Self::new(0, 0, 0)
    }
}

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_REL: u16 = 0x02;

const REL_X: u16 = 0x00;
const REL_Y: u16 = 0x01;

const BTN_LEFT: u16 = 0x110;
const KEY_BACK: u16 = 158;

const PORT: u16 = 7878;
const SWITCH_THRESHOLD_X: i32 = 1920;

// ioctl коды для uinput
const UI_SET_EVBIT: u32 = 0x40045564;
const UI_SET_KEYBIT: u32 = 0x40045565;
const UI_SET_RELBIT: u32 = 0x40045566;
const UI_DEV_CREATE: u32 = 0x5501;
const UI_DEV_DESTROY: u32 = 0x5502;

/// Преобразование Windows VK codes → Linux input keycodes
fn vk_to_linux(vk: u16) -> u16 {
    match vk {
        0x71 => 60,   // F2  → KEY_F2  (Назад)
        0x7B => 88,   // F12 → KEY_F12 (Громкость +)
        0x7A => 87,   // F11 → KEY_F11 (Громкость -)
        0x2D => 102,  // Insert → KEY_HOME (Домой)
        0x24 => 102,  // Home → KEY_HOME
        0x25 => 105,  // Left
        0x26 => 103,  // Up
        0x27 => 106,  // Right
        0x28 => 108,  // Down
        0x0D => 28,   // Enter
        0x1B => 1,    // Escape
        0x08 => 14,   // Backspace
        0x20 => 57,   // Space
        0x09 => 15,   // Tab
        0x2E => 111,  // Delete
        0x70 => 59,   // F1
        0x72 => 61,   // F3
        0x73 => 62,   // F4
        0x74 => 63,   // F5
        0x75 => 64,   // F6
        0x76 => 65,   // F7
        0x77 => 66,   // F8
        0x78 => 67,   // F9
        0x79 => 68,   // F10
        0x41 => 30,   // A
        0x42 => 48,   // B
        0x43 => 46,   // C
        0x44 => 32,   // D
        0x45 => 18,   // E
        0x46 => 33,   // F
        0x47 => 34,   // G
        0x48 => 35,   // H
        0x49 => 23,   // I
        0x4A => 36,   // J
        0x4B => 37,   // K
        0x4C => 38,   // L
        0x4D => 50,   // M
        0x4E => 49,   // N
        0x4F => 24,   // O
        0x50 => 25,   // P
        0x51 => 16,   // Q
        0x52 => 19,   // R
        0x53 => 31,   // S
        0x54 => 20,   // T
        0x55 => 22,   // U
        0x56 => 47,   // V
        0x57 => 17,   // W
        0x58 => 45,   // X
        0x59 => 21,   // Y
        0x5A => 44,   // Z
        0x30 => 11,   // 0
        0x31 => 2,    // 1
        0x32 => 3,    // 2
        0x33 => 4,    // 3
        0x34 => 5,    // 4
        0x35 => 6,    // 5
        0x36 => 7,    // 6
        0x37 => 8,    // 7
        0x38 => 9,    // 8
        0x39 => 10,   // 9
        0x10 => 42,   // Shift
        0x11 => 29,   // Ctrl
        0x12 => 56,   // Alt
        0x5B => 125,  // Win
        0xBB => 13,   // =
        0xBD => 12,   // -
        0xDB => 26,   // [
        0xDD => 27,   // ]
        0xDC => 43,   // \
        0xBA => 39,   // ;
        0xDE => 40,   // '
        0xBC => 51,   // ,
        0xBE => 52,   // .
        0xBF => 53,   // /
        _ => vk,
    }
}

struct UInputDevice {
    file: File,
}

impl UInputDevice {
    fn create() -> Result<Self, String> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open("/dev/uinput")
            .map_err(|e| format!("Не удалось открыть /dev/uinput: {}", e))?;
        let fd = file.as_raw_fd();

        unsafe {
            // Разрешаем EV_KEY, EV_REL, EV_SYN — передаём значение напрямую
            if libc::ioctl(fd, UI_SET_EVBIT as libc::c_int, EV_KEY as libc::c_int) < 0 {
                return Err(format!("UI_SET_EVBIT EV_KEY failed: {}", std::io::Error::last_os_error()));
            }
            if libc::ioctl(fd, UI_SET_EVBIT as libc::c_int, EV_REL as libc::c_int) < 0 {
                return Err("UI_SET_EVBIT EV_REL failed".into());
            }
            libc::ioctl(fd, UI_SET_EVBIT as libc::c_int, EV_SYN as libc::c_int);

            // Разрешаем REL_X, REL_Y
            libc::ioctl(fd, UI_SET_RELBIT as libc::c_int, REL_X as libc::c_int);
            libc::ioctl(fd, UI_SET_RELBIT as libc::c_int, REL_Y as libc::c_int);

            // Разрешаем все нужные клавиши
            let keys: Vec<u16> = vec![
                BTN_LEFT, KEY_BACK,
                102, // KEY_HOME
                105, 103, 106, 108, // стрелки
                28, 1, 14, 111, // Enter, Esc, Backspace, Delete
                59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 87, 88, // F1-F12
             ];
            for key in keys {
                libc::ioctl(fd, UI_SET_KEYBIT as libc::c_int, key as libc::c_int);
            }

            // Заполняем uinput_user_dev
            #[repr(C)]
            struct UInputUserDev {
                name: [u8; 80],
                id: [u16; 4],
                ff_effects_max: u32,
                absmax: [i32; 64],
                absmin: [i32; 64],
                absfuzz: [i32; 64],
                absflat: [i32; 64],
            }
            let mut dev: UInputUserDev = std::mem::zeroed();
            let name = b"MiStick Companion Virtual Input\0";
            dev.name[..name.len()].copy_from_slice(name);
            dev.id[0] = 0x03; // BUS_USB
            dev.id[1] = 0x1234;
            dev.id[2] = 0x5678;
            dev.id[3] = 1;

            if libc::write(fd, &dev as *const _ as *const libc::c_void, std::mem::size_of::<UInputUserDev>()) < 0 {
                return Err("write uinput_user_dev failed".into());
            }

            if libc::ioctl(fd, UI_DEV_CREATE as libc::c_int, 0) < 0 {
                return Err(format!("UI_DEV_CREATE failed: {}", std::io::Error::last_os_error()));
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(500));
        info!("✓ Виртуальное устройство создано через uinput");
        Ok(UInputDevice { file })
    }

    fn write_event(&mut self, event: LinuxInputEvent) -> std::io::Result<()> {
        let bytes: [u8; std::mem::size_of::<LinuxInputEvent>()] =
            unsafe { std::mem::transmute(event) };
        self.file.write_all(&bytes)
    }

    fn write_events(&mut self, events: &[LinuxInputEvent]) -> std::io::Result<()> {
        for ev in events {
            self.write_event(*ev)?;
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();
    info!("=== Mi Stick Companion v1.0 (uinput) ===");
    info!("Порт: {}", PORT);
    info!("Архитектура: {}-бит", std::mem::size_of::<usize>() * 8);

    let input_device = match UInputDevice::create() {
        Ok(dev) => dev,
        Err(e) => {
            error!("Не удалось создать uinput: {}", e);
            return Err(e.into());
        }
    };

    let listener = TcpListener::bind(format!("127.0.0.1:{}", PORT)).await?;
    info!("TCP-сервер запущен. Ожидание Windows...");

    let input_device = std::sync::Arc::new(tokio::sync::Mutex::new(input_device));

    loop {
        let (socket, addr) = listener.accept().await?;
        info!("Подключен: {}", addr);

        let (mut reader, mut writer) = socket.into_split();
        let dev_clone = input_device.clone();

        tokio::spawn(async move {
            let mut buf = [0u8; 5];
            let mut cursor_x: i32 = 0;

            loop {
                match reader.read_exact(&mut buf).await {
                    Ok(_) => {
                        let packet: InputPacket =
                            unsafe { std::ptr::read(buf.as_ptr() as *const _) };

                        let p_type = packet.event_type;
                        let p1 = packet.param1;
                        let p2 = packet.param2;

                        debug!("Пакет: type={}, p1={}, p2={}", p_type, p1, p2);

                        let mut dev_guard = dev_clone.lock().await;

                        match p_type {
                            1 => {
                                let dx = p1 as i32;
                                let dy = p2 as i32;
                                cursor_x += dx;

                                let mut events = vec![];
                                if dx != 0 {
                                    events.push(LinuxInputEvent::new(EV_REL, REL_X, dx));
                                }
                                if dy != 0 {
                                    events.push(LinuxInputEvent::new(EV_REL, REL_Y, dy));
                                }
                                events.push(LinuxInputEvent::syn());

                                if let Err(e) = dev_guard.write_events(&events) {
                                    error!("Ошибка мыши: {}", e);
                                }

                                if cursor_x >= SWITCH_THRESHOLD_X {
                                    info!("✓ Достигнут правый край!");
                                    let signal = InputPacket {
                                        event_type: 5,
                                        param1: 0,
                                        param2: 0,
                                    };
                                    let bytes: [u8; 5] =
                                        unsafe { std::mem::transmute(signal) };
                                    if let Err(e) = writer.write_all(&bytes).await {
                                        error!("Ошибка отправки сигнала: {}", e);
                                    } else {
                                        let _ = writer.flush().await;
                                        info!("✓ Сигнал отправлен в Windows");
                                    }
                                    cursor_x = 0;
                                }
                            }
                            2 => {
                                let code = match p1 {
                                    1 => BTN_LEFT,
                                    3 => KEY_BACK,
                                    _ => continue,
                                };
                                let value = if p2 > 0 { 1 } else { 0 };
                                let events = vec![
                                    LinuxInputEvent::new(EV_KEY, code, value),
                                    LinuxInputEvent::syn(),
                                ];
                                if let Err(e) = dev_guard.write_events(&events) {
                                    error!("Ошибка клика: {}", e);
                                }
                            }
                            3 | 4 => {
                                let value = if p_type == 3 { 1 } else { 0 };
                                let code = vk_to_linux(p1 as u16);
                                let events = vec![
                                    LinuxInputEvent::new(EV_KEY, code, value),
                                    LinuxInputEvent::syn(),
                                ];
                                if let Err(e) = dev_guard.write_events(&events) {
                                    error!("Ошибка клавиши: {}", e);
                                }
                            }
                            _ => {
                                warn!("Неизвестный тип: {}", p_type);
                            }
                        }
                    }
                    Err(e) => {
                        info!("Клиент отключился: {}", e);
                        break;
                    }
                }
            }
        });
    }
}