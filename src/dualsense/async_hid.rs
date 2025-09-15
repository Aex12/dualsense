use std::time::Duration;

use async_hid::{AsyncHidRead, Device, DeviceId, DeviceReader, HidBackend, HidError, HidResult};
use async_io::Timer;
use futures_lite::{FutureExt, Stream, StreamExt};
use zerocopy::transmute;

use crate::dualsense::proto::DS_FEATURE_REPORT_BT_FULL;

use super::proto::{
    DS_INPUT_REPORT_BT_SIZE, DS_INPUT_REPORT_USB_SIZE, DUALSENSE_PID, DualSenseInputReport,
    DualSenseInputReportBT, DualSenseInputReportUSB, SONY_VID,
};

const OPEN_TIMEOUT: u64 = 500;
const READ_TIMEOUT: u64 = 200;
const WRITE_TIMEOUT: u64 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DualSenseConnectionType {
    USB,
    BT,
}

impl DualSenseConnectionType {
    pub fn from_report_size(size: usize) -> Option<Self> {
        match size {
            DS_INPUT_REPORT_BT_SIZE => Some(Self::BT),
            DS_INPUT_REPORT_USB_SIZE => Some(Self::USB),
            _ => None,
        }
    }

    pub fn report_size(&self) -> usize {
        match self {
            Self::USB => DS_INPUT_REPORT_USB_SIZE,
            Self::BT => DS_INPUT_REPORT_BT_SIZE,
        }
    }
}

impl std::fmt::Display for DualSenseConnectionType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::USB => write!(f, "USB"),
            Self::BT => write!(f, "BT"),
        }
    }
}

pub struct DualSense {
    device: Device,
    connection_type: DualSenseConnectionType,
}

impl DualSense {
    pub fn is(device: &Device) -> bool {
        device.vendor_id == SONY_VID && device.product_id == DUALSENSE_PID
    }

    pub async fn enumerate<'a>(hid: &'a HidBackend) -> HidResult<impl Stream<Item = Device> + 'a> {
        let stream = hid.enumerate().await?.filter(DualSense::is);
        Ok(stream)
    }

    pub async fn open_device_id(hid: &HidBackend, device_id: &DeviceId) -> HidResult<Self> {
        let devices = hid.query_devices(device_id).await?;
        let device = devices
            .into_iter()
            .find(DualSense::is)
            .ok_or(HidError::NotConnected)?;
        Self::open_device(device).await
    }

    pub async fn open_device(device: Device) -> HidResult<Self> {
        let mut reader = device
            .open_readable()
            .or(async {
                Timer::after(Duration::from_millis(OPEN_TIMEOUT)).await;
                Err(HidError::NotConnected)
            })
            .await?;

        let mut buf = [0u8; DS_INPUT_REPORT_BT_SIZE];
        let size = reader
            .read_input_report(&mut buf)
            .or(async {
                Timer::after(Duration::from_millis(READ_TIMEOUT)).await;
                Err(HidError::Disconnected)
            })
            .await?;

        let connection_type = DualSenseConnectionType::from_report_size(size)
            .ok_or_else(|| HidError::message("Unknown report size"))?;

        // Enable full report over Bluetooth
        if connection_type == DualSenseConnectionType::BT {
            let mut buf = [0u8; 41];
            buf[0] = DS_FEATURE_REPORT_BT_FULL;
            let _ = device.read_feature_report(&mut buf).await;
        }

        Ok(Self {
            device,
            connection_type,
        })
    }

    pub async fn connect(&self) -> HidResult<DualSenseConnection> {
        let reader = self
            .device
            .open_readable()
            .or(async {
                Timer::after(Duration::from_millis(OPEN_TIMEOUT)).await;
                Err(HidError::NotConnected)
            })
            .await?;

        Ok(DualSenseConnection::new(reader, self.connection_type))
    }

    pub fn device_id(&self) -> &DeviceId {
        &self.device.id
    }

    pub fn name(&self) -> String {
        format!("DualSense {}", self.connection_type)
    }

    pub fn connection_type(&self) -> DualSenseConnectionType {
        self.connection_type
    }
}

pub struct DualSenseConnection {
    reader: DeviceReader,
    connection_type: DualSenseConnectionType,
}

impl DualSenseConnection {
    fn new(reader: DeviceReader, connection_type: DualSenseConnectionType) -> Self {
        Self {
            reader,
            connection_type,
        }
    }

    pub async fn read_input_report(&mut self) -> HidResult<DualSenseInputReport> {
        let mut buf = [0u8; DS_INPUT_REPORT_BT_SIZE];
        let size = self
            .reader
            .read_input_report(&mut buf)
            .or(async {
                Timer::after(Duration::from_millis(READ_TIMEOUT)).await;
                Err(HidError::Disconnected)
            })
            .await?;

        // device disconnected
        if size == 0 {
            return Err(HidError::Disconnected);
        }

        let input_report: DualSenseInputReport = match self.connection_type {
            DualSenseConnectionType::USB => {
                let report: DualSenseInputReportUSB = transmute!(buf);
                report.input_report
            }
            DualSenseConnectionType::BT => {
                let report: DualSenseInputReportBT = transmute!(buf);
                report.input_report
            }
        };
        Ok(input_report)
    }
}

#[cfg(test)]
mod tests {
    use macro_rules_attribute::apply;
    use smol_macros::{Executor, LocalExecutor, test};

    use super::*;
    use async_hid::HidBackend;

    #[apply(test!)]
    async fn test_open() {
        let hid = HidBackend::default();
        let mut stream = DualSense::enumerate(&hid).await.unwrap();
        if let Some(device) = stream.next().await {
            let ds = DualSense::open_device(device).await.unwrap();
            println!("Opened device: {:?}", ds.device_id());
            let mut connection = ds.connect().await.unwrap();
            for _ in 0..5 {
                let report = connection.read_input_report().await.unwrap();
            }
        } else {
            println!("No DualSense device found");
        }
    }

    #[apply(test!)]
    async fn concurrent_read_input_and_feature(ex: &LocalExecutor<'_>) {
        let hid = HidBackend::default();
        let mut stream = DualSense::enumerate(&hid).await.unwrap();
        if let Some(device) = stream.next().await {
            let ds = DualSense::open_device(device).await.unwrap();
            println!("Opened device: {:?}", ds.device_id());
            let mut connection = ds.connect().await.unwrap();
            let first_input_report_battery =
                connection.read_input_report().await.unwrap().battery();
            let first_feature_report = {
                let mut buf = [0u8; 128];
                buf[0] = DS_FEATURE_REPORT_BT_FULL;
                let size = ds.device.read_feature_report(&mut buf).await.unwrap();
                assert!(size == 41);
                buf[..size].to_vec()
            };

            // reading input report and feature report concurrently should not interfere with each other

            let start = std::time::Instant::now();

            let read_task = ex.spawn(async move {
                let start = std::time::Instant::now();
                for _ in 0..250 {
                    let report = connection.read_input_report().await.unwrap();
                    let battery = report.battery();
                    assert_eq!(battery, first_input_report_battery);
                }
                let elapsed = start.elapsed();
                println!("read_task took: {:?}", elapsed);
            });

            let feature_task = ex.spawn(async move {
                let start = std::time::Instant::now();
                for _ in 0..250 {
                    let mut buf = [0u8; 128];
                    buf[0] = DS_FEATURE_REPORT_BT_FULL;
                    let size = ds.device.read_feature_report(&mut buf).await.unwrap();
                    let report = buf[..size].to_vec();
                    assert_eq!(report, first_feature_report);
                }
                let elapsed = start.elapsed();
                println!("feature_task took: {:?}", elapsed);
            });

            read_task.await;
            feature_task.await;

            let elapsed = start.elapsed();
            println!("Combined tasks took: {:?}", elapsed);
        } else {
            panic!("No DualSense device found");
        }
    }
}
