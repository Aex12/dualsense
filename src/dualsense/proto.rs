use static_assertions::const_assert_eq;
use zerocopy::byteorder::{LittleEndian as LE, U16, U32};
use zerocopy::{FromBytes, Immutable, KnownLayout};

pub const SONY_VID: u16 = 0x054C;
pub const DUALSENSE_PID: u16 = 0x0CE6;

pub const DS_INPUT_REPORT_USB: u8 = 0x01;
pub const DS_INPUT_REPORT_USB_SIZE: usize = 64;
pub const DS_INPUT_REPORT_BT: u8 = 0x31;
pub const DS_INPUT_REPORT_BT_SIZE: usize = 78;

pub const DS_FEATURE_REPORT_BT_FULL: [u8; 1] = [0x05];

pub const DS_STATUS_BATTERY_CAPACITY: u8 = 0xF;
pub const DS_STATUS_CHARGING: u8 = 0xF0;
pub const DS_STATUS_CHARGING_SHIFT: u8 = 4;

#[derive(FromBytes, KnownLayout, Immutable, PartialEq, Eq, Clone, Debug)]
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

#[derive(FromBytes, KnownLayout, Immutable, PartialEq, Eq, Clone, Debug)]
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
pub const DS_INPUT_REPORT_SIZE: usize = core::mem::size_of::<DualSenseInputReport>();

impl DualSenseInputReport {
    pub fn parse<'a>(data: &'a [u8]) -> Option<&'a Self> {
        let offset = match *data.first()? {
            DS_INPUT_REPORT_USB => 1,
            DS_INPUT_REPORT_BT => 2,
            _ => return None,
        };
        let bytes: &'a [u8] = data.get(offset..offset + DS_INPUT_REPORT_SIZE)?;
        Self::ref_from_bytes(bytes).ok()
    }

    pub fn battery(&self) -> (u8, u8) {
        let s = self.status;
        let capacity = s & DS_STATUS_BATTERY_CAPACITY;
        let charging = (s & DS_STATUS_CHARGING) >> DS_STATUS_CHARGING_SHIFT;
        (capacity, charging)
    }
}

#[derive(FromBytes, KnownLayout, Immutable, PartialEq, Eq, Clone, Debug)]
#[repr(C)]
pub struct DualSenseInputReportUSB {
    pub report_id: u8, // 0x01 (USB full report)
    pub input_report: DualSenseInputReport,
    /**
     * This padding will always be zeros, as the USB report is 64 bytes,
     * I'm keeping it the same size as the BT report
     * as it makes the code simpler by reducing branching
     */
    pub padding: [u8; 14],
}
const_assert_eq!(
    core::mem::size_of::<DualSenseInputReportUSB>(),
    DS_INPUT_REPORT_BT_SIZE
);

#[derive(FromBytes, KnownLayout, Immutable, PartialEq, Eq, Clone, Debug)]
#[repr(C)]
pub struct DualSenseInputReportBT {
    pub report_id: u8, // either 0x01 (BT non-full report) or 0x31 (BT full report)
    pub padding: u8,
    pub input_report: DualSenseInputReport,
    pub padding2: [u8; 13],
}
const_assert_eq!(
    core::mem::size_of::<DualSenseInputReportBT>(),
    DS_INPUT_REPORT_BT_SIZE
);
