// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 CarlosEvangelista

use std::ffi::{CString, c_char, c_void};
use std::mem::size_of;

pub const KERNEL_INDEX_SMC: u32 = 2;
pub const SMC_CMD_READ_BYTES: u8 = 5;
pub const SMC_CMD_READ_INDEX: u8 = 8;
pub const SMC_CMD_READ_KEYINFO: u8 = 9;
pub const SMC_MAX_DATA_SIZE: usize = 32;

pub const IO_OBJECT_NULL: IoObjectT = 0;
pub const K_IO_RETURN_SUCCESS: KernReturn = 0;
pub const K_IO_RETURN_NOT_FOUND: KernReturn = 0xe000_02c0u32 as i32;
const K_IO_MAIN_PORT_DEFAULT: MachPortT = 0;

pub type KernReturn = i32;
pub type SmcBytes = [u8; SMC_MAX_DATA_SIZE];

type MachPortT = u32;
type IoObjectT = MachPortT;
type IoServiceT = IoObjectT;
type IoConnectT = IoObjectT;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SmcKeyDataVers {
    pub major: u8,
    pub minor: u8,
    pub build: u8,
    pub reserved: u8,
    pub release: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SmcKeyDataPLimitData {
    pub version: u16,
    pub length: u16,
    pub cpu_p_limit: u32,
    pub gpu_p_limit: u32,
    pub mem_p_limit: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SmcKeyDataKeyInfo {
    pub data_size: u32,
    pub data_type: u32,
    pub data_attributes: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SmcKeyData {
    pub key: u32,
    pub vers: SmcKeyDataVers,
    pub p_limit_data: SmcKeyDataPLimitData,
    pub key_info: SmcKeyDataKeyInfo,
    pub result: u8,
    pub status: u8,
    pub data8: u8,
    pub data32: u32,
    pub bytes: SmcBytes,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SmcContext {
    pub connection: IoConnectT,
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> *mut c_void;
    fn IOServiceGetMatchingService(masterPort: MachPortT, matching: *mut c_void) -> IoServiceT;
    fn IOServiceOpen(
        service: IoServiceT,
        owningTask: MachPortT,
        r#type: u32,
        connect: *mut IoConnectT,
    ) -> KernReturn;
    fn IOServiceClose(connect: IoConnectT) -> KernReturn;
    fn IOObjectRelease(object: IoObjectT) -> KernReturn;
    fn IOConnectCallStructMethod(
        connection: IoConnectT,
        selector: u32,
        inputStruct: *const c_void,
        inputStructCnt: usize,
        outputStruct: *mut c_void,
        outputStructCnt: *mut usize,
    ) -> KernReturn;
}

unsafe extern "C" {
    fn mach_task_self() -> MachPortT;
}

fn smc_call(ctx: &SmcContext, input: &SmcKeyData, output: &mut SmcKeyData) -> KernReturn {
    let input_size = size_of::<SmcKeyData>();
    let mut output_size = size_of::<SmcKeyData>();

    // SAFETY: The pointers and buffer lengths passed to IOKit are valid for the
    // duration of the call and match the exact C layout of SmcKeyData.
    unsafe {
        IOConnectCallStructMethod(
            ctx.connection,
            KERNEL_INDEX_SMC,
            input as *const SmcKeyData as *const c_void,
            input_size,
            output as *mut SmcKeyData as *mut c_void,
            &mut output_size,
        )
    }
}

pub fn smc_fourcc_from_str(value: &str) -> u32 {
    let bytes = value.as_bytes();
    let b0 = *bytes.first().unwrap_or(&0);
    let b1 = *bytes.get(1).unwrap_or(&0);
    let b2 = *bytes.get(2).unwrap_or(&0);
    let b3 = *bytes.get(3).unwrap_or(&0);

    ((b0 as u32) << 24) | ((b1 as u32) << 16) | ((b2 as u32) << 8) | (b3 as u32)
}

pub fn smc_fourcc_to_bytes(value: u32) -> [u8; 5] {
    [
        ((value >> 24) & 0xff) as u8,
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
        0,
    ]
}

pub fn smc_fourcc_to_string(value: u32) -> String {
    let raw = smc_fourcc_to_bytes(value);
    String::from_utf8_lossy(&raw[..4]).to_string()
}

pub fn smc_is_valid_temperature(value: f64, min_valid: f64, max_valid: f64) -> bool {
    value.is_finite() && value > min_valid && value < max_valid
}

pub fn smc_open(ctx: &mut SmcContext) -> KernReturn {
    let service_name = CString::new("AppleSMC").expect("static service name has no NUL bytes");

    // SAFETY: service_name is a valid C string pointer for the duration of the call.
    let service = unsafe {
        IOServiceGetMatchingService(
            K_IO_MAIN_PORT_DEFAULT,
            IOServiceMatching(service_name.as_ptr()),
        )
    };
    if service == IO_OBJECT_NULL {
        return K_IO_RETURN_NOT_FOUND;
    }

    // SAFETY: service is a valid io_service_t until released; ctx.connection points
    // to writable memory.
    let result = unsafe {
        let task = mach_task_self();
        IOServiceOpen(service, task, 0, &mut ctx.connection)
    };

    // SAFETY: service is an IO object acquired from IOKit and must be released once.
    unsafe {
        IOObjectRelease(service);
    }

    result
}

pub fn smc_close(ctx: &mut SmcContext) {
    if ctx.connection == IO_OBJECT_NULL {
        return;
    }

    // SAFETY: connection was obtained from IOServiceOpen and is valid to close once.
    unsafe {
        IOServiceClose(ctx.connection);
    }
    ctx.connection = IO_OBJECT_NULL;
}

pub fn smc_read_key_info(
    ctx: &SmcContext,
    key_str: &str,
    key_info: &mut SmcKeyDataKeyInfo,
) -> bool {
    if key_str.len() < 4 {
        return false;
    }

    let mut input = SmcKeyData::default();
    let mut output = SmcKeyData::default();

    input.key = smc_fourcc_from_str(key_str);
    input.data8 = SMC_CMD_READ_KEYINFO;

    let result = smc_call(ctx, &input, &mut output);
    if result != K_IO_RETURN_SUCCESS || output.result != 0 {
        return false;
    }

    *key_info = output.key_info;
    key_info.data_size > 0 && key_info.data_size <= SMC_MAX_DATA_SIZE as u32
}

pub fn smc_read_key_bytes(
    ctx: &SmcContext,
    key_str: &str,
    key_info: &SmcKeyDataKeyInfo,
    bytes: &mut SmcBytes,
) -> bool {
    if key_str.len() < 4 {
        return false;
    }
    if key_info.data_size == 0 || key_info.data_size > SMC_MAX_DATA_SIZE as u32 {
        return false;
    }

    let mut input = SmcKeyData::default();
    let mut output = SmcKeyData::default();

    input.key = smc_fourcc_from_str(key_str);
    input.data8 = SMC_CMD_READ_BYTES;
    input.key_info.data_size = key_info.data_size;

    let result = smc_call(ctx, &input, &mut output);
    if result != K_IO_RETURN_SUCCESS || output.result != 0 {
        return false;
    }

    let size = key_info.data_size as usize;
    bytes[..size].copy_from_slice(&output.bytes[..size]);
    true
}

pub fn smc_read_key_raw(
    ctx: &SmcContext,
    key_str: &str,
    key_info: &mut SmcKeyDataKeyInfo,
    bytes: &mut SmcBytes,
) -> bool {
    if !smc_read_key_info(ctx, key_str, key_info) {
        return false;
    }
    smc_read_key_bytes(ctx, key_str, key_info, bytes)
}

pub fn smc_read_key_at_index(ctx: &SmcContext, index: u32, key_out: &mut [u8; 5]) -> bool {
    let mut input = SmcKeyData::default();
    let mut output = SmcKeyData::default();

    input.data8 = SMC_CMD_READ_INDEX;
    input.data32 = index;

    let result = smc_call(ctx, &input, &mut output);
    if result != K_IO_RETURN_SUCCESS || output.result != 0 {
        return false;
    }

    *key_out = smc_fourcc_to_bytes(output.key);
    true
}

fn smc_decode_ui32(bytes: &SmcBytes, data_type: u32, data_size: u32, value_out: &mut u32) -> bool {
    if data_type != smc_fourcc_from_str("ui32") || data_size < 4 {
        return false;
    }

    let be = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let le = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);

    if (1..100_000).contains(&be) {
        *value_out = be;
        return true;
    }
    if (1..100_000).contains(&le) {
        *value_out = le;
        return true;
    }

    *value_out = be;
    true
}

pub fn smc_read_key_count(ctx: &SmcContext, count_out: &mut u32) -> bool {
    let mut key_info = SmcKeyDataKeyInfo::default();
    let mut bytes = [0u8; SMC_MAX_DATA_SIZE];
    let mut count = 0u32;

    if smc_read_key_raw(ctx, "#KEY", &mut key_info, &mut bytes)
        && smc_decode_ui32(&bytes, key_info.data_type, key_info.data_size, &mut count)
    {
        *count_out = count;
        return true;
    }

    bytes.fill(0);
    if smc_read_key_raw(ctx, "KEY#", &mut key_info, &mut bytes)
        && smc_decode_ui32(&bytes, key_info.data_type, key_info.data_size, &mut count)
    {
        *count_out = count;
        return true;
    }

    false
}

pub fn smc_decode_temperature(
    bytes: &[u8],
    data_type: u32,
    data_size: u32,
    min_valid: f64,
    max_valid: f64,
    temp_out: &mut f64,
) -> bool {
    if data_type == smc_fourcc_from_str("sp78") {
        if data_size < 2 || bytes.len() < 2 {
            return false;
        }

        let raw = i16::from_be_bytes([bytes[0], bytes[1]]);
        *temp_out = raw as f64 / 256.0;
        return true;
    }

    if data_type == smc_fourcc_from_str("sp96") {
        if data_size < 2 || bytes.len() < 2 {
            return false;
        }

        let raw = i16::from_be_bytes([bytes[0], bytes[1]]);
        *temp_out = raw as f64 / 64.0;
        return true;
    }

    if data_type == smc_fourcc_from_str("flt ") {
        if data_size < 4 || bytes.len() < 4 {
            return false;
        }

        let raw_be = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let raw_le = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);

        let be_value = f32::from_bits(raw_be) as f64;
        let le_value = f32::from_bits(raw_le) as f64;

        if smc_is_valid_temperature(be_value, min_valid, max_valid) {
            *temp_out = be_value;
            return true;
        }
        if smc_is_valid_temperature(le_value, min_valid, max_valid) {
            *temp_out = le_value;
            return true;
        }

        *temp_out = be_value;
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(expected: f64, actual: f64) {
        let delta = (expected - actual).abs();
        assert!(
            delta <= 0.0001,
            "expected {expected}, got {actual} (delta: {delta})"
        );
    }

    #[test]
    fn fourcc_roundtrip_works() {
        let value = smc_fourcc_from_str("sp78");
        assert_eq!(0x7370_3738, value);
        assert_eq!("sp78", smc_fourcc_to_string(value));
    }

    #[test]
    fn valid_temperature_checks_finite_and_range() {
        assert!(smc_is_valid_temperature(42.0, 5.0, 130.0));
        assert!(!smc_is_valid_temperature(5.0, 5.0, 130.0));
        assert!(!smc_is_valid_temperature(130.0, 5.0, 130.0));
        assert!(!smc_is_valid_temperature(f64::NAN, 5.0, 130.0));
    }

    #[test]
    fn decode_sp78_works() {
        let bytes = [0x2a, 0x00];
        let mut out = 0.0;
        let ok =
            smc_decode_temperature(&bytes, smc_fourcc_from_str("sp78"), 2, 5.0, 130.0, &mut out);
        assert!(ok);
        assert_close(42.0, out);
    }

    #[test]
    fn decode_sp96_works() {
        let bytes = [0x2a, 0x00];
        let mut out = 0.0;
        let ok =
            smc_decode_temperature(&bytes, smc_fourcc_from_str("sp96"), 2, 5.0, 130.0, &mut out);
        assert!(ok);
        assert_close(168.0, out);
    }

    #[test]
    fn decode_flt_prefers_valid_big_endian() {
        let expected = 58.5f32;
        let bytes = expected.to_bits().to_be_bytes();
        let mut out = 0.0;

        let ok =
            smc_decode_temperature(&bytes, smc_fourcc_from_str("flt "), 4, 5.0, 130.0, &mut out);
        assert!(ok);
        assert_close(expected as f64, out);
    }

    #[test]
    fn decode_flt_falls_back_to_valid_little_endian() {
        let expected = 50.0f32;
        let bytes = expected.to_bits().to_le_bytes();
        let mut out = 0.0;

        let ok =
            smc_decode_temperature(&bytes, smc_fourcc_from_str("flt "), 4, 5.0, 130.0, &mut out);
        assert!(ok);
        assert_close(expected as f64, out);
    }

    #[test]
    fn decode_temperature_rejects_unknown_type() {
        let bytes = [0u8; 4];
        let mut out = 0.0;
        let ok =
            smc_decode_temperature(&bytes, smc_fourcc_from_str("abcd"), 4, 5.0, 130.0, &mut out);
        assert!(!ok);
    }
}
