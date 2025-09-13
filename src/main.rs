#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod device_manager;
mod dualsense;
mod tray_icon;

fn main() -> anyhow::Result<()> {
    tray_icon::run_tray_icon()?;
    Ok(())
}
