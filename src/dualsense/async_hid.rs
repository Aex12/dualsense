use std::time::Duration;

use async_hid::{
    AsyncHidRead, Device, DeviceId, DeviceReader, DeviceWriter, HidBackend, HidError, HidResult,
};
use async_io::Timer;
use futures_lite::{FutureExt, Stream, StreamExt};
use zerocopy::transmute;

use super::proto::{
    DS_INPUT_REPORT_BT_SIZE, DS_INPUT_REPORT_USB_SIZE, DUALSENSE_PID, DualSenseInputReport,
    DualSenseInputReportBT, DualSenseInputReportUSB, SONY_VID,
};

const OPEN_TIMEOUT: u64 = 500;
const READ_TIMEOUT: u64 = 500;

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

        Ok(Self {
            device,
            connection_type,
        })
    }

    pub async fn connect(&self) -> HidResult<DualSenseConnection> {
        let (reader, writer) = self
            .device
            .open()
            .or(async {
                Timer::after(Duration::from_millis(OPEN_TIMEOUT)).await;
                Err(HidError::NotConnected)
            })
            .await?;

        Ok(DualSenseConnection::new(reader, writer))
    }

    pub fn device_id(&self) -> &DeviceId {
        &self.device.id
    }

    pub fn name(&self) -> String {
        self.device.name.clone()
    }

    pub fn connection_type(&self) -> DualSenseConnectionType {
        self.connection_type
    }
}

pub struct DualSenseConnection {
    reader: DeviceReader,
    writer: DeviceWriter,
}

impl DualSenseConnection {
    fn new(reader: DeviceReader, writer: DeviceWriter) -> Self {
        Self { reader, writer }
    }

    pub async fn read_input_report<'a>(
        &mut self,
    ) -> HidResult<(DualSenseInputReport, DualSenseConnectionType)> {
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
        let connection_type = DualSenseConnectionType::from_report_size(size).ok_or_else(|| {
            HidError::message(format!("Unknown report size: {size}, buf: {buf:?}"))
        })?;
        let input_report: DualSenseInputReport = match connection_type {
            DualSenseConnectionType::USB => {
                let report: DualSenseInputReportUSB = transmute!(buf);
                report.input_report
            }
            DualSenseConnectionType::BT => {
                let report: DualSenseInputReportBT = transmute!(buf);
                report.input_report
            }
        };
        Ok((input_report, connection_type))
    }
}

/*
pub async fn main() -> anyhow::Result<()> {
    let hid = HidBackend::default();

    hid.watch()?
        .for_each(|event| {
            println!("HID event: {:?}", event);
        })
        .await;

    let tasks = DualSense::find_all(&hid)
        .await?
        .map(|d| async move {
            let mut dualsense = DualSense::open_device(d).await?;
            println!("Opened DualSense device: {:?}", dualsense.connection_type);
            while let Some(report) = dualsense.read_input_report().await {
                println!("Report: {:?}", report.battery());
            }

            Ok::<(), HidError>(())
        })
        .collect::<Vec<_>>()
        .await;

    for task in tasks {
        let _ = task.await;
    }

    Ok(())
}
*/
