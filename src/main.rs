use smol::{Unblock, io, net, prelude::*};

mod dualsense;
mod tray_icon;

fn main() -> anyhow::Result<()> {
    smol::block_on(async {
        let _ = dualsense::async_hid::main().await;
    });
    dualsense::hid::main();
    // tray_icon::run_tray_icon()?;
    Ok(())
}
