// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 CarlosEvangelista

use silicontherm_rs::tui::tui_run;

fn parse_interval_or_default(args: &[String], default_value: f64) -> f64 {
    if args.len() < 2 {
        return default_value;
    }

    match args[1].parse::<f64>() {
        Ok(value) if value > 0.0 => value,
        _ => default_value,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let interval_seconds = parse_interval_or_default(&args, 2.0);

    if let Err(error) = tui_run(interval_seconds) {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}
