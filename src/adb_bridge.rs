use crate::config::MiStickConfig;
use crate::state::InputEvent;
use flume::Sender;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct InputPacket {
    event_type: u8,
    param1: i16,
    param2: i16,
}

pub struct ReceiverHandle {
    cancel: tokio::sync::watch::Sender<bool>,
}

impl ReceiverHandle {
    pub fn cancel(&self) {
        let _ = self.cancel.send(true);
    }
}

pub struct AdbBridge {
    config: MiStickConfig,
    writer: Option<OwnedWriteHalf>,
    receiver_handle: Option<ReceiverHandle>,
}

impl AdbBridge {
    pub fn new(config: MiStickConfig) -> Self {
        Self { config, writer: None, receiver_handle: None }
    }

    pub async fn connect(&mut self) -> Result<OwnedReadHalf, Box<dyn std::error::Error>> {
        self.adb_connect().await?;

        self.ensure_companion().await?;

        self.setup_forward().await?;

        let local_addr = format!("127.0.0.1:{}", self.config.tunnel_port);
        info!("TCP подключение к {}...", local_addr);

        let stream = match TcpStream::connect(&local_addr).await {
            Ok(s) => { info!("✓ TCP подключено!"); s }
            Err(e) => {
                warn!("TCP не удалось: {}, пробуем ещё раз после перезапуска companion", e);
                self.restart_companion().await?;
                tokio::time::sleep(Duration::from_secs(2)).await;
                TcpStream::connect(&local_addr).await?
            }
        };

        stream.set_nodelay(true)?;
        let (reader, writer) = stream.into_split();
        self.writer = Some(writer);
        info!("✓ Туннель установлен!");
        Ok(reader)
    }

    pub async fn reconnect(&mut self) -> Result<OwnedReadHalf, Box<dyn std::error::Error>> {
        info!("Переподключение...");
        if let Some(handle) = self.receiver_handle.take() {
            handle.cancel();
        }
        self.writer = None;
        tokio::time::sleep(Duration::from_secs(1)).await;
        self.connect().await
    }

    async fn adb_connect(&self) -> Result<(), Box<dyn std::error::Error>> {
        let adb = &self.config.adb_path;
        let addr = format!("{}:{}", self.config.ip, self.config.adb_port);

        match Command::new(adb).arg("version").output().await {
            Ok(o) if o.status.success() => {}
            Ok(o) => return Err(format!("ADB error: {}", String::from_utf8_lossy(&o.stderr)).into()),
            Err(e) => return Err(format!("ADB not found: {}", e).into()),
        }

        info!("ADB connect {}...", addr);
        let out = Command::new(adb).arg("connect").arg(&addr).output().await?;
        let s = String::from_utf8_lossy(&out.stdout);
        if !s.contains("connected") && !s.contains("already") {
            return Err(format!("ADB failed: {}", s).into());
        }
        info!("✓ ADB подключен");
        Ok(())
    }

    async fn is_companion_running(&self) -> bool {
        let adb = &self.config.adb_path;
        let out = Command::new(adb)
            .args(["shell", "pidof companion"])
            .output()
            .await;

        match out {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                !stdout.trim().is_empty()
            }
            _ => false,
        }
    }

    async fn is_companion_installed(&self) -> bool {
        let adb = &self.config.adb_path;
        let out = Command::new(adb)
            .args(["shell", "test -x /data/local/tmp/companion && echo OK"])
            .output()
            .await;

        match out {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout.contains("OK")
            }
            _ => false,
        }
    }

    async fn upload_companion(&self) -> Result<(), Box<dyn std::error::Error>> {
        let adb = &self.config.adb_path;
        let local_path = &self.config.companion_path;

        if !std::path::Path::new(local_path).exists() {
            return Err(format!("Companion не найден: {}", local_path).into());
        }

        info!("Загрузка companion на Mi Stick...");
        let out = Command::new(adb)
            .args(["push", local_path, "/data/local/tmp/companion"])
            .output()
            .await?;

        if !out.status.success() {
            return Err(format!("Push failed: {}", String::from_utf8_lossy(&out.stderr)).into());
        }

        let _ = Command::new(adb)
            .args(["shell", "chmod 755 /data/local/tmp/companion"])
            .output()
            .await?;

        info!("✓ Companion загружен");
        Ok(())
    }

    async fn start_companion(&self) -> Result<(), Box<dyn std::error::Error>> {
        let adb = &self.config.adb_path;

        info!("Запуск companion...");
        let out = Command::new(adb)
            .args(["shell", "nohup /data/local/tmp/companion > /dev/null 2>&1 &"])
            .output()
            .await?;

        if !out.status.success() {
            return Err(format!("Start failed: {}", String::from_utf8_lossy(&out.stderr)).into());
        }

        tokio::time::sleep(Duration::from_secs(2)).await;

        if self.is_companion_running().await {
            info!("✓ Companion запущен");
            Ok(())
        } else {
            Err("Companion не запустился".into())
        }
    }

    async fn stop_companion(&self) {
        let adb = &self.config.adb_path;
        let _ = Command::new(adb)
            .args(["shell", "pkill -f companion"])
            .output()
            .await;
        info!("Companion остановлен");
    }

    async fn restart_companion(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Перезапуск companion...");
        self.stop_companion().await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        self.start_companion().await
    }

    async fn ensure_companion(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_companion_running().await {
            info!("✓ Companion уже запущен");
            return Ok(());
        }

        info!("Companion не запущен, запускаем...");

        if !self.is_companion_installed().await {
            info!("Companion не установлен, загружаем...");
            self.upload_companion().await?;
        }

        self.start_companion().await?;
        Ok(())
    }

    async fn setup_forward(&self) -> Result<(), Box<dyn std::error::Error>> {
        let adb = &self.config.adb_path;
        let port = self.config.tunnel_port;

        info!("ADB forward tcp:{} -> tcp:{}...", port, port);
        let out = Command::new(adb)
            .args(["forward", &format!("tcp:{}", port), &format!("tcp:{}", port)])
            .output()
            .await?;

        if !out.status.success() {
            return Err(format!("Forward failed: {}", String::from_utf8_lossy(&out.stderr)).into());
        }
        info!("✓ Forward настроен");
        Ok(())
    }

    pub fn start_receiver(
        &mut self,
        tx: Sender<InputEvent>,
        reader: OwnedReadHalf,
        adb: Arc<Mutex<AdbBridge>>,
        tray_tx: flume::Sender<crate::tray::TrayStatus>,
    ) {
        if let Some(handle) = self.receiver_handle.take() {
            handle.cancel();
        }

        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);

        tokio::spawn(async move {
            let mut reader = reader;
            let mut buf = [0u8; 5];

            loop {
                tokio::select! {
                    result = reader.read_exact(&mut buf) => {
                        match result {
                            Ok(_) => {
                                let p: InputPacket = unsafe { std::ptr::read(buf.as_ptr() as *const _) };
                                if p.event_type == 5 {
                                    info!("Сигнал возврата от companion");
                                    let _ = tx.send(InputEvent::SwitchToWindows);
                                }
                            }
                            Err(e) => {
                                if *cancel_rx.borrow() { return; }

                                warn!("Соединение потеряно: {}", e);
                                let _ = tray_tx.send(crate::tray::TrayStatus::Disconnected);

                                loop {
                                    if *cancel_rx.borrow() { return; }

                                    info!("Автопереподключение через 3 сек...");
                                    tokio::select! {
                                        _ = tokio::time::sleep(Duration::from_secs(3)) => {}
                                        _ = cancel_rx.changed() => { return; }
                                    }

                                    let mut guard = adb.lock().await;
                                    if *cancel_rx.borrow() { return; }

                                    match guard.reconnect().await {
                                        Ok(new_reader) => {
                                            info!("✓ Автопереподключение успешно!");
                                            let _ = tray_tx.send(crate::tray::TrayStatus::Windows);
                                            reader = new_reader;
                                            break;
                                        }
                                        Err(e) => {
                                            warn!("Не удалось: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ = cancel_rx.changed() => {
                        info!("Receiver отменён");
                        return;
                    }
                }
            }
        });

        self.receiver_handle = Some(ReceiverHandle { cancel: cancel_tx });
    }

    pub async fn send_event(&mut self, event: InputEvent) {
        let packet = match event {
            InputEvent::MouseMove { dx, dy } => InputPacket { event_type: 1, param1: dx as i16, param2: dy as i16 },
            InputEvent::MouseClick { button, pressed } => InputPacket { event_type: 2, param1: button as i16, param2: if pressed { 1 } else { 0 } },
            InputEvent::KeyPress { code } => InputPacket { event_type: 3, param1: code, param2: 0 },
            InputEvent::KeyRelease { code } => InputPacket { event_type: 4, param1: code, param2: 0 },
            _ => return,
        };

        if let Some(w) = &mut self.writer {
            let bytes: [u8; 5] = unsafe { std::mem::transmute(packet) };
            if let Err(e) = w.write_all(&bytes).await {
                error!("Send failed: {}", e);
                self.writer = None;
            }
        }
    }
}