use crate::config::MiStickConfig;
use crate::state::InputEvent;
use flume::Sender;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::process::Command;
use tracing::{error, info, warn};

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct InputPacket {
    event_type: u8,
    param1: i16,
    param2: i16,
}

pub struct AdbBridge {
    config: MiStickConfig,
    writer: Option<OwnedWriteHalf>,
}

impl AdbBridge {
    pub fn new(config: MiStickConfig) -> Self {
        Self {
            config,
            writer: None,
        }
    }

    /// Подключается к companion и возвращает половину stream для чтения
    pub async fn connect(&mut self) -> Result<OwnedReadHalf, Box<dyn std::error::Error>> {
        let local_addr = format!("127.0.0.1:{}", self.config.tunnel_port);

        info!("Попытка прямого TCP подключения к {}...", local_addr);
        let stream = match TcpStream::connect(&local_addr).await {
            Ok(s) => {
                info!("✓ Прямое TCP подключение успешно!");
                s
            }
            Err(_) => {
                info!("Прямое подключение не удалось, пробуем через ADB...");
                self.connect_via_adb(&local_addr).await?
            }
        };

        stream.set_nodelay(true)?;

        // Разделяем stream на чтение и запись
        let (reader, writer) = stream.into_split();
        self.writer = Some(writer);

        info!("✓ TCP-туннель установлен (двунаправленный)!");
        Ok(reader)
    }

    async fn connect_via_adb(
        &self,
        local_addr: &str,
    ) -> Result<TcpStream, Box<dyn std::error::Error>> {
        let adb = &self.config.adb_path;
        let addr = format!("{}:{}", self.config.ip, self.config.adb_port);

        info!("Проверка ADB: {}...", adb);
        match Command::new(adb).arg("version").output().await {
            Ok(output) => {
                if !output.status.success() {
                    return Err(format!("ADB не работает: {}", String::from_utf8_lossy(&output.stderr)).into());
                }
            }
            Err(e) => {
                error!("ADB не найден: {}", e);
                return Err(format!("ADB не найден: {}", e).into());
            }
        }

        info!("ADB connect к {}...", addr);
        let output = Command::new(adb)
            .arg("connect")
            .arg(&addr)
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.contains("connected") && !stdout.contains("already") {
            return Err(format!("ADB connect failed: {}", stdout).into());
        }
        info!("ADB подключен");

        let port = self.config.tunnel_port;
        info!("Настройка ADB forward tcp:{} -> tcp:{}...", port, port);
        let _ = Command::new(adb)
            .arg("forward")
            .arg(format!("tcp:{}", port))
            .arg(format!("tcp:{}", port))
            .output()
            .await?;

        info!("Подключение к Companion по TCP: {}...", local_addr);

        for attempt in 1..=5 {
            match TcpStream::connect(local_addr).await {
                Ok(s) => return Ok(s),
                Err(e) => {
                    warn!("Попытка {}/5: {}", attempt, e);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }

        Err("Не удалось подключиться к Companion после 5 попыток".into())
    }

    pub async fn send_event(&mut self, event: InputEvent) {
        let packet = match event {
            InputEvent::MouseMove { dx, dy } => InputPacket {
                event_type: 1,
                param1: dx as i16,
                param2: dy as i16,
            },
            InputEvent::MouseClick { button, pressed } => InputPacket {
                event_type: 2,
                param1: button as i16,
                param2: if pressed { 1 } else { 0 },
            },
            InputEvent::KeyPress { code } => InputPacket {
                event_type: 3,
                param1: code,
                param2: 0,
            },
            InputEvent::KeyRelease { code } => InputPacket {
                event_type: 4,
                param1: code,
                param2: 0,
            },
            _ => return,
        };

        if let Some(writer) = &mut self.writer {
            let bytes: [u8; 5] = unsafe { std::mem::transmute(packet) };
            if let Err(e) = writer.write_all(&bytes).await {
                error!("Ошибка отправки: {}", e);
                self.writer = None;
            }
        } else {
            error!("Нет writer'а — соединение разорвано");
        }
    }

    /// Запуск приёмника сигналов от companion (использует ReadHalf)
    pub fn start_receiver(tx: Sender<InputEvent>, mut reader: OwnedReadHalf) {
        tokio::spawn(async move {
            let mut buf = [0u8; 5];

            info!("Запуск приёмника сигналов от companion...");

            loop {
                match reader.read_exact(&mut buf).await {
                    Ok(_) => {
                        let packet: InputPacket =
                            unsafe { std::ptr::read(buf.as_ptr() as *const _) };

                        if packet.event_type == 5 {
                            info!("Получен сигнал возврата на Windows от companion");
                            let _ = tx.send(InputEvent::SwitchToWindows);
                        }
                    }
                    Err(e) => {
                        error!("Ошибка чтения от companion: {}", e);
                        break;
                    }
                }
            }
        });
    }
}