pub const SONY_VID: u16 = 0x054C;
pub const DUAL_SENSE_PID: u16 = 0x0CE6;

pub const DS_INPUT_REPORT_USB: u8 = 0x01;
pub const DS_INPUT_REPORT_USB_SIZE: usize = 64;
pub const DS_INPUT_REPORT_BT: u8 = 0x31;
pub const DS_INPUT_REPORT_BT_SIZE: usize = 78;

pub const DS_FEATURE_REPORT_BT_FULL: [u8; 1] = [0x05];

pub const DS_STATUS_BATTERY_CAPACITY: u8 = 0xF;
pub const DS_STATUS_CHARGING: u8 = 0xF0;
pub const DS_STATUS_CHARGING_SHIFT: u8 = 4;
