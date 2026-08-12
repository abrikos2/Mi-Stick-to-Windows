use std::fs::{File, OpenOptions};
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
const INPUT_DIR: &str = "/dev/input";

const SWITCH_THRESHOLD_X: i32 = 1920;

fn eviocgbit(ev: u16, len: u32) -> libc::c_int {
    let dir: u32 = 2;
    let typ: u32 = 0x45;
    let nr: u32 = 0x20 + ev as u32;
    let size: u32 = len;
    ((dir << 30) | (size << 16) | (typ << 8) | nr) as libc::c_int
}

fn has_rel_axis(fd: libc::c_int) -> bool {
    let mut bits: [u8; 16] = [0; 16];
    let request = eviocgbit(EV_REL, bits.len() as u32);
    unsafe {
        let ret = libc::ioctl(fd, request, bits.as_mut_ptr());
        if ret < 0 {
            return false;
        }
    }
    let has_x = (bits[0] & 0x01) != 0;
    let has_y = (bits[0] & 0x02) != 0;
    has_x && has_y
}

struct InputDevice {
    file: File,
    name: String,
    path: String,
}

impl InputDevice {
    fn find_mouse() -> Result<Self, String> {
        let entries = std::fs::read_dir(INPUT_DIR)
            .map_err(|e| format!("Не удалось прочитать {}: {}", INPUT_DIR, e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

            if !name.starts_with("event") {
                continue;
            }

            let full_path = format!("{}/{}", INPUT_DIR, name);

            match OpenOptions::new().read(true).write(true).open(&full_path) {
                Ok(file) => {
                    let fd = file.as_raw_fd();

                    if has_rel_axis(fd) {
                        let sys_name_path = format!("/sys/class/input/{}/name", name);
                        let device_name = std::fs::read_to_string(&sys_name_path)
                            .unwrap_or_else(|_| "unknown".to_string())
                            .trim()
                            .to_string();

                        info!("✓ Найдено устройство мыши: {} ({})", name, device_name);

                        return Ok(InputDevice {
                            file,
                            name: device_name,
                            path: full_path,
                        });
                    }
                }
                Err(e) => {
                    debug!("Не удалось открыть {}: {}", full_path, e);
                }
            }
        }

        Err("Устройство мыши не найдено".into())
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
    tracing_subscriber::fmt::init();
    info!("=== Mi Stick Companion v0.8 ===");
    info!("Порт: {}", PORT);
    info!("Архитектура: {}-бит", std::mem::size_of::<usize>() * 8);

    let mouse_device = match InputDevice::find_mouse() {
        Ok(dev) => {
            info!("✓ Используем: {} ({})", dev.name, dev.path);
            Some(dev)
        }
        Err(e) => {
            error!("Не найдено: {}", e);
            None
        }
    };

    if mouse_device.is_none() {
        return Ok(());
    }

    let listener = TcpListener::bind(format!("127.0.0.1:{}", PORT)).await?;
    info!("TCP-сервер запущен. Ожидание Windows...");

    let mouse_device = std::sync::Arc::new(tokio::sync::Mutex::new(mouse_device));

    loop {
        let (socket, addr) = listener.accept().await?;
        info!("Подключен: {}", addr);

        // Разделяем stream на чтение и запись
        let (mut reader, mut writer) = socket.into_split();

        let mouse_clone = mouse_device.clone();

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

                        let mut dev_guard = mouse_clone.lock().await;

                        if let Some(ref mut dev) = *dev_guard {
                            match p_type {
                                1 => {
                                    let dx = p1 as i32;
                                    let dy = p2 as i32;

                                    cursor_x += dx;
                                    debug!("Позиция: x={}, dx={}", cursor_x, dx);

                                    let mut events = vec![];
                                    if dx != 0 {
                                        events.push(LinuxInputEvent::new(EV_REL, REL_X, dx));
                                    }
                                    if dy != 0 {
                                        events.push(LinuxInputEvent::new(EV_REL, REL_Y, dy));
                                    }
                                    events.push(LinuxInputEvent::syn());

                                    if let Err(e) = dev.write_events(&events) {
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
                                        }
                                        cursor_x = 0;
                                    }
                                }
                                2 => {
                                    let code = match p1 {
                                        1 => BTN_LEFT,
                                        3 => KEY_BACK,
                                        _ => return,
                                    };
                                    let value = if p2 > 0 { 1 } else { 0 };
                                    let events = vec![
                                        LinuxInputEvent::new(EV_KEY, code, value),
                                        LinuxInputEvent::syn(),
                                    ];
                                    if let Err(e) = dev.write_events(&events) {
                                        error!("Ошибка клика: {}", e);
                                    }
                                }
                                3 | 4 => {
                                    let value = if p_type == 3 { 1 } else { 0 };
                                    let code = p1 as u16;
                                    let events = vec![
                                        LinuxInputEvent::new(EV_KEY, code, value),
                                        LinuxInputEvent::syn(),
                                    ];
                                    if let Err(e) = dev.write_events(&events) {
                                        error!("Ошибка клавиши: {}", e);
                                    }
                                }
                                _ => {
                                    warn!("Неизвестный тип: {}", p_type);
                                }
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