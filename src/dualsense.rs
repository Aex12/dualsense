use hidapi::{HidApi, HidError, HidResult};
use zerocopy::FromBytes;
use zerocopy::byteorder::{LittleEndian as LE, U16, U32};
use zerocopy_derive::{FromBytes, Immutable, KnownLayout};

const SONY_VID: u16 = 0x054C;
const DUAL_SENSE_PID: u16 = 0x0CE6;

const DS_INPUT_REPORT_USB: u8 = 0x01;
const DS_INPUT_REPORT_USB_SIZE: usize = 64;
const DS_INPUT_REPORT_BT: u8 = 0x31;
const DS_INPUT_REPORT_BT_SIZE: usize = 78;

const DS_STATUS_BATTERY_CAPACITY: u8 = 0xF;
const DS_STATUS_CHARGING: u8 = 0xF0;
const DS_STATUS_CHARGING_SHIFT: u8 = 4;

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
    fn parse_report<'a>(data: &'a [u8], size: usize) -> &'a Self {
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

pub struct DualSense {
    dev: hidapi::HidDevice,
}

impl DualSense {
    #[allow(dead_code)]
    fn new(dev: hidapi::HidDevice) -> Self {
        Self { dev }
    }

    pub fn open(hidapi: Option<HidApi>) -> HidResult<Self> {
        let hidapi = hidapi.map_or_else(HidApi::new, Ok)?;
        let dev = hidapi.open(SONY_VID, DUAL_SENSE_PID)?;
        Ok(Self::new(dev))
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
        let report = DualSenseInputReport::parse_report(&buf, size);
        Ok(f(report))
    }

    pub fn poll_report<F>(&self, pollrate: usize, f: F) -> HidResult<()>
    where
        F: Fn(&DualSenseInputReport) -> (),
    {
        let mut buf = [0u8; DS_INPUT_REPORT_BT_SIZE];
        loop {
            let size = self.dev.read_timeout(&mut buf, 500)?;
            if size == 0 {
                return HidResult::Err(HidError::InvalidZeroSizeData);
            }
            let report = DualSenseInputReport::parse_report(&buf, size);
            f(report);
            if pollrate != 0 {
                std::thread::sleep(std::time::Duration::from_millis(pollrate as u64));
            }
        }
    }
}
