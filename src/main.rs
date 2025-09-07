mod dualsense;

use hidapi::HidApi;

use crate::dualsense::DualSense;

fn main() -> anyhow::Result<()> {
    let hid = HidApi::new().unwrap();
    let ds = DualSense::open(Some(hid))?;

    ds.poll_report(1000, |report| {
        let (capacity, charging) = report.battery();
        println!("Battery: {} (charging: {})", capacity, charging);
    })?;

    Ok(())
}
