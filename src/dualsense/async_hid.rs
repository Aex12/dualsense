use std::time::Duration;

use async_hid::{
    AsyncHidRead, Device, DeviceReader, DeviceWriter, HidBackend, HidError, HidResult,
};
use async_io::Timer;
use futures_lite::{FutureExt, Stream, StreamExt};
use zerocopy::transmute;

use crate::dualsense::proto::{DS_INPUT_REPORT_SIZE, DUALSENSE_PID};

use super::proto::{
    DS_INPUT_REPORT_BT_SIZE, DS_INPUT_REPORT_USB_SIZE, DualSenseInputReport,
    DualSenseInputReportBT, DualSenseInputReportUSB, SONY_VID,
};

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
    reader: DeviceReader,
    writer: DeviceWriter,
    connection_type: DualSenseConnectionType,
    current_input_report: u8,
}

impl DualSense {
    pub fn is(device: &Device) -> bool {
        device.vendor_id == SONY_VID && device.product_id == DUALSENSE_PID
    }

    pub async fn find_all<'a>(hid: &'a HidBackend) -> HidResult<impl Stream<Item = Device> + 'a> {
        let stream = hid.enumerate().await?.filter(DualSense::is);
        Ok(stream)
    }

    pub async fn open_device(device: Device) -> HidResult<Self> {
        let (mut reader, writer) = device.open().await?;

        let mut buf = [0u8; DS_INPUT_REPORT_BT_SIZE];
        let size = reader
            .read_input_report(&mut buf)
            .or(async {
                Timer::after(Duration::from_secs(2)).await;
                Err(HidError::Disconnected)
            })
            .await?;

        let connection_type = DualSenseConnectionType::from_report_size(size)
            .ok_or_else(|| HidError::message("Unknown report size"))?;

        Ok(Self {
            device,
            reader,
            writer,
            connection_type,
            current_input_report: buf[0],
        })
    }

    pub fn connection_type(&self) -> DualSenseConnectionType {
        self.connection_type
    }

    pub async fn read_input_report<'a>(&mut self) -> Option<DualSenseInputReport> {
        let mut buf = [0u8; DS_INPUT_REPORT_BT_SIZE];
        let read = self.reader.read_input_report(&mut buf).or(async {
            Timer::after(Duration::from_secs(2)).await;
            Err(HidError::Disconnected)
        });

        let Ok(size) = read.await else {
            return None;
        };
        // device disconnected
        if size == 0 {
            return None;
        }
        // connection type changed
        if size != self.connection_type.report_size() {
            self.connection_type = DualSenseConnectionType::from_report_size(size)?;
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
        Some(input_report)
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
