mod dualsense;
mod tray_icon;

fn main() -> anyhow::Result<()> {
    tray_icon::run_tray_icon()?;
    Ok(())
}
