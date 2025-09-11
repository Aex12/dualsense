use super::proto::{
    DS_FEATURE_REPORT_BT_FULL, DS_INPUT_REPORT_BT_SIZE, DUALSENSE_PID, DualSenseInputReport,
    SONY_VID,
};
use hidapi::{HidApi, HidError, HidResult};

pub struct DualSense {
    dev: hidapi::HidDevice,
}

impl DualSense {
    #[allow(dead_code)]
    fn new(dev: hidapi::HidDevice) -> Self {
        Self { dev }
    }

    pub fn open_first() -> HidResult<Self> {
        let hidapi = HidApi::new()?;
        let dev = hidapi.open(SONY_VID, DUALSENSE_PID)?;
        Ok(Self::new(dev))
    }

    pub fn open_all() -> HidResult<Vec<Self>> {
        let hidapi = HidApi::new()?;
        let devices = hidapi
            .device_list()
            .filter(|d| d.vendor_id() == SONY_VID && d.product_id() == DUALSENSE_PID)
            .filter_map(|d| d.open_device(&hidapi).ok())
            .map(|dev| Self::new(dev))
            .collect();
        Ok(devices)
    }

    pub fn enable_bluetooth_full_report(&self) -> HidResult<()> {
        self.dev.send_feature_report(&DS_FEATURE_REPORT_BT_FULL)
    }

    pub fn read_report<F, R>(&self, f: F) -> HidResult<R>
    where
        F: FnOnce(&DualSenseInputReport) -> R,
    {
        let mut buf = [0u8; DS_INPUT_REPORT_BT_SIZE];
        let size = self.dev.read_timeout(&mut buf, 500)?;
        if size == 0 {
            return HidResult::Err(HidError::InvalidZeroSizeData);
        }
        let report = DualSenseInputReport::parse(&buf).unwrap();
        Ok(f(report))
    }

    pub fn poll_report<F>(&self, pollrate: u64, f: &mut F) -> HidResult<()>
    where
        F: FnMut(&DualSenseInputReport) -> bool,
    {
        let mut buf = [0u8; DS_INPUT_REPORT_BT_SIZE];
        loop {
            let size = self.dev.read_timeout(&mut buf, 500)?;
            if size == 0 {
                return HidResult::Err(HidError::InvalidZeroSizeData);
            }
            let report = DualSenseInputReport::parse(&buf).unwrap();
            let keepgoing = f(report);
            if keepgoing == false {
                break;
            }
            if pollrate != 0 {
                std::thread::sleep(std::time::Duration::from_millis(pollrate));
            }
        }
        Ok(())
    }
}

/*
pub fn main() -> anyhow::Result<()> {
    let devices = DualSense::open_all()?;

    devices[0].poll_report(0, &mut |device| {
        println!("{:?}", device.battery());
        return true;
    })?;

    Ok(())
}
 */
