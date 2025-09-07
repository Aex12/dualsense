use image::imageops::FilterType;
use tao::{
    event::Event,
    event_loop::{ControlFlow, EventLoopBuilder},
};
use tray_icon::{
    TrayIconBuilder, TrayIconEvent,
    menu::{AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

use crate::dualsense::DualSense;

enum UserEvent {
    TrayIconEvent(tray_icon::TrayIconEvent),
    MenuEvent(tray_icon::menu::MenuEvent),
    BatteryEvent(u8, bool),
    DisconnectEvent,
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

    // handle battery status updates
    let proxy = event_loop.create_proxy();
    std::thread::spawn(move || {
        loop {
            if let Ok(ds) = DualSense::open_first() {
                if let Err(_) = ds.enable_bluetooth_full_report() {
                    let _ = proxy.send_event(UserEvent::DisconnectEvent);
                    std::thread::sleep(std::time::Duration::from_millis(1000));
                    continue;
                };
                ds.poll_report(1000, |report| {
                    let (capacity, charging) = report.battery();
                    let _ = proxy.send_event(UserEvent::BatteryEvent(capacity, charging != 0));
                })
                .unwrap_or_else(|_e| {
                    let _ = proxy.send_event(UserEvent::DisconnectEvent);
                });
            }
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }
    });

    let tray_menu = Menu::new();

    let device_i = MenuItem::new("Device", false, None);
    let quit_i = MenuItem::new("Quit", true, None);
    let _ = tray_menu.append_items(&[
        &device_i,
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::about(
            None,
            Some(AboutMetadata {
                name: Some("tao".to_string()),
                copyright: Some("Copyright tao".to_string()),
                ..Default::default()
            }),
        ),
        &PredefinedMenuItem::separator(),
        &quit_i,
    ]);

    let mut tray_icon = None;
    let mut was_charging = false;

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
                        .with_tooltip("DualSense - Disconnected")
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

            Event::UserEvent(UserEvent::TrayIconEvent(_event)) => {
                // println!("{event:?}");
            }

            Event::UserEvent(UserEvent::MenuEvent(event)) => {
                // println!("{event:?}");

                if event.id == quit_i.id() {
                    tray_icon.take();
                    *control_flow = ControlFlow::Exit;
                }
            }

            Event::UserEvent(UserEvent::BatteryEvent(capacity, charging)) => {
                let device_str = format!(
                    "Device 1 - {}% {}",
                    capacity * 10,
                    if charging { "(charging)" } else { "" }
                );
                tray_icon.as_mut().map(|tray| {
                    let _ = tray.set_tooltip(Some(&format!("DualSense\n\n{}", device_str)));
                });
                device_i.set_text(&device_str);
                if charging != was_charging {
                    was_charging = charging;
                    let icon = if charging {
                        load_icon(&[0, 255, 0, 255])
                    } else {
                        load_icon(&[0, 0, 0, 255])
                    };
                    tray_icon.as_mut().map(|tray| {
                        tray.set_icon(Some(icon)).unwrap();
                    });
                }
            }

            Event::UserEvent(UserEvent::DisconnectEvent) => {
                tray_icon.as_mut().map(|tray| {
                    let _ = tray.set_tooltip(Some(&format!("DualSense - Disconnected")));
                });
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
