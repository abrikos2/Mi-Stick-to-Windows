use flume::{Receiver, Sender};
use image::{ImageBuffer, Rgba};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::windows::EventLoopBuilderExtWindows;
use tray_icon::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    TrayIconBuilder, TrayIconEvent,
};

#[derive(Debug, Clone)]
pub enum TrayCommand {
    SwitchToMiStick,
    Reconnect,
    OpenSettings,
    Quit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TrayStatus {
    Windows,
    MiStick,
    Disconnected,
}

pub fn start_tray() -> (Receiver<TrayCommand>, Sender<TrayStatus>) {
    let (cmd_tx, cmd_rx) = flume::unbounded::<TrayCommand>();
    let (status_tx, status_rx) = flume::unbounded::<TrayStatus>();

    std::thread::spawn(move || {
        let event_loop = EventLoopBuilder::new().with_any_thread(true).build();
        let menu = create_menu();
        let icon_win = make_icon([0, 200, 0]);
        let icon_mi = make_icon([0, 100, 255]);
        let icon_disc = make_icon([255, 50, 50]);

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Mi Stick Bridge")
            .with_icon(icon_win.clone())
            .build()
            .expect("Failed to create tray icon");

        let mut status = TrayStatus::Windows;
        let menu_rx = tray_icon::menu::MenuEvent::receiver();
        let tray_rx = TrayIconEvent::receiver();

        event_loop.run(move |_, _, cf| {
            *cf = ControlFlow::Wait;

            while let Ok(s) = status_rx.try_recv() {
                if s != status {
                    status = s.clone();
                    let ic = match &status {
                        TrayStatus::Windows => &icon_win,
                        TrayStatus::MiStick => &icon_mi,
                        TrayStatus::Disconnected => &icon_disc,
                    };
                    let _ = tray.set_icon(Some(ic.clone()));
                    let tt = match &status {
                        TrayStatus::Windows => "Mi Stick Bridge — Windows",
                        TrayStatus::MiStick => "Mi Stick Bridge — Mi Stick",
                        TrayStatus::Disconnected => "Mi Stick Bridge — Отключено",
                    };
                    let _ = tray.set_tooltip(Some(tt));
                }
            }

            while let Ok(ev) = menu_rx.try_recv() {
                let cmd = match ev.id().0.as_ref() {
                    "sw_mi" => TrayCommand::SwitchToMiStick,
                    "reconn" => TrayCommand::Reconnect,
                    "settings" => TrayCommand::OpenSettings,
                    "quit" => TrayCommand::Quit,
                    _ => continue,
                };
                let _ = cmd_tx.send(cmd);
            }

            while let Ok(ev) = tray_rx.try_recv() {
                if let TrayIconEvent::DoubleClick { .. } = ev {
                    let cmd = match status {
                        TrayStatus::Windows => TrayCommand::SwitchToMiStick,
                        TrayStatus::MiStick => TrayCommand::SwitchToMiStick,
                        TrayStatus::Disconnected => TrayCommand::Reconnect,
                    };
                    let _ = cmd_tx.send(cmd);
                }
            }
        });
    });

    (cmd_rx, status_tx)
}

fn create_menu() -> Menu {
    let m = Menu::new();
    m.append(&MenuItem::with_id("sw_mi", "📺 На Mi Stick", true, None)).ok();
    m.append(&PredefinedMenuItem::separator()).ok();
    m.append(&MenuItem::with_id("reconn", "🔄 Переподключить", true, None)).ok();
    m.append(&PredefinedMenuItem::separator()).ok();
    m.append(&MenuItem::with_id("settings", "⚙️ Настройки...", true, None)).ok();
    m.append(&PredefinedMenuItem::separator()).ok();
    m.append(&MenuItem::with_id("quit", "❌ Выход", true, None)).ok();
    m
}

fn make_icon(color: [u8; 3]) -> tray_icon::Icon {
    let size = 32u32;
    let mut buf: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(size, size);
    for p in buf.pixels_mut() { *p = Rgba([color[0], color[1], color[2], 255]); }
    tray_icon::Icon::from_rgba(buf.into_raw(), size, size).unwrap()
}