use crate::config::ScrcpyConfig;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use tracing::{error, info};

// WinAPI флаг: не создавать консольное окно для дочернего процесса
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

static CHILD: Mutex<Option<Child>> = Mutex::new(None);

pub fn start_scrcpy(cfg: &ScrcpyConfig) {
    if !cfg.enabled {
        info!("scrcpy отключен в конфиге");
        return;
    }

    let mut args: Vec<String> = vec!["-s".to_string(), cfg.device.clone()];
    args.extend(cfg.extra_args.clone());

    info!("Запуск scrcpy: {} {:?}", cfg.path, args);

    let mut cmd = Command::new(&cfg.path);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // На Windows — скрыть консоль
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    match cmd.spawn() {
        Ok(child) => {
            info!("scrcpy запущен (PID {})", child.id());
            *CHILD.lock().unwrap() = Some(child);
        }
        Err(e) => {
            error!(
                "Не удалось запустить scrcpy: {}. Проверьте путь '{}'",
                e, cfg.path
            );
        }
    }
}

pub fn stop_scrcpy() {
    if let Some(mut child) = CHILD.lock().unwrap().take() {
        info!("Остановка scrcpy (PID {})...", child.id());
        let _ = child.kill();
        let _ = child.wait();
        info!("scrcpy остановлен");
    }
}