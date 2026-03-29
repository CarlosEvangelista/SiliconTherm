// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 CarlosEvangelista

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
    let ok = smc_decode_temperature(&bytes, smc_fourcc_from_str("sp78"), 2, 5.0, 130.0, &mut out);
    assert!(ok);
    assert_close(42.0, out);
}

#[test]
fn decode_sp96_works() {
    let bytes = [0x2a, 0x00];
    let mut out = 0.0;
    let ok = smc_decode_temperature(&bytes, smc_fourcc_from_str("sp96"), 2, 5.0, 130.0, &mut out);
    assert!(ok);
    assert_close(168.0, out);
}

#[test]
fn decode_flt_prefers_valid_big_endian() {
    let expected = 58.5f32;
    let bytes = expected.to_bits().to_be_bytes();
    let mut out = 0.0;

    let ok = smc_decode_temperature(&bytes, smc_fourcc_from_str("flt "), 4, 5.0, 130.0, &mut out);
    assert!(ok);
    assert_close(expected as f64, out);
}

#[test]
fn decode_flt_falls_back_to_valid_little_endian() {
    let expected = 50.0f32;
    let bytes = expected.to_bits().to_le_bytes();
    let mut out = 0.0;

    let ok = smc_decode_temperature(&bytes, smc_fourcc_from_str("flt "), 4, 5.0, 130.0, &mut out);
    assert!(ok);
    assert_close(expected as f64, out);
}

#[test]
fn decode_temperature_rejects_unknown_type() {
    let bytes = [0u8; 4];
    let mut out = 0.0;
    let ok = smc_decode_temperature(&bytes, smc_fourcc_from_str("abcd"), 4, 5.0, 130.0, &mut out);
    assert!(!ok);
}
