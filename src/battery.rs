// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 CarlosEvangelista

use std::ffi::{CString, c_char, c_void};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use crate::models::BatteryStats;
use crate::smc::{IO_OBJECT_NULL, K_IO_RETURN_SUCCESS, KernReturn};

type IoObjectT = u32;
type IoServiceT = IoObjectT;
const K_IO_MAIN_PORT_DEFAULT: u32 = 0;

type CFTypeRef = *const c_void;
type CFDictionaryRef = *const c_void;
type CFMutableDictionaryRef = *mut c_void;
type CFStringRef = *const c_void;
type CFArrayRef = *const c_void;
type CFAllocatorRef = *const c_void;
type CFBooleanRef = *const c_void;
type CFNumberRef = *const c_void;
type CFTypeID = usize;
type CFIndex = isize;
type Boolean = u8;
type CFNumberType = i32;

const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const K_CF_NUMBER_INT_TYPE: CFNumberType = 9;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> *mut c_void;
    fn IOServiceGetMatchingService(masterPort: u32, matching: *mut c_void) -> IoServiceT;
    fn IORegistryEntryCreateCFProperties(
        entry: IoServiceT,
        properties: *mut CFMutableDictionaryRef,
        allocator: CFAllocatorRef,
        options: u32,
    ) -> KernReturn;
    fn IOObjectRelease(object: IoObjectT) -> KernReturn;

    fn IOPSCopyPowerSourcesInfo() -> CFTypeRef;
    fn IOPSCopyPowerSourcesList(blob: CFTypeRef) -> CFArrayRef;
    fn IOPSGetPowerSourceDescription(blob: CFTypeRef, ps: CFTypeRef) -> CFDictionaryRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;
    fn CFNumberGetTypeID() -> CFTypeID;
    fn CFBooleanGetTypeID() -> CFTypeID;
    fn CFDictionaryGetValue(theDict: CFDictionaryRef, key: *const c_void) -> CFTypeRef;
    fn CFNumberGetValue(
        number: CFNumberRef,
        theType: CFNumberType,
        valuePtr: *mut c_void,
    ) -> Boolean;
    fn CFBooleanGetValue(boolean: CFBooleanRef) -> Boolean;
    fn CFRelease(cf: CFTypeRef);
    fn CFArrayGetCount(theArray: CFArrayRef) -> CFIndex;
    fn CFArrayGetValueAtIndex(theArray: CFArrayRef, idx: CFIndex) -> CFTypeRef;
    fn CFStringCreateWithCString(
        alloc: CFAllocatorRef,
        cStr: *const c_char,
        encoding: u32,
    ) -> CFStringRef;
}

#[derive(Debug)]
struct SessionState {
    discharged_mwh: f64,
    last_sample: Option<Instant>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            discharged_mwh: 0.0,
            last_sample: None,
        }
    }
}

fn session_state() -> &'static Mutex<SessionState> {
    static SESSION: OnceLock<Mutex<SessionState>> = OnceLock::new();
    SESSION.get_or_init(|| Mutex::new(SessionState::default()))
}

fn with_cf_string_key<T>(value: &str, f: impl FnOnce(CFStringRef) -> T) -> Option<T> {
    let c_value = CString::new(value).ok()?;

    // SAFETY: c_value is a valid, NUL-terminated UTF-8 C string.
    let key = unsafe {
        CFStringCreateWithCString(
            std::ptr::null(),
            c_value.as_ptr(),
            K_CF_STRING_ENCODING_UTF8,
        )
    };
    if key.is_null() {
        return None;
    }

    let result = f(key);

    // SAFETY: key was created with Create rule and must be released once.
    unsafe { CFRelease(key) };

    Some(result)
}

fn cf_dict_get_int(dict: CFDictionaryRef, key: CFStringRef) -> Option<i32> {
    if dict.is_null() || key.is_null() {
        return None;
    }

    // SAFETY: dict/key are valid CoreFoundation objects for this lookup.
    let value = unsafe { CFDictionaryGetValue(dict, key as *const c_void) };
    if value.is_null() {
        return None;
    }

    // SAFETY: value type ID can be queried for runtime type checking.
    let value_type = unsafe { CFGetTypeID(value) };
    // SAFETY: static CoreFoundation type ID query.
    let number_type = unsafe { CFNumberGetTypeID() };
    if value_type != number_type {
        return None;
    }

    let mut out = 0i32;
    // SAFETY: value is a CFNumber and out points to writable i32 storage.
    let ok = unsafe {
        CFNumberGetValue(
            value as CFNumberRef,
            K_CF_NUMBER_INT_TYPE,
            &mut out as *mut i32 as *mut c_void,
        )
    };
    if ok == 0 {
        return None;
    }

    Some(out)
}

fn cf_dict_get_bool(dict: CFDictionaryRef, key: CFStringRef) -> Option<bool> {
    if dict.is_null() || key.is_null() {
        return None;
    }

    // SAFETY: dict/key are valid CoreFoundation objects for this lookup.
    let value = unsafe { CFDictionaryGetValue(dict, key as *const c_void) };
    if value.is_null() {
        return None;
    }

    // SAFETY: value type ID can be queried for runtime type checking.
    let value_type = unsafe { CFGetTypeID(value) };
    // SAFETY: static CoreFoundation type ID query.
    let bool_type = unsafe { CFBooleanGetTypeID() };
    if value_type != bool_type {
        return None;
    }

    // SAFETY: value is known to be CFBoolean.
    Some(unsafe { CFBooleanGetValue(value as CFBooleanRef) != 0 })
}

fn cf_dict_get_int_by_name(dict: CFDictionaryRef, key_name: &str) -> Option<i32> {
    with_cf_string_key(key_name, |key| cf_dict_get_int(dict, key)).flatten()
}

fn cf_dict_get_bool_by_name(dict: CFDictionaryRef, key_name: &str) -> Option<bool> {
    with_cf_string_key(key_name, |key| cf_dict_get_bool(dict, key)).flatten()
}

trait BatteryDataSource {
    fn read_into(&self, stats: &mut BatteryStats);
}

struct RegistryBatterySource;

impl BatteryDataSource for RegistryBatterySource {
    fn read_into(&self, stats: &mut BatteryStats) {
        battery_read_from_registry(stats);
    }
}

struct IopsBatterySource;

impl BatteryDataSource for IopsBatterySource {
    fn read_into(&self, stats: &mut BatteryStats) {
        battery_read_from_iops(stats);
    }
}

fn battery_update_session_discharge_with_state(
    state: &mut SessionState,
    stats: &mut BatteryStats,
    now: Instant,
) {
    if let Some(last) = state.last_sample {
        let elapsed_seconds = now.duration_since(last).as_secs_f64();
        if elapsed_seconds > 0.0 && stats.voltage_mv > 0 && stats.amperage_ma < 0 {
            let power_mw = ((-stats.amperage_ma) as f64 * stats.voltage_mv as f64) / 1000.0;
            state.discharged_mwh += power_mw * (elapsed_seconds / 3600.0);
        }
    }

    state.last_sample = Some(now);
    stats.session_discharged_mwh = state.discharged_mwh;
}

fn battery_read_from_registry(stats: &mut BatteryStats) {
    let service_name = CString::new("AppleSmartBattery").expect("static service name has no NUL");

    // SAFETY: service_name is a valid C string for the duration of this call.
    let battery_service = unsafe {
        IOServiceGetMatchingService(
            K_IO_MAIN_PORT_DEFAULT,
            IOServiceMatching(service_name.as_ptr()),
        )
    };
    if battery_service == IO_OBJECT_NULL {
        return;
    }

    let mut properties: CFMutableDictionaryRef = std::ptr::null_mut();
    // SAFETY: battery_service is a valid io_service_t and properties points to writable storage.
    let result = unsafe {
        IORegistryEntryCreateCFProperties(battery_service, &mut properties, std::ptr::null(), 0)
    };

    // SAFETY: battery_service was retained by IOKit and should be released once.
    unsafe { IOObjectRelease(battery_service) };

    if result != K_IO_RETURN_SUCCESS || properties.is_null() {
        return;
    }

    let dict = properties as CFDictionaryRef;

    if let Some(value) = cf_dict_get_int_by_name(dict, "CurrentCapacity") {
        stats.current_capacity_mah = value;
    }
    if let Some(value) = cf_dict_get_int_by_name(dict, "MaxCapacity") {
        stats.max_capacity_mah = value;
    }
    if let Some(value) = cf_dict_get_int_by_name(dict, "DesignCapacity") {
        stats.design_capacity_mah = value;
    }
    if let Some(value) = cf_dict_get_int_by_name(dict, "Voltage") {
        stats.voltage_mv = value;
    }

    if let Some(value) = cf_dict_get_int_by_name(dict, "Amperage") {
        stats.amperage_ma = value;
    } else if let Some(value) = cf_dict_get_int_by_name(dict, "InstantAmperage") {
        stats.amperage_ma = value;
    }

    if let Some(value) = cf_dict_get_int_by_name(dict, "CycleCount") {
        stats.cycle_count = value;
    }
    if let Some(value) = cf_dict_get_bool_by_name(dict, "IsCharging") {
        stats.is_charging = value;
    }

    if let Some(value) = cf_dict_get_int_by_name(dict, "AccumulatedSystemEnergyConsumed") {
        stats.accumulated_energy_mwh = value;
        stats.has_accumulated_energy = true;
    }

    // SAFETY: properties was created by Copy/Create API and should be released once.
    unsafe { CFRelease(properties as CFTypeRef) };
}

fn battery_read_from_iops(stats: &mut BatteryStats) {
    // SAFETY: IOKit creates an owned info blob.
    let source_info = unsafe { IOPSCopyPowerSourcesInfo() };
    if source_info.is_null() {
        return;
    }

    // SAFETY: source_info is a valid IOPS info object.
    let source_list = unsafe { IOPSCopyPowerSourcesList(source_info) };
    if source_list.is_null() {
        // SAFETY: source_info is owned and must be released.
        unsafe { CFRelease(source_info) };
        return;
    }

    // SAFETY: source_list is a valid CFArrayRef.
    let count = unsafe { CFArrayGetCount(source_list) };
    if count <= 0 {
        // SAFETY: source_list and source_info are owned and must be released.
        unsafe {
            CFRelease(source_list as CFTypeRef);
            CFRelease(source_info);
        }
        return;
    }

    // SAFETY: index 0 exists because count > 0.
    let source = unsafe { CFArrayGetValueAtIndex(source_list, 0) };
    // SAFETY: source_info/source are valid IOPS objects.
    let description = unsafe { IOPSGetPowerSourceDescription(source_info, source) };
    if !description.is_null() {
        let current = cf_dict_get_int_by_name(description, "Current Capacity");
        let max = cf_dict_get_int_by_name(description, "Max Capacity");

        if let (Some(current), Some(max)) = (current, max)
            && max > 0
        {
            if stats.current_capacity_mah < 0 {
                stats.current_capacity_mah = current;
            }
            if stats.max_capacity_mah < 0 {
                stats.max_capacity_mah = max;
            }
        }

        if let Some(charging) = cf_dict_get_bool_by_name(description, "Is Charging") {
            stats.is_charging = charging;
        }
    }

    // SAFETY: source_list and source_info are owned and must be released.
    unsafe {
        CFRelease(source_list as CFTypeRef);
        CFRelease(source_info);
    }
}

pub fn battery_reader_reset_session() {
    let mut state = session_state().lock().expect("session mutex poisoned");
    state.discharged_mwh = 0.0;
    state.last_sample = None;
}

fn battery_apply_derived_fields(stats: &mut BatteryStats) {
    if stats.current_capacity_mah >= 0 && stats.max_capacity_mah > 0 {
        stats.charge_percent =
            100.0 * stats.current_capacity_mah as f64 / stats.max_capacity_mah as f64;
    }

    if stats.voltage_mv > 0 {
        stats.power_w = (stats.amperage_ma as f64 * stats.voltage_mv as f64) / 1_000_000.0;
    }

    stats.available = stats.max_capacity_mah > 0 || stats.voltage_mv > 0;
}

fn battery_read_stats_with_sources_and_state(
    stats: &mut BatteryStats,
    sources: &[&dyn BatteryDataSource],
    now: Instant,
    state: &mut SessionState,
) {
    *stats = BatteryStats::default();
    stats.session_discharged_mwh = state.discharged_mwh;

    for source in sources {
        source.read_into(stats);
    }

    battery_apply_derived_fields(stats);
    battery_update_session_discharge_with_state(state, stats, now);
}

fn battery_read_stats_with_sources_at(
    stats: &mut BatteryStats,
    sources: &[&dyn BatteryDataSource],
    now: Instant,
) {
    let mut state = session_state().lock().expect("session mutex poisoned");
    battery_read_stats_with_sources_and_state(stats, sources, now, &mut state);
}

pub fn battery_read_stats(stats: &mut BatteryStats) {
    let registry = RegistryBatterySource;
    let iops = IopsBatterySource;
    let sources: [&dyn BatteryDataSource; 2] = [&registry, &iops];
    battery_read_stats_with_sources_at(stats, &sources, Instant::now());
}

#[cfg(test)]
mod tests;
