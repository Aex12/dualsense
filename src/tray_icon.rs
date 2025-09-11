use smol::channel::bounded;

use async_hid::{HidBackend, HidError};
use futures_lite::StreamExt;
use image::imageops::FilterType;
use tao::{
    event::Event,
    event_loop::{ControlFlow, EventLoopBuilder},
};
use tray_icon::{
    MouseButtonState, TrayIconBuilder, TrayIconEvent,
    menu::{AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

use crate::dualsense::async_hid::DualSense;

enum UserEvent {
    TrayIconEvent(tray_icon::TrayIconEvent),
    MenuEvent(tray_icon::menu::MenuEvent),
    DeviceInfo(Vec<String>),
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

    let tray_menu = Menu::new();

    let mut query_device_i: Option<MenuItem> = None;
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

    let mut device_info_items: Vec<MenuItem> = vec![];

    let mut tray_icon = None;

    let _menu_channel = MenuEvent::receiver();
    let _tray_channel = TrayIconEvent::receiver();

    let proxy = event_loop.create_proxy();
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
                TrayIconEvent::Click { button_state, .. } => {
                    if button_state == MouseButtonState::Down {
                        if let Some(_) = query_device_i {
                            return;
                        }
                        query_device_i = Some(MenuItem::new("Querying device info", false, None));
                        let _ = tray_menu.prepend(query_device_i.as_ref().unwrap());
                        let proxy = proxy.clone();
                        smol::spawn(async move {
                            let hid = HidBackend::default();
                            let tasks = DualSense::find_all(&hid)
                                .await?
                                .map(|d| {
                                    smol::spawn(async {
                                        let mut ds = DualSense::open_device(d).await?;
                                        let report = ds.read_input_report().await.ok_or(
                                            HidError::message("Error reading input report"),
                                        )?;
                                        let (capacity, charging) = report.battery();
                                        Ok::<_, HidError>((
                                            ds.connection_type(),
                                            capacity,
                                            charging,
                                        ))
                                    })
                                })
                                .enumerate()
                                .collect::<Vec<_>>()
                                .await;

                            let mut device_info: Vec<String> = Vec::new();
                            for (idx, task) in tasks {
                                let Ok((connection_type, capacity, charging)) = task.await else {
                                    continue;
                                };
                                let label = format!(
                                    "DualSense {} {}: {}% ({})",
                                    idx + 1,
                                    connection_type,
                                    capacity * 10,
                                    charging
                                );
                                device_info.push(label);
                            }
                            let _ = proxy.send_event(UserEvent::DeviceInfo(device_info));

                            Ok::<(), HidError>(())
                        })
                        .detach();
                    }
                }
                _ => {}
            },

            Event::UserEvent(UserEvent::MenuEvent(event)) => {
                if event.id == quit_i.id() {
                    tray_icon.take();
                    *control_flow = ControlFlow::Exit;
                }
            }

            Event::UserEvent(UserEvent::DeviceInfo(devices)) => {
                if let Some(i) = query_device_i.take() {
                    let _ = tray_menu.remove(&i);
                }
                for item in device_info_items.iter() {
                    let _ = tray_menu.remove(item);
                }
                device_info_items.clear();
                if devices.len() == 0 {
                    let item = MenuItem::new("No DualSense device found", false, None);
                    let _ = tray_menu.prepend(&item);
                    device_info_items.push(item);
                }
                for label in devices.into_iter() {
                    let item = MenuItem::new(label, false, None);
                    let _ = tray_menu.prepend(&item);
                    device_info_items.push(item);
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
