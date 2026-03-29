// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 CarlosEvangelista

use std::cmp;
use std::io;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Wrap};
use ratatui::{Frame, Terminal};
use time::OffsetDateTime;
use time::UtcOffset;
use time::format_description::FormatItem;
use time::macros::format_description;

use crate::battery::{battery_read_stats, battery_reader_reset_session};
use crate::models::{
    BatteryStats, SensorCollection, SensorEntry, SensorSection, UiHeatLevel, UiSortMode,
    WATCHTEMP_SENSOR_MAX_RETRIES, WATCHTEMP_SENSOR_MAX_VALID_C, WATCHTEMP_SENSOR_MIN_VALID_C,
    WATCHTEMP_SENSOR_RETRY_DELAY_US,
};
use crate::sensors::{
    sensor_count_active_by_section, sensor_discover_profile, sensor_enrich_from_keyspace,
    sensor_refresh_active,
};
use crate::smc::{K_IO_RETURN_SUCCESS, SmcContext, smc_close, smc_open};
use crate::ui_logic::{
    ui_build_type_tiles, ui_build_visible_indices_by_section, ui_find_next_match,
    ui_meter_filled_columns, ui_meter_heat_level_for_column, ui_meter_loading_column_highlighted,
    ui_sort_mode_label, ui_sort_visible_indices, ui_temp_to_heat_level, ui_temp_to_ratio,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoadStage {
    Profile,
    Keyspace,
    Live,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewPage {
    Cpu,
    Gpu,
    Battery,
}

impl ViewPage {
    fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Gpu => "GPU",
            Self::Battery => "Battery",
        }
    }

    fn cycle(self, direction: i32) -> Self {
        let current = match self {
            Self::Cpu => 0,
            Self::Gpu => 1,
            Self::Battery => 2,
        };
        match (current + direction).rem_euclid(3) {
            0 => Self::Cpu,
            1 => Self::Gpu,
            _ => Self::Battery,
        }
    }

    fn section(self) -> SensorSection {
        match self {
            Self::Cpu => SensorSection::Cpu,
            Self::Gpu => SensorSection::Gpu,
            Self::Battery => SensorSection::Battery,
        }
    }
}

#[derive(Debug)]
struct UiState {
    running: bool,
    search_mode: bool,
    filter_enabled: bool,
    query: String,
    selected_index: usize,
    top_index: usize,
    sort_mode: UiSortMode,
    current_page: ViewPage,
    status: String,
    frame: u32,
    tab_targets: Vec<TabTarget>,
    header_targets: Vec<HeaderTarget>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            running: true,
            search_mode: false,
            filter_enabled: false,
            query: String::new(),
            selected_index: 0,
            top_index: 0,
            sort_mode: UiSortMode::SectionKey,
            current_page: ViewPage::Cpu,
            status: "starting".to_string(),
            frame: 0,
            tab_targets: Vec::new(),
            header_targets: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TabTarget {
    y: u16,
    start_x: u16,
    end_x: u16,
    page: ViewPage,
}

#[derive(Clone, Copy, Debug)]
struct HeaderTarget {
    y: u16,
    start_x: u16,
    end_x: u16,
    sort_mode: UiSortMode,
}

#[derive(Clone, Copy, Debug)]
struct SensorsTableWidths {
    marker: u16,
    key: u16,
    section: u16,
    sensor_type: u16,
    temp: u16,
    meter: u16,
    name: u16,
}

#[derive(Clone, Debug)]
struct RuntimeSnapshot {
    sensors: SensorCollection,
    battery: BatteryStats,
    failed: bool,
    stage: LoadStage,
    stage_message: String,
    interval_seconds: f64,
}

#[derive(Debug)]
struct RuntimeState {
    stop_requested: bool,
    paused: bool,
    failed: bool,
    stage: LoadStage,
    stage_message: String,
    interval_seconds: f64,
    sensors: SensorCollection,
    battery: BatteryStats,
}

type RuntimeHandle = Arc<Mutex<RuntimeState>>;

fn runtime_snapshot(runtime: &RuntimeHandle) -> RuntimeSnapshot {
    let guard = runtime.lock().expect("runtime mutex poisoned");
    RuntimeSnapshot {
        sensors: guard.sensors.clone(),
        battery: guard.battery.clone(),
        failed: guard.failed,
        stage: guard.stage,
        stage_message: guard.stage_message.clone(),
        interval_seconds: guard.interval_seconds,
    }
}

fn runtime_set_stage(runtime: &RuntimeHandle, stage: LoadStage, message: &str, failed: bool) {
    let mut guard = runtime.lock().expect("runtime mutex poisoned");
    guard.stage = stage;
    guard.failed = failed;
    guard.stage_message = message.to_string();
}

fn runtime_should_stop(runtime: &RuntimeHandle) -> bool {
    runtime
        .lock()
        .expect("runtime mutex poisoned")
        .stop_requested
}

fn runtime_request_stop(runtime: &RuntimeHandle) {
    let mut guard = runtime.lock().expect("runtime mutex poisoned");
    guard.stop_requested = true;
}

fn runtime_sleep(runtime: &RuntimeHandle, seconds: f64) {
    let clamped = seconds.max(0.05);
    let chunks = cmp::max(1, (clamped * 20.0) as usize);

    for _ in 0..chunks {
        if runtime_should_stop(runtime) {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn runtime_set_paused(runtime: &RuntimeHandle, paused: bool) {
    let mut guard = runtime.lock().expect("runtime mutex poisoned");
    guard.paused = paused;
}

fn runtime_set_interval(runtime: &RuntimeHandle, interval_seconds: f64) {
    let mut guard = runtime.lock().expect("runtime mutex poisoned");
    guard.interval_seconds = interval_seconds;
}

fn runtime_thread_main(runtime: RuntimeHandle) {
    battery_reader_reset_session();
    let mut initial_battery = BatteryStats::default();
    battery_read_stats(&mut initial_battery);
    {
        let mut guard = runtime.lock().expect("runtime mutex poisoned");
        guard.battery = initial_battery;
    }

    let mut smc = SmcContext::default();
    let open_result = smc_open(&mut smc);
    if open_result != K_IO_RETURN_SUCCESS {
        runtime_set_stage(
            &runtime,
            LoadStage::Error,
            &format!("could not open AppleSMC (0x{open_result:08x})"),
            true,
        );
        return;
    }

    runtime_set_stage(
        &runtime,
        LoadStage::Profile,
        "Loading platform sensor profile",
        false,
    );

    let mut profile_collection = SensorCollection::default();
    let profile_ok = sensor_discover_profile(
        &smc,
        &mut profile_collection,
        5,
        12_000,
        WATCHTEMP_SENSOR_MIN_VALID_C,
        WATCHTEMP_SENSOR_MAX_VALID_C,
    );

    let mut profile_battery = BatteryStats::default();
    battery_read_stats(&mut profile_battery);

    if !profile_ok {
        runtime_set_stage(&runtime, LoadStage::Error, "Profile discovery failed", true);
        smc_close(&mut smc);
        return;
    }

    {
        let mut guard = runtime.lock().expect("runtime mutex poisoned");
        guard.sensors = profile_collection;
        guard.battery = profile_battery;
    }

    runtime_set_stage(
        &runtime,
        LoadStage::Keyspace,
        "Scanning SMC keyspace for extra sensors",
        false,
    );

    let mut keyspace_collection = runtime
        .lock()
        .expect("runtime mutex poisoned")
        .sensors
        .clone();

    let keyspace_ok = sensor_enrich_from_keyspace(
        &smc,
        &mut keyspace_collection,
        2,
        8_000,
        WATCHTEMP_SENSOR_MIN_VALID_C,
        WATCHTEMP_SENSOR_MAX_VALID_C,
    );

    let mut keyspace_battery = BatteryStats::default();
    battery_read_stats(&mut keyspace_battery);

    if keyspace_ok {
        {
            let mut guard = runtime.lock().expect("runtime mutex poisoned");
            guard.sensors = keyspace_collection;
            guard.battery = keyspace_battery;
        }
        runtime_set_stage(&runtime, LoadStage::Live, "Live refresh", false);
    } else {
        {
            let mut guard = runtime.lock().expect("runtime mutex poisoned");
            guard.battery = keyspace_battery;
        }
        runtime_set_stage(
            &runtime,
            LoadStage::Error,
            "Keyspace scan failed (using profile sensors)",
            true,
        );
    }

    while !runtime_should_stop(&runtime) {
        let (paused, interval_seconds, mut sensors_copy) = {
            let guard = runtime.lock().expect("runtime mutex poisoned");
            (guard.paused, guard.interval_seconds, guard.sensors.clone())
        };

        if !paused {
            sensor_refresh_active(
                &smc,
                &mut sensors_copy,
                WATCHTEMP_SENSOR_MAX_RETRIES,
                WATCHTEMP_SENSOR_RETRY_DELAY_US,
                WATCHTEMP_SENSOR_MIN_VALID_C,
                WATCHTEMP_SENSOR_MAX_VALID_C,
            );

            let mut battery = BatteryStats::default();
            battery_read_stats(&mut battery);

            let mut guard = runtime.lock().expect("runtime mutex poisoned");
            guard.sensors = sensors_copy;
            guard.battery = battery;
        }

        runtime_sleep(&runtime, interval_seconds);
    }

    smc_close(&mut smc);
}

fn ensure_selection_visible(state: &mut UiState, visible_count: usize, page_rows: usize) {
    if visible_count == 0 {
        state.selected_index = 0;
        state.top_index = 0;
        return;
    }

    if state.selected_index >= visible_count {
        state.selected_index = visible_count - 1;
    }

    if state.top_index > state.selected_index {
        state.top_index = state.selected_index;
    }

    let page_size = page_rows.max(1);
    if state.selected_index >= state.top_index + page_size {
        state.top_index = state.selected_index - page_size + 1;
    }
}

fn cycle_sort_mode(mode: UiSortMode) -> UiSortMode {
    match mode {
        UiSortMode::SectionKey => UiSortMode::TempDesc,
        UiSortMode::TempDesc => UiSortMode::Name,
        UiSortMode::Name => UiSortMode::Key,
        UiSortMode::Key => UiSortMode::Type,
        UiSortMode::Type => UiSortMode::SectionKey,
    }
}

fn section_color(section: SensorSection) -> Color {
    match section {
        SensorSection::Cpu => Color::Green,
        SensorSection::Gpu => Color::Magenta,
        SensorSection::Battery => Color::Yellow,
    }
}

fn section_label(section: SensorSection) -> &'static str {
    match section {
        SensorSection::Cpu => "CPU",
        SensorSection::Gpu => "GPU",
        SensorSection::Battery => "BATTERY",
    }
}

fn page_accent_color(page: ViewPage) -> Color {
    match page {
        ViewPage::Cpu => Color::Green,
        ViewPage::Gpu => Color::Magenta,
        ViewPage::Battery => Color::Yellow,
    }
}

fn page_tint_color(page: ViewPage) -> Color {
    match page {
        ViewPage::Cpu => Color::Rgb(12, 28, 18),
        ViewPage::Gpu => Color::Rgb(28, 16, 32),
        ViewPage::Battery => Color::Rgb(32, 30, 14),
    }
}

fn temp_style(temp_c: f64) -> Style {
    match ui_temp_to_heat_level(temp_c, 70.0, 85.0) {
        UiHeatLevel::Cool => Style::default().fg(Color::Green),
        UiHeatLevel::Warm => Style::default().fg(Color::Yellow),
        UiHeatLevel::Hot => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    }
}

fn meter_segment_style(level: UiHeatLevel, highlighted: bool) -> Style {
    let accent = if highlighted {
        Modifier::BOLD
    } else {
        Modifier::DIM
    };

    match level {
        UiHeatLevel::Cool => Style::default().fg(Color::Green).add_modifier(accent),
        UiHeatLevel::Warm => Style::default().fg(Color::Yellow).add_modifier(accent),
        UiHeatLevel::Hot => Style::default().fg(Color::Red).add_modifier(accent),
    }
}

fn meter_empty_style() -> Style {
    Style::default()
        .fg(Color::Magenta)
        .add_modifier(Modifier::DIM)
}

fn push_meter_run(
    spans: &mut Vec<Span<'static>>,
    level: UiHeatLevel,
    highlighted: bool,
    count: usize,
) {
    if count == 0 {
        return;
    }

    spans.push(Span::styled(
        "|".repeat(count),
        meter_segment_style(level, highlighted),
    ));
}

fn meter_line_from_ratio(ratio: f64, width: usize) -> Line<'static> {
    if width == 0 {
        return Line::from("");
    }

    let filled = ui_meter_filled_columns(ratio, width as i32).max(0) as usize;
    let mut spans = Vec::with_capacity(6);
    spans.push(Span::styled(
        "[",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));

    let mut run_level = UiHeatLevel::Cool;
    let mut run_count = 0usize;
    let mut started = false;
    for column in 0..filled {
        let level = ui_meter_heat_level_for_column(column as i32, width as i32);
        if !started {
            run_level = level;
            run_count = 1;
            started = true;
            continue;
        }

        if level == run_level {
            run_count += 1;
            continue;
        }

        push_meter_run(&mut spans, run_level, true, run_count);
        run_level = level;
        run_count = 1;
    }

    if started {
        push_meter_run(&mut spans, run_level, true, run_count);
    }

    let empty = width.saturating_sub(filled);
    if empty > 0 {
        spans.push(Span::styled("|".repeat(empty), meter_empty_style()));
    }

    spans.push(Span::styled(
        "]",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));
    Line::from(spans)
}

fn meter_line(temp_c: f64, width: usize) -> Line<'static> {
    let ratio = ui_temp_to_ratio(temp_c, 25.0, 100.0);
    meter_line_from_ratio(ratio, width)
}

fn loading_meter_line(width: usize, frame: u32) -> Line<'static> {
    if width == 0 {
        return Line::from("");
    }

    let pulse_radius = ((width as i32) / 9).clamp(2, 6);
    let mut spans = Vec::with_capacity(6);
    spans.push(Span::styled(
        "[",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));

    let mut run_level = UiHeatLevel::Cool;
    let mut run_highlighted = false;
    let mut run_count = 0usize;
    let mut started = false;
    for column in 0..width {
        let highlighted = ui_meter_loading_column_highlighted(
            column as i32,
            width as i32,
            frame,
            pulse_radius,
            2,
        );
        let level = ui_meter_heat_level_for_column(column as i32, width as i32);
        if !started {
            run_level = level;
            run_highlighted = highlighted;
            run_count = 1;
            started = true;
            continue;
        }

        if level == run_level && highlighted == run_highlighted {
            run_count += 1;
            continue;
        }

        push_meter_run(&mut spans, run_level, run_highlighted, run_count);
        run_level = level;
        run_highlighted = highlighted;
        run_count = 1;
    }

    if started {
        push_meter_run(&mut spans, run_level, run_highlighted, run_count);
    }

    spans.push(Span::styled(
        "]",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));
    Line::from(spans)
}

fn compute_sensors_table_widths(content_width: u16) -> SensorsTableWidths {
    const MARKER_WIDTH: u16 = 1;
    const KEY_WIDTH: u16 = 4;
    const SECTION_WIDTH: u16 = 7;
    const TYPE_WIDTH: u16 = 11;
    const TEMP_WIDTH: u16 = 8;
    const COLUMN_COUNT: u16 = 7;
    const SPACING: u16 = 1;

    let fixed = MARKER_WIDTH
        + KEY_WIDTH
        + SECTION_WIDTH
        + TYPE_WIDTH
        + TEMP_WIDTH
        + SPACING * (COLUMN_COUNT - 1);
    let flex = content_width.saturating_sub(fixed);

    let mut meter = (flex * 11) / 20;
    let mut name = flex.saturating_sub(meter);

    if flex > 0 {
        let meter_min = cmp::min(10, flex);
        if meter < meter_min {
            let deficit = meter_min - meter;
            meter = meter_min;
            name = name.saturating_sub(deficit);
        }

        let remaining_after_meter = flex.saturating_sub(meter);
        let name_min = cmp::min(8, remaining_after_meter);
        if name < name_min {
            let deficit = name_min - name;
            name = name_min;
            meter = meter.saturating_sub(deficit);
        }
    }

    SensorsTableWidths {
        marker: MARKER_WIDTH,
        key: KEY_WIDTH,
        section: SECTION_WIDTH,
        sensor_type: TYPE_WIDTH,
        temp: TEMP_WIDTH,
        meter,
        name,
    }
}

fn now_timestamp_label() -> String {
    const TS_FORMAT: &[FormatItem<'static>] =
        format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

    let now_utc = OffsetDateTime::now_utc();
    let now_local = match UtcOffset::current_local_offset() {
        Ok(offset) => now_utc.to_offset(offset),
        Err(_) => now_utc,
    };

    now_local
        .format(TS_FORMAT)
        .unwrap_or_else(|_| "0000-00-00 00:00:00".to_string())
}

#[derive(Clone, Copy, Debug, Default)]
struct SectionTempStats {
    count: usize,
    avg_temp_c: f64,
    max_temp_c: f64,
}

fn compute_section_temp_stats(
    entries: &[SensorEntry],
    section: SensorSection,
) -> Option<SectionTempStats> {
    let mut count = 0usize;
    let mut sum = 0.0f64;
    let mut max = 0.0f64;

    for entry in entries {
        if !entry.active || entry.section != section {
            continue;
        }
        count += 1;
        sum += entry.last_temp_c;
        if count == 1 || entry.last_temp_c > max {
            max = entry.last_temp_c;
        }
    }

    if count == 0 {
        return None;
    }

    Some(SectionTempStats {
        count,
        avg_temp_c: sum / count as f64,
        max_temp_c: max,
    })
}

fn section_summary_line(
    section: SensorSection,
    stats: Option<SectionTempStats>,
    min_temp_c: f64,
    max_temp_c: f64,
    loading: bool,
    frame: u32,
    meter_width: usize,
) -> Line<'static> {
    let label_style = Style::default()
        .fg(section_color(section))
        .add_modifier(Modifier::BOLD);

    let mut spans = vec![
        Span::styled(format!("{:<8}", section_label(section)), label_style),
        Span::raw(" "),
    ];

    if let Some(data) = stats {
        let ratio = ui_temp_to_ratio(data.avg_temp_c, min_temp_c, max_temp_c);
        spans.extend(meter_line_from_ratio(ratio, meter_width).spans);
        spans.push(Span::raw(" "));
        let level = ui_temp_to_heat_level(data.max_temp_c, 70.0, 85.0);
        spans.push(Span::styled(
            format!(
                "avg {:>5.1}C max {:>5.1}C ({})",
                data.avg_temp_c, data.max_temp_c, data.count
            ),
            meter_segment_style(level, true),
        ));
    } else if loading {
        spans.extend(loading_meter_line(meter_width, frame).spans);
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            "Loading sensors...",
            Style::default().fg(Color::Gray),
        ));
    } else {
        spans.push(Span::styled(
            "No active sensors",
            Style::default().fg(Color::Gray),
        ));
    }

    Line::from(spans)
}

fn battery_summary_line(
    battery: &BatteryStats,
    loading: bool,
    frame: u32,
    meter_width: usize,
) -> Line<'static> {
    let label_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);

    let mut spans = vec![Span::styled("BATTERY ", label_style)];
    if !battery.available && loading {
        spans.extend(loading_meter_line(meter_width, frame).spans);
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            "Loading battery...",
            Style::default().fg(Color::Gray),
        ));
        return Line::from(spans);
    }

    let charge_percent = if battery.charge_percent.is_finite() {
        battery.charge_percent.max(0.0)
    } else {
        0.0
    };
    let ratio = (charge_percent / 100.0).clamp(0.0, 1.0);
    spans.extend(meter_line_from_ratio(ratio, meter_width).spans);
    spans.push(Span::raw(" "));

    let level = if charge_percent < 20.0 {
        UiHeatLevel::Hot
    } else if charge_percent < 50.0 {
        UiHeatLevel::Warm
    } else {
        UiHeatLevel::Cool
    };

    spans.push(Span::styled(
        format!("{:>5.1}% ", charge_percent),
        meter_segment_style(level, true),
    ));
    spans.push(Span::styled(
        if battery.is_charging {
            "charging "
        } else {
            "discharging "
        },
        meter_segment_style(level, true),
    ));
    spans.push(Span::styled(
        if battery.power_w.is_finite() {
            format!("{:.2}W", battery.power_w.abs())
        } else {
            "n/a".to_string()
        },
        meter_segment_style(level, true),
    ));

    Line::from(spans)
}

fn draw_top_panel(f: &mut Frame<'_>, area: Rect, snapshot: &RuntimeSnapshot, frame: u32) {
    let active_cpu = sensor_count_active_by_section(&snapshot.sensors.items, SensorSection::Cpu);
    let active_gpu = sensor_count_active_by_section(&snapshot.sensors.items, SensorSection::Gpu);
    let active_battery =
        sensor_count_active_by_section(&snapshot.sensors.items, SensorSection::Battery);
    let active_total = active_cpu + active_gpu + active_battery;
    let loading = matches!(snapshot.stage, LoadStage::Profile | LoadStage::Keyspace);
    let accent = Color::Cyan;

    let top_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(5)])
        .split(area);
    let header_area = top_chunks[0];
    let summary_area = top_chunks[1];

    let inner_width = summary_area.width.saturating_sub(2);
    let meter_width = usize::from(inner_width.saturating_sub(44).clamp(10, 40));

    let timestamp = now_timestamp_label();
    let header_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let headline = Line::from(Span::styled(
        format!(
            "SiliconTherm | {} | interval {:.1}s | sensors {} | {}",
            timestamp, snapshot.interval_seconds, active_total, snapshot.stage_message
        ),
        header_style,
    ));
    f.render_widget(Paragraph::new(vec![headline]), header_area);

    let lines = vec![
        section_summary_line(
            SensorSection::Cpu,
            compute_section_temp_stats(&snapshot.sensors.items, SensorSection::Cpu),
            20.0,
            100.0,
            loading,
            frame,
            meter_width,
        ),
        section_summary_line(
            SensorSection::Gpu,
            compute_section_temp_stats(&snapshot.sensors.items, SensorSection::Gpu),
            20.0,
            105.0,
            loading,
            frame,
            meter_width,
        ),
        battery_summary_line(&snapshot.battery, loading, frame, meter_width),
    ];

    let panel = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(panel, summary_area);
}

fn draw_battery_panel(f: &mut Frame<'_>, area: Rect, battery: &BatteryStats) {
    let mut lines = Vec::new();
    if !battery.available {
        lines.push(Line::from("Battery data unavailable"));
    } else {
        if battery.current_capacity_mah >= 0 && battery.max_capacity_mah > 0 {
            lines.push(Line::from(format!(
                "Capacity: {} / {} mAh ({:.1}%)",
                battery.current_capacity_mah, battery.max_capacity_mah, battery.charge_percent
            )));
        } else {
            lines.push(Line::from("Capacity: n/a"));
        }

        lines.push(Line::from(format!(
            "Voltage: {} mV | Current: {} mA",
            battery.voltage_mv, battery.amperage_ma
        )));

        lines.push(Line::from(format!(
            "Cycles: {} | {}",
            battery.cycle_count,
            if battery.is_charging {
                "Charging"
            } else {
                "Discharging"
            }
        )));

        if battery.power_w.is_finite() {
            lines.push(Line::from(format!(
                "Power: {:.2} W | Session: {:.2} mWh",
                battery.power_w.abs(),
                battery.session_discharged_mwh
            )));
        }
    }

    let panel = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Battery"))
        .wrap(Wrap { trim: true });
    f.render_widget(panel, area);
}

fn draw_info_panel(
    f: &mut Frame<'_>,
    area: Rect,
    state: &UiState,
    snapshot: &RuntimeSnapshot,
    visible_count: usize,
) {
    let mut lines = Vec::new();

    lines.push(Line::from(format!(
        "Visible sensors: {} | Sort: {} | Filter: {}",
        visible_count,
        ui_sort_mode_label(state.sort_mode),
        if state.filter_enabled { "ON" } else { "OFF" }
    )));

    let section = state.current_page.section();
    lines.push(Line::from(vec![
        Span::raw("Type stats ("),
        Span::styled(
            section_label(section),
            Style::default()
                .fg(section_color(section))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("):"),
    ]));

    let tiles = ui_build_type_tiles(&snapshot.sensors.items, section, 4);
    if tiles.is_empty() {
        lines.push(Line::from("Types: [no sensors]"));
    } else {
        let mut spans = Vec::new();
        for tile in tiles {
            spans.push(Span::styled(
                format!("{} ", tile.type_label),
                Style::default()
                    .fg(section_color(section))
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                format!("{:.1}C", tile.avg_temp_c),
                temp_style(tile.avg_temp_c),
            ));
            spans.push(Span::styled(
                format!(" ({})  ", tile.count),
                Style::default().fg(Color::Gray),
            ));
        }
        lines.push(Line::from(spans));
    }

    let panel = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Info"))
        .wrap(Wrap { trim: true });
    f.render_widget(panel, area);
}

fn draw_help_panel(f: &mut Frame<'_>, area: Rect) {
    let lines = vec![
        Line::from("q/F10 quit | Tab/Shift+Tab pages (1/2/3)"),
        Line::from("Arrows/PgUp/PgDn/Home/End navigate"),
        Line::from("/ or F3 search | F4 filter | F6 sort"),
        Line::from("n/N next/prev match | +/- interval"),
        Line::from("Space pause/resume | Mouse tabs/headers"),
    ];

    let panel = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Help"))
        .wrap(Wrap { trim: true });
    f.render_widget(panel, area);
}

fn draw_sensors_table(
    f: &mut Frame<'_>,
    area: Rect,
    state: &mut UiState,
    snapshot: &RuntimeSnapshot,
    entries: &[SensorEntry],
    visible_indices: &[usize],
) {
    let accent = page_accent_color(state.current_page);
    let tint = page_tint_color(state.current_page);
    f.render_widget(Block::default().style(Style::default().bg(tint)), area);

    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent).add_modifier(Modifier::BOLD))
        .title_style(Style::default().fg(accent).add_modifier(Modifier::BOLD))
        .title(format!("Sensors ({})", state.current_page.label()));
    f.render_widget(outer_block, area);

    state.tab_targets.clear();
    state.header_targets.clear();

    if area.width <= 2 || area.height <= 2 {
        return;
    }

    let inner = Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let selector_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: 1,
    };

    let labels = [
        (ViewPage::Cpu, "CPU", Color::Green),
        (ViewPage::Gpu, "GPU", Color::Magenta),
        (ViewPage::Battery, "Battery", Color::Yellow),
    ];
    let mut selector_spans = Vec::new();
    let mut cursor = selector_area.x;
    for (index, (page, label, color)) in labels.iter().enumerate() {
        if index > 0 {
            selector_spans.push(Span::raw(" "));
            cursor = cursor.saturating_add(1);
        }

        let tab_text = format!("[ {} ]", label);
        let selected = *page == state.current_page;
        let style = if selected {
            Style::default()
                .fg(Color::Black)
                .bg(*color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::DIM)
        };
        let width = tab_text.chars().count() as u16;

        let start_x = cursor;
        let end_x = start_x.saturating_add(width).saturating_sub(1);
        if width > 0
            && start_x < selector_area.x.saturating_add(selector_area.width)
            && end_x >= selector_area.x
        {
            state.tab_targets.push(TabTarget {
                y: selector_area.y,
                start_x,
                end_x,
                page: *page,
            });
        }

        selector_spans.push(Span::styled(tab_text, style));
        cursor = end_x.saturating_add(1);
    }

    selector_spans.push(Span::styled(
        " | Tab/Shift-Tab or click to switch section",
        Style::default().fg(Color::White),
    ));
    f.render_widget(
        Paragraph::new(Line::from(selector_spans)).wrap(Wrap { trim: true }),
        selector_area,
    );

    let show_search = state.search_mode || !state.query.is_empty();
    if show_search && selector_area.width > 12 {
        let query = if state.query.is_empty() {
            "<empty>"
        } else {
            &state.query
        };
        let suffix = if state.search_mode { "_" } else { "" };
        let search_text = format!(" Search: {query}{suffix} ");
        let search_width = (search_text.chars().count() as u16).min(selector_area.width);
        let search_x = selector_area
            .x
            .saturating_add(selector_area.width.saturating_sub(search_width));
        let search_style = if state.search_mode {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::Cyan)
                .bg(Color::Rgb(18, 24, 36))
                .add_modifier(Modifier::BOLD)
        };

        f.render_widget(
            Paragraph::new(Line::from(Span::styled(search_text, search_style))),
            Rect {
                x: search_x,
                y: selector_area.y,
                width: search_width,
                height: 1,
            },
        );
    }

    if inner.height <= 1 {
        return;
    }

    let table_area = Rect {
        x: inner.x,
        y: inner.y.saturating_add(1),
        width: inner.width,
        height: inner.height.saturating_sub(1),
    };

    let page_rows = usize::from(table_area.height.saturating_sub(1)).max(1);
    ensure_selection_visible(state, visible_indices.len(), page_rows);

    let content_width = table_area.width;
    let widths = compute_sensors_table_widths(content_width);
    let spacing = 1u16;
    let table_columns = [
        widths.marker,
        widths.key,
        widths.section,
        widths.sensor_type,
        widths.temp,
        widths.meter,
        widths.name,
    ];
    let header_y = table_area.y;
    let mut x = table_area.x;
    let sorts = [
        None,
        Some(UiSortMode::Key),
        Some(UiSortMode::SectionKey),
        Some(UiSortMode::Type),
        Some(UiSortMode::TempDesc),
        Some(UiSortMode::TempDesc),
        Some(UiSortMode::Name),
    ];
    for (index, width) in table_columns.iter().enumerate() {
        if *width > 0 {
            if let Some(sort_mode) = sorts[index] {
                state.header_targets.push(HeaderTarget {
                    y: header_y,
                    start_x: x,
                    end_x: x.saturating_add(*width).saturating_sub(1),
                    sort_mode,
                });
            }
            x = x.saturating_add(*width).saturating_add(spacing);
        }
    }

    let end = cmp::min(state.top_index + page_rows, visible_indices.len());
    let page_slice = &visible_indices[state.top_index..end];
    let selected_style = Style::default()
        .fg(accent)
        .add_modifier(Modifier::BOLD | Modifier::REVERSED);

    let mut rows = Vec::new();
    if page_slice.is_empty() {
        let spinner = ["|", "/", "-", "\\"];
        let loading_label = spinner[(state.frame as usize) % spinner.len()];
        let loading = snapshot.stage != LoadStage::Live && !snapshot.failed;
        let message = if loading {
            format!("{loading_label} {}", snapshot.stage_message)
        } else {
            "No active sensors detected.".to_string()
        };
        let meter_cell = if loading && widths.meter >= 3 {
            Cell::from(loading_meter_line(
                widths.meter.saturating_sub(2) as usize,
                state.frame,
            ))
        } else {
            Cell::from("")
        };
        let name_cell = Cell::from(message.clone());

        rows.push(Row::new(vec![
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
            meter_cell,
            name_cell,
        ]));
    } else {
        for (row_offset, index) in page_slice.iter().enumerate() {
            if let Some(entry) = entries.get(*index) {
                let selected = state.top_index + row_offset == state.selected_index;
                let section_style = Style::default().fg(section_color(entry.section));
                let temp_label = format!("{:.2}C", entry.last_temp_c);
                let meter_cell = if widths.meter >= 3 {
                    Cell::from(meter_line(
                        entry.last_temp_c,
                        widths.meter.saturating_sub(2) as usize,
                    ))
                } else {
                    Cell::from("")
                };
                let key_label = format!("{:<4}", entry.key.chars().take(4).collect::<String>());
                let marker_cell = if selected {
                    Cell::from(">").style(selected_style)
                } else {
                    Cell::from("|").style(section_style.add_modifier(Modifier::BOLD))
                };

                let key_cell = if selected {
                    Cell::from(key_label).style(selected_style)
                } else {
                    Cell::from(key_label).style(section_style)
                };
                let section_cell = if selected {
                    Cell::from(section_label(entry.section)).style(selected_style)
                } else {
                    Cell::from(section_label(entry.section)).style(section_style)
                };
                let type_cell = if selected {
                    Cell::from(entry.type_label.clone()).style(selected_style)
                } else {
                    Cell::from(entry.type_label.clone())
                };
                let temp_cell = if selected {
                    Cell::from(temp_label).style(selected_style)
                } else {
                    Cell::from(temp_label).style(temp_style(entry.last_temp_c))
                };
                let name_cell = if selected {
                    Cell::from(entry.name.clone()).style(selected_style)
                } else {
                    Cell::from(entry.name.clone())
                };

                rows.push(
                    Row::new(vec![
                        marker_cell,
                        key_cell,
                        section_cell,
                        type_cell,
                        temp_cell,
                        meter_cell,
                        name_cell,
                    ])
                    .height(1),
                );
            }
        }
    }

    let header_active = |active: bool| {
        if active {
            Style::default()
                .fg(accent)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED | Modifier::REVERSED)
        } else {
            Style::default().fg(accent).add_modifier(Modifier::BOLD)
        }
    };
    let temp_active = state.sort_mode == UiSortMode::TempDesc;
    let header = Row::new(vec![
        Cell::from("|").style(header_active(false)),
        Cell::from("Key").style(header_active(state.sort_mode == UiSortMode::Key)),
        Cell::from("Section").style(header_active(state.sort_mode == UiSortMode::SectionKey)),
        Cell::from("Type").style(header_active(state.sort_mode == UiSortMode::Type)),
        Cell::from("Temp").style(header_active(temp_active)),
        Cell::from("Meter").style(header_active(temp_active)),
        Cell::from("Name").style(header_active(state.sort_mode == UiSortMode::Name)),
    ]);

    let table = Table::new(
        rows,
        [
            Constraint::Length(widths.marker),
            Constraint::Length(widths.key),
            Constraint::Length(widths.section),
            Constraint::Length(widths.sensor_type),
            Constraint::Length(widths.temp),
            Constraint::Length(widths.meter),
            Constraint::Length(widths.name),
        ],
    )
    .column_spacing(1)
    .header(header);

    let mut table_state = TableState::default();
    if !page_slice.is_empty() {
        table_state.select(Some(state.selected_index - state.top_index));
    }

    f.render_stateful_widget(table, table_area, &mut table_state);
}

fn draw_ui(f: &mut Frame<'_>, state: &mut UiState, snapshot: &RuntimeSnapshot) {
    let area = f.area();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(10),
            Constraint::Length(8),
        ])
        .split(area);

    draw_top_panel(f, layout[0], snapshot, state.frame);

    let section = state.current_page.section();
    let mut visible_indices = ui_build_visible_indices_by_section(
        &snapshot.sensors.items,
        Some(&state.query),
        state.filter_enabled,
        true,
        section,
    );

    ui_sort_visible_indices(
        &snapshot.sensors.items,
        &mut visible_indices,
        state.sort_mode,
    );
    draw_sensors_table(
        f,
        layout[1],
        state,
        snapshot,
        &snapshot.sensors.items,
        &visible_indices,
    );

    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(46),
            Constraint::Percentage(34),
            Constraint::Percentage(20),
        ])
        .split(layout[2]);

    draw_info_panel(f, bottom[0], state, snapshot, visible_indices.len());
    draw_battery_panel(f, bottom[1], &snapshot.battery);
    draw_help_panel(f, bottom[2]);
}

fn handle_search_input(state: &mut UiState, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            state.search_mode = false;
            state.status = "search canceled".to_string();
        }
        KeyCode::Enter => {
            state.search_mode = false;
            state.status = "search applied".to_string();
        }
        KeyCode::Backspace => {
            state.query.pop();
        }
        KeyCode::Char(c) => {
            if !key.modifiers.is_empty() {
                return;
            }
            if state.query.len() < 127 {
                state.query.push(c);
            }
        }
        _ => {}
    }
}

fn hit_target(row: u16, col: u16, y: u16, start_x: u16, end_x: u16) -> bool {
    row == y && col >= start_x && col <= end_x
}

fn handle_mouse_event(state: &mut UiState, event: MouseEvent) {
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            for target in &state.tab_targets {
                if hit_target(
                    event.row,
                    event.column,
                    target.y,
                    target.start_x,
                    target.end_x,
                ) {
                    state.current_page = target.page;
                    state.selected_index = 0;
                    state.top_index = 0;
                    state.status =
                        format!("page changed to {} (mouse)", state.current_page.label());
                    return;
                }
            }

            for target in &state.header_targets {
                if hit_target(
                    event.row,
                    event.column,
                    target.y,
                    target.start_x,
                    target.end_x,
                ) {
                    state.sort_mode = target.sort_mode;
                    state.status = format!(
                        "sort changed to {} (mouse)",
                        ui_sort_mode_label(state.sort_mode)
                    );
                    return;
                }
            }
        }
        MouseEventKind::ScrollUp => {
            if state.selected_index > 0 {
                state.selected_index -= 1;
            }
        }
        MouseEventKind::ScrollDown => {
            state.selected_index = state.selected_index.saturating_add(1);
        }
        _ => {}
    }
}

fn handle_navigation_key(
    state: &mut UiState,
    runtime: &RuntimeHandle,
    snapshot: &RuntimeSnapshot,
    key: KeyEvent,
) {
    let section_filter = state.current_page.section();
    let mut visible_indices = ui_build_visible_indices_by_section(
        &snapshot.sensors.items,
        Some(&state.query),
        state.filter_enabled,
        true,
        section_filter,
    );

    ui_sort_visible_indices(
        &snapshot.sensors.items,
        &mut visible_indices,
        state.sort_mode,
    );

    if state.search_mode {
        handle_search_input(state, key);
        return;
    }

    match key.code {
        KeyCode::F(10) | KeyCode::Char('q') | KeyCode::Char('Q') => {
            state.running = false;
        }
        KeyCode::Char('c')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            state.running = false;
        }
        KeyCode::Up => {
            if state.selected_index > 0 {
                state.selected_index -= 1;
            }
        }
        KeyCode::Down => {
            if state.selected_index + 1 < visible_indices.len() {
                state.selected_index += 1;
            }
        }
        KeyCode::PageUp => {
            let page = 12usize;
            state.selected_index = state.selected_index.saturating_sub(page);
        }
        KeyCode::PageDown => {
            if visible_indices.is_empty() {
                state.selected_index = 0;
            } else {
                let page = 12usize;
                state.selected_index =
                    cmp::min(state.selected_index + page, visible_indices.len() - 1);
            }
        }
        KeyCode::Home => {
            state.selected_index = 0;
        }
        KeyCode::End => {
            if !visible_indices.is_empty() {
                state.selected_index = visible_indices.len() - 1;
            }
        }
        KeyCode::Left | KeyCode::Char('[') => {
            state.current_page = state.current_page.cycle(-1);
            state.selected_index = 0;
            state.top_index = 0;
            state.status = format!("page changed to {}", state.current_page.label());
        }
        KeyCode::Right | KeyCode::Char(']') => {
            state.current_page = state.current_page.cycle(1);
            state.selected_index = 0;
            state.top_index = 0;
            state.status = format!("page changed to {}", state.current_page.label());
        }
        KeyCode::Tab => {
            state.current_page = state.current_page.cycle(1);
            state.selected_index = 0;
            state.top_index = 0;
            state.status = format!("page changed to {}", state.current_page.label());
        }
        KeyCode::BackTab => {
            state.current_page = state.current_page.cycle(-1);
            state.selected_index = 0;
            state.top_index = 0;
            state.status = format!("page changed to {}", state.current_page.label());
        }
        KeyCode::Char('1') => {
            state.current_page = ViewPage::Cpu;
            state.selected_index = 0;
            state.top_index = 0;
        }
        KeyCode::Char('2') => {
            state.current_page = ViewPage::Gpu;
            state.selected_index = 0;
            state.top_index = 0;
        }
        KeyCode::Char('3') => {
            state.current_page = ViewPage::Battery;
            state.selected_index = 0;
            state.top_index = 0;
        }
        KeyCode::F(3) | KeyCode::Char('/') => {
            state.search_mode = true;
            state.status = "search mode".to_string();
        }
        KeyCode::F(4) | KeyCode::Char('f') | KeyCode::Char('F') => {
            state.filter_enabled = !state.filter_enabled;
            state.selected_index = 0;
            state.top_index = 0;
            state.status = format!(
                "filter {}",
                if state.filter_enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            );
        }
        KeyCode::F(6) | KeyCode::Char('s') | KeyCode::Char('S') => {
            state.sort_mode = cycle_sort_mode(state.sort_mode);
            state.status = format!("sort changed to {}", ui_sort_mode_label(state.sort_mode));
        }
        KeyCode::Char('n') => {
            state.selected_index = ui_find_next_match(
                &snapshot.sensors.items,
                &visible_indices,
                state.selected_index,
                Some(&state.query),
                true,
            );
        }
        KeyCode::Char('N') => {
            state.selected_index = ui_find_next_match(
                &snapshot.sensors.items,
                &visible_indices,
                state.selected_index,
                Some(&state.query),
                false,
            );
        }
        KeyCode::Char(' ') => {
            let paused_now = runtime.lock().expect("runtime mutex poisoned").paused;
            runtime_set_paused(runtime, !paused_now);
            state.status = "pause toggled".to_string();
        }
        KeyCode::Char('+') => {
            let interval = runtime
                .lock()
                .expect("runtime mutex poisoned")
                .interval_seconds;
            let next = (interval - 0.2).max(0.2);
            runtime_set_interval(runtime, next);
            state.status = format!("interval {:.1}s", next);
        }
        KeyCode::Char('-') => {
            let interval = runtime
                .lock()
                .expect("runtime mutex poisoned")
                .interval_seconds;
            let next = (interval + 0.2).min(10.0);
            runtime_set_interval(runtime, next);
            state.status = format!("interval {:.1}s", next);
        }
        _ => {}
    }
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut out = io::stdout();
        let _ = execute!(out, LeaveAlternateScreen, DisableMouseCapture);
    }
}

pub fn tui_run(interval_seconds: f64) -> Result<(), String> {
    enable_raw_mode().map_err(|e| format!("could not enable raw mode: {e}"))?;

    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)
        .map_err(|e| format!("could not enter alternate screen: {e}"))?;
    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(out);
    let mut terminal =
        Terminal::new(backend).map_err(|e| format!("could not initialize terminal: {e}"))?;

    let runtime = Arc::new(Mutex::new(RuntimeState {
        stop_requested: false,
        paused: false,
        failed: false,
        stage: LoadStage::Profile,
        stage_message: "Starting background loader".to_string(),
        interval_seconds,
        sensors: SensorCollection::default(),
        battery: BatteryStats::default(),
    }));

    let runtime_for_thread = Arc::clone(&runtime);
    let worker = thread::Builder::new()
        .name("silicontherm-runtime".to_string())
        .spawn(move || runtime_thread_main(runtime_for_thread))
        .map_err(|e| format!("could not start runtime thread: {e}"))?;

    let mut ui_state = UiState::default();

    while ui_state.running {
        let snapshot = runtime_snapshot(&runtime);

        terminal
            .draw(|f| draw_ui(f, &mut ui_state, &snapshot))
            .map_err(|e| format!("draw failed: {e}"))?;

        if event::poll(Duration::from_millis(80)).map_err(|e| format!("event poll failed: {e}"))? {
            let event = event::read().map_err(|e| format!("event read failed: {e}"))?;
            match event {
                Event::Key(key)
                    if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat =>
                {
                    handle_navigation_key(&mut ui_state, &runtime, &snapshot, key);
                }
                Event::Mouse(mouse) => {
                    handle_mouse_event(&mut ui_state, mouse);
                }
                _ => {}
            }
        }

        ui_state.frame = ui_state.frame.wrapping_add(1);
    }

    runtime_request_stop(&runtime);
    worker
        .join()
        .map_err(|_| "runtime thread panicked".to_string())?;

    Ok(())
}
