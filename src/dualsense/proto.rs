use zerocopy::FromBytes;
use zerocopy::byteorder::{LittleEndian as LE, U16, U32};
use zerocopy_derive::{FromBytes, Immutable, KnownLayout};

use super::constants::{
    DS_INPUT_REPORT_BT_SIZE, DS_INPUT_REPORT_USB_SIZE, DS_STATUS_BATTERY_CAPACITY,
    DS_STATUS_CHARGING, DS_STATUS_CHARGING_SHIFT,
};

#[derive(FromBytes, KnownLayout, Immutable, Debug)]
#[repr(C)]
pub struct DualSenseTouchPoint {
    contact: u8,
    x_lo: u8,
    xhi_ylo: u8, // x_hi:4 (high nibble), y_lo:4 (low nibble)
    y_hi: u8,
}

impl DualSenseTouchPoint {
    pub fn x(&self) -> u16 {
        let x_hi = (self.xhi_ylo >> 4) as u16;
        let x_lo = self.x_lo as u16;
        (x_hi << 8) | x_lo
    }
    pub fn y(&self) -> u16 {
        let y_lo = (self.xhi_ylo & 0x0F) as u16;
        let y_hi = self.y_hi as u16;
        (y_hi << 4) | y_lo
    }
}

#[derive(FromBytes, KnownLayout, Immutable, Debug)]
#[repr(C)]
pub struct DualSenseInputReport {
    x: u8,
    y: u8,
    rx: u8,
    ry: u8,
    z: u8,
    rz: u8,
    seq_number: u8,
    buttons: [u8; 4],
    reserved: [u8; 4],

    // Motion sensors (little endian words in HID report)
    gyro: [U16<LE>; 3],
    accel: [U16<LE>; 3],
    sensor_timestamp: U32<LE>,
    reserved2: u8,

    points: [DualSenseTouchPoint; 2],

    reserved3: [u8; 12],
    status: u8,
    reserved4: [u8; 10],
}
const DS_INPUT_REPORT_SIZE: usize = core::mem::size_of::<DualSenseInputReport>();

impl DualSenseInputReport {
    pub fn parse_report<'a>(data: &'a [u8], size: usize) -> &'a Self {
        let offset = match size {
            DS_INPUT_REPORT_USB_SIZE => 1,
            DS_INPUT_REPORT_BT_SIZE => 2,
            _ => panic!("Unknown report format"),
        };
        let bytes: &'a [u8] = data
            .get(offset..offset + DS_INPUT_REPORT_SIZE)
            .expect("Invalid report size");

        Self::ref_from_bytes(bytes).unwrap()
    }

    pub fn battery(&self) -> (u8, u8) {
        let s = self.status;
        let capacity = s & DS_STATUS_BATTERY_CAPACITY;
        let charging = (s & DS_STATUS_CHARGING) >> DS_STATUS_CHARGING_SHIFT;
        (capacity, charging)
    }
}
