use std::{collections::HashMap, sync::Arc};

use async_hid::DeviceId;
use image::imageops::FilterType;
use tao::{
    event::Event,
    event_loop::{ControlFlow, EventLoopBuilder},
};
use tray_icon::{
    MouseButtonState, TrayIconBuilder, TrayIconEvent,
    menu::{AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

use crate::device_manager::{DeviceManager, DeviceManagerEvent};

enum UserEvent {
    TrayIconEvent(tray_icon::TrayIconEvent),
    MenuEvent(tray_icon::menu::MenuEvent),
    Device(DeviceManagerEvent),
}

pub fn run_tray_icon() -> anyhow::Result<()> {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();

    // set a tray event handler that forwards the event and wakes up the event loop
    let proxy = event_loop.create_proxy();
    TrayIconEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::TrayIconEvent(event));
    }));

    // set a menu event handler that forwards the event and wakes up the event loop
    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::MenuEvent(event));
    }));

    let mut device_manager = DeviceManager::new();
    let proxy = event_loop.create_proxy();
    device_manager.set_event_handler(move |event| {
        println!("{:?}", event);
        let _ = proxy.send_event(UserEvent::Device(event));
    });
    let device_manager = Arc::new(device_manager);
    let _dm_task = {
        let device_manager = device_manager.clone();
        smol::spawn(async move {
            let _ = device_manager.open_all_devices().await;
            let _ = device_manager.watch_pnp().await;
        })
    };

    let tray_menu = Menu::new();

    let quit_i = MenuItem::new("Quit", true, None);
    let _ = tray_menu.append_items(&[
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::about(
            None,
            Some(AboutMetadata {
                name: Some("Dualsense".to_string()),
                copyright: Some("Dualsense".to_string()),
                ..Default::default()
            }),
        ),
        &PredefinedMenuItem::separator(),
        &quit_i,
    ]);

    let mut device_info: HashMap<DeviceId, (String, (u8, bool))> = HashMap::new();
    let mut device_info_i: Vec<MenuItem> = Vec::new();
    let mut redraw_device_info = false;

    let mut tray_icon = None;

    let _menu_channel = MenuEvent::receiver();
    let _tray_channel = TrayIconEvent::receiver();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(tao::event::StartCause::Init) => {
                let icon = load_icon(&[0, 0, 0, 255]);

                // We create the icon once the event loop is actually running
                // to prevent issues like https://github.com/tauri-apps/tray-icon/issues/90
                tray_icon = Some(
                    TrayIconBuilder::new()
                        .with_menu(Box::new(tray_menu.clone()))
                        .with_tooltip("DualSense")
                        .with_icon(icon)
                        .build()
                        .unwrap(),
                );

                // We have to request a redraw here to have the icon actually show up.
                // Tao only exposes a redraw method on the Window so we use core-foundation directly.
                #[cfg(target_os = "macos")]
                unsafe {
                    use objc2_core_foundation::{CFRunLoopGetMain, CFRunLoopWakeUp};

                    let rl = CFRunLoopGetMain().unwrap();
                    CFRunLoopWakeUp(&rl);
                }
            }

            Event::UserEvent(UserEvent::TrayIconEvent(event)) => match event {
                TrayIconEvent::Click { button_state, .. }
                    if button_state == MouseButtonState::Down =>
                {
                    let device_manager = device_manager.clone();
                    smol::spawn(async move { device_manager.update_status().await }).detach();
                }
                _ => {}
            },

            Event::UserEvent(UserEvent::MenuEvent(event)) => {
                if event.id == quit_i.id() {
                    tray_icon.take();
                    *control_flow = ControlFlow::Exit;
                }
            }

            Event::UserEvent(UserEvent::Device(event)) => match event {
                DeviceManagerEvent::Connected(device_id, name) => {
                    device_info.insert(device_id, (name, (0, false)));
                    redraw_device_info = true;
                }
                DeviceManagerEvent::Disconnected(device_id) => {
                    device_info.remove(&device_id);
                    redraw_device_info = true;
                }
                DeviceManagerEvent::BatteryUpdate(device_id, status_update) => {
                    let Some((_, status)) = device_info.get_mut(&device_id) else {
                        return;
                    };
                    if status != &status_update {
                        *status = status_update;
                        redraw_device_info = true;
                    }
                }
            },

            Event::MainEventsCleared => {
                if redraw_device_info {
                    println!("Redrawing device info");
                    redraw_device_info = false;

                    for i in device_info_i.drain(..) {
                        let _ = tray_menu.remove(&i);
                    }

                    for (i, info) in device_info.values().enumerate() {
                        let label = format!("{}. {}", i + 1, info.0);
                        let status = if &info.1.0 == &0 {
                            "Unknown".to_string()
                        } else if info.1.1 {
                            format!("{}%, charging", info.1.0)
                        } else {
                            format!("{}%", info.1.0)
                        };
                        let item = MenuItem::new(&format!("{label} ({status})"), false, None);
                        let _ = tray_menu.insert(&item, i);
                        device_info_i.push(item);
                    }
                }
            }

            _ => {}
        }
    })
}

fn load_icon(bg: &[u8; 4]) -> tray_icon::Icon {
    const ICON_PNG: &[u8] =
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icon.webp"));
    let (rgba, width, height) = {
        let image = image::load_from_memory(ICON_PNG)
            .expect("Failed to open icon path")
            .resize(32, 32, FilterType::Triangle)
            .into_rgba8();
        let (width, height) = image.dimensions();
        let mut rgba = image.into_raw();
        // set custom background
        for (i, pixel) in rgba.chunks_exact_mut(4).enumerate() {
            let cur_width = (i as u32) % width;
            let cur_height = (i as u32) / width;
            if pixel[3] == 0 && cur_width >= 24 && cur_height <= 12 {
                for i in 0..4 {
                    pixel[i] = bg[i];
                }
            }
        }
        (rgba, width, height)
    };
    tray_icon::Icon::from_rgba(rgba, width, height).expect("Failed to open icon")
}
