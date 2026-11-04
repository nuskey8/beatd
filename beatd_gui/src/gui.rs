use beatd::{Analyzer, FFT_SIZE as FFT};
use cpal::Sample;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use eframe::egui;
use rosc::{OscMessage, OscPacket, OscType};
use std::{
    net::UdpSocket,
    sync::mpsc::{self, Receiver, SyncSender},
    time::{Duration, Instant},
};

const BANDS: [(f32, f32); 4] = [
    (40.0, 150.0),
    (150.0, 500.0),
    (500.0, 2000.0),
    (2000.0, 10000.0),
];

#[derive(Clone, Copy)]
struct Palette {
    dark: bool,
    wave_bg: egui::Color32,
    wave_center: egui::Color32,
    wave: egui::Color32,
    beat: egui::Color32,
    spectrum: egui::Color32,
    meter_bg: egui::Color32,
    threshold_active: egui::Color32,
    threshold_inactive: egui::Color32,
    bands: [egui::Color32; 4],
    level: egui::Color32,
    frame: egui::Color32,
    dot_inactive: egui::Color32,
    widget_bg: egui::Color32,
}

impl Palette {
    fn new(dark: bool) -> Self {
        if dark {
            Self {
                dark,
                wave_bg: egui::Color32::from_gray(25),
                wave_center: egui::Color32::from_gray(80),
                wave: egui::Color32::from_rgb(80, 235, 155),
                beat: egui::Color32::WHITE,
                spectrum: egui::Color32::LIGHT_GREEN,
                meter_bg: egui::Color32::from_gray(30),
                threshold_active: egui::Color32::WHITE,
                threshold_inactive: egui::Color32::from_gray(150),
                bands: [
                    egui::Color32::from_rgb(163, 83, 109),
                    egui::Color32::from_rgb(225, 196, 83),
                    egui::Color32::from_rgb(80, 210, 111),
                    egui::Color32::from_rgb(117, 126, 240),
                ],
                level: egui::Color32::LIGHT_BLUE,
                frame: egui::Color32::from_gray(35),
                dot_inactive: egui::Color32::BLACK,
                widget_bg: egui::Color32::BLACK,
            }
        } else {
            Self {
                dark,
                wave_bg: egui::Color32::from_gray(238),
                wave_center: egui::Color32::from_gray(160),
                wave: egui::Color32::from_rgb(0, 130, 80),
                beat: egui::Color32::from_gray(50),
                spectrum: egui::Color32::from_rgb(20, 120, 65),
                meter_bg: egui::Color32::from_gray(225),
                threshold_active: egui::Color32::from_gray(30),
                threshold_inactive: egui::Color32::from_gray(120),
                bands: [
                    egui::Color32::from_rgb(150, 55, 85),
                    egui::Color32::from_rgb(150, 120, 0),
                    egui::Color32::from_rgb(20, 125, 65),
                    egui::Color32::from_rgb(70, 80, 190),
                ],
                level: egui::Color32::from_rgb(20, 95, 160),
                frame: egui::Color32::from_gray(242),
                dot_inactive: egui::Color32::from_gray(190),
                widget_bg: egui::Color32::WHITE,
            }
        }
    }

    fn band_fill(&self, i: usize, active: bool) -> egui::Color32 {
        let color = self.bands[i];
        if self.dark {
            color.gamma_multiply(if active { 0.65 } else { 0.4 })
        } else {
            egui::Color32::from_rgba_unmultiplied(
                color.r(),
                color.g(),
                color.b(),
                if active { 48 } else { 28 },
            )
        }
    }

    fn band_frame(&self, i: usize) -> egui::Color32 {
        let color = self.bands[i];
        if self.dark {
            color.gamma_multiply(0.18)
        } else {
            egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 22)
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
enum TriggerMode {
    #[default]
    Trigger,
    Level,
}

impl TriggerMode {
    fn label(self) -> &'static str {
        match self {
            Self::Trigger => "Trigger",
            Self::Level => "Level",
        }
    }
}

fn mode_combo(ui: &mut egui::Ui, id: impl std::hash::Hash, mode: &mut TriggerMode, width: f32) {
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(mode.label())
        .show_ui(ui, |ui| {
            ui.selectable_value(mode, TriggerMode::Trigger, "Trigger");
            ui.selectable_value(mode, TriggerMode::Level, "Level");
        });
}

fn field_label(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).strong());
}

fn check_label(ui: &mut egui::Ui, checked: &mut bool, label: &str) {
    ui.add(egui::Checkbox::new(
        checked,
        egui::RichText::new(label).strong(),
    ));
}

#[derive(Clone, Copy)]
struct Trigger {
    above_since: Option<Instant>,
    below_since: Option<Instant>,
    last_on: Option<Instant>,
    armed: bool,
}

impl Default for Trigger {
    fn default() -> Self {
        Self {
            above_since: None,
            below_since: None,
            last_on: None,
            armed: true,
        }
    }
}

impl Trigger {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn update(
        &mut self,
        active: bool,
        above: bool,
        now: Instant,
        on_delay: f32,
        off_delay: f32,
        decay: f32,
    ) -> (bool, Option<bool>) {
        let mut active = active;
        let mut event = None;
        if above {
            self.below_since = None;
            if !active && self.armed {
                let since = *self.above_since.get_or_insert(now);
                if now.duration_since(since).as_secs_f32() >= on_delay {
                    active = true;
                    self.last_on = Some(now);
                    self.above_since = None;
                    event = Some(true);
                }
            }
        } else {
            self.above_since = None;
            self.armed = true;
            if active {
                let since = *self.below_since.get_or_insert(now);
                if now.duration_since(since).as_secs_f32() >= off_delay {
                    active = false;
                    self.last_on = None;
                    self.below_since = None;
                    event = Some(false);
                }
            } else {
                self.below_since = None;
            }
        }
        if active
            && decay > 0.0
            && self
                .last_on
                .is_some_and(|on| now.duration_since(on).as_secs_f32() >= decay)
        {
            active = false;
            self.last_on = None;
            self.armed = false;
            self.above_since = None;
            event = Some(false);
        }
        (active, event)
    }
}

fn preset_dir() -> std::path::PathBuf {
    let home = if cfg!(windows) {
        std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))
    } else {
        std::env::var_os("HOME")
    };
    home.map_or_else(|| std::path::PathBuf::from("."), std::path::PathBuf::from)
        .join(".beatd")
        .join("presets")
}

fn preset_path(slot: usize) -> std::path::PathBuf {
    preset_dir().join(format!("preset-{slot}.json"))
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Band {
    low: f32,
    high: f32,
    threshold: f32,
    gain: f32,
    #[serde(skip)]
    level: f32,
    #[serde(skip)]
    active: bool,
    address: String,
    #[serde(default)]
    mode: TriggerMode,
    #[serde(default)]
    on_delay: f32,
    #[serde(default)]
    off_delay: f32,
    #[serde(default)]
    decay: f32,
    #[serde(skip)]
    trigger: Trigger,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Preset {
    bands: [Band; 4],
    level_channel: Band,
    gain: f32,
    compressor: f32,
    osc_enabled: bool,
    destination: String,
    osc_port: u16,
    waveform_address: String,
    spectrum_address: String,
    bpm_detect: bool,
    bpm_address: String,
    bpm_range_min: f32,
    show_waveform: bool,
    show_spectrum: bool,
    show_channels: bool,
    show_bands: bool,
    show_channel: [bool; 5],
    device_name: Option<String>,
}

fn valid_preset(preset: &Preset) -> bool {
    preset
        .bands
        .iter()
        .chain(std::iter::once(&preset.level_channel))
        .all(|b| {
            b.low.is_finite()
                && b.high.is_finite()
                && b.low >= 0.0
                && b.low < b.high
                && b.high <= 24000.0
                && (0.000001..=1.0).contains(&b.threshold)
                && (0.1..=20.0).contains(&b.gain)
                && (0.0..=10.0).contains(&b.on_delay)
                && (0.0..=10.0).contains(&b.off_delay)
                && (0.0..=30.0).contains(&b.decay)
                && b.address.starts_with('/')
        })
        && (0.1..=10.0).contains(&preset.gain)
        && (0.2..=3.0).contains(&preset.compressor)
        && preset.osc_port != 0
        && (preset.bpm_range_min == 0.0 || (25.0..=300.0).contains(&preset.bpm_range_min))
        && [
            &preset.waveform_address,
            &preset.spectrum_address,
            &preset.bpm_address,
        ]
        .iter()
        .all(|address| address.is_empty() || address.starts_with('/'))
}

impl Band {
    fn new(i: usize) -> Self {
        Self {
            low: BANDS[i].0,
            high: BANDS[i].1,
            threshold: 0.15,
            gain: 4.0,
            level: 0.0,
            active: false,
            address: format!("/beatd/band/{}", i + 1),
            mode: TriggerMode::Trigger,
            on_delay: 0.0,
            off_delay: 0.0,
            decay: 0.0,
            trigger: Trigger::default(),
        }
    }
}
struct Beatd {
    devices: Vec<cpal::Device>,
    device_names: Vec<String>,
    device: usize,
    stream: Option<cpal::Stream>,
    rx: Option<Receiver<f32>>,
    analyzer: Analyzer,
    bands: [Band; 4],
    level_channel: Band,
    osc_enabled: bool,
    destination: String,
    osc_port: u16,
    socket: Option<UdpSocket>,
    waveform_address: String,
    spectrum_address: String,
    status: String,
    last_beat: Option<Instant>,
    bpm: f32,
    bpm_detect: bool,
    bpm_address: String,
    bpm_beat_age: f32,
    gain: f32,
    compressor: f32,
    show_waveform: bool,
    show_spectrum: bool,
    show_channels: bool,
    show_bands: bool,
    show_channel: [bool; 5],
}
impl Default for Beatd {
    fn default() -> Self {
        let mut app = Self {
            devices: vec![],
            device_names: vec![],
            device: 0,
            stream: None,
            rx: None,
            analyzer: Analyzer::new(44100),
            bands: std::array::from_fn(Band::new),
            level_channel: Band {
                address: "/beatd/level".into(),
                gain: 1.0,
                ..Band::new(0)
            },
            osc_enabled: false,
            destination: "127.0.0.1".into(),
            osc_port: 8000,
            socket: UdpSocket::bind("0.0.0.0:0").ok(),
            waveform_address: "/beatd/waveform".into(),
            spectrum_address: "/beatd/spectrum".into(),
            status: "Select an input and press Start".into(),
            last_beat: None,
            bpm: 0.0,
            bpm_detect: true,
            bpm_address: "/beatd/bpm".into(),
            bpm_beat_age: f32::INFINITY,
            gain: 1.0,
            compressor: 1.0,
            show_waveform: true,
            show_spectrum: true,
            show_channels: true,
            show_bands: true,
            show_channel: [true; 5],
        };
        app.refresh_devices();
        if let Some(name) = cpal::default_host()
            .default_input_device()
            .and_then(|d| d.name().ok())
        {
            if let Some(index) = app.device_names.iter().position(|n| *n == name) {
                app.device = index;
            }
        }
        app.status = match app.start() {
            Ok(()) => "Audio running".into(),
            Err(e) => format!("Audio: {e}"),
        };
        app
    }
}
fn push_samples<T: cpal::Sample>(data: &[T], channels: usize, tx: &SyncSender<f32>)
where
    f32: cpal::FromSample<T>,
{
    for frame in data.chunks_exact(channels) {
        let sample = frame.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / channels as f32;
        if tx.try_send(sample).is_err() {
            break;
        }
    }
}
impl Beatd {
    fn save_settings(&mut self, slot: usize) {
        let path = preset_path(slot);
        let tmp = path.with_extension("json.tmp");
        let preset = Preset {
            bands: self.bands.clone(),
            level_channel: self.level_channel.clone(),
            gain: self.gain,
            compressor: self.compressor,
            osc_enabled: self.osc_enabled,
            destination: self.destination.clone(),
            osc_port: self.osc_port,
            waveform_address: self.waveform_address.clone(),
            spectrum_address: self.spectrum_address.clone(),
            bpm_detect: self.bpm_detect,
            bpm_address: self.bpm_address.clone(),
            bpm_range_min: self.analyzer.bpm_range_min(),
            show_waveform: self.show_waveform,
            show_spectrum: self.show_spectrum,
            show_channels: self.show_channels,
            show_bands: self.show_bands,
            show_channel: self.show_channel,
            device_name: self.device_names.get(self.device).cloned(),
        };
        let result = std::fs::create_dir_all(preset_dir())
            .map_err(|e| e.to_string())
            .and_then(|_| serde_json::to_vec_pretty(&preset).map_err(|e| e.to_string()))
            .and_then(|data| std::fs::write(&tmp, data).map_err(|e| e.to_string()))
            .and_then(|_| {
                if cfg!(windows) {
                    let _ = std::fs::remove_file(&path);
                }
                std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
            });
        self.status = result.map_or_else(|e| e, |_| format!("Saved preset {slot}"));
    }
    fn load_settings(&mut self, slot: usize) {
        let result = std::fs::read(preset_path(slot))
            .map_err(|e| e.to_string())
            .and_then(|data| serde_json::from_slice::<Preset>(&data).map_err(|e| e.to_string()));
        match result {
            Ok(preset) if valid_preset(&preset) => {
                self.apply_preset(preset);
                self.status = format!("Loaded preset {slot}");
            }
            Ok(_) => self.status = "Invalid preset settings".into(),
            Err(e) => self.status = e,
        }
    }
    fn apply_preset(&mut self, preset: Preset) {
        self.release_notes();
        self.bands = preset.bands;
        for band in &mut self.bands {
            band.active = false;
            band.level = 0.0;
            band.trigger.reset();
        }
        self.level_channel = preset.level_channel;
        self.level_channel.active = false;
        self.level_channel.level = 0.0;
        self.level_channel.trigger.reset();
        self.gain = preset.gain;
        self.compressor = preset.compressor;
        self.osc_enabled = preset.osc_enabled;
        self.destination = preset.destination;
        self.osc_port = preset.osc_port;
        self.waveform_address = preset.waveform_address;
        self.spectrum_address = preset.spectrum_address;
        self.bpm_detect = preset.bpm_detect;
        self.bpm_address = preset.bpm_address;
        self.bpm = 0.0;
        self.last_beat = None;
        self.bpm_beat_age = f32::INFINITY;
        self.analyzer.set_bpm_range_min(preset.bpm_range_min);
        self.analyzer.set_bpm_enabled(preset.bpm_detect);
        self.show_waveform = preset.show_waveform;
        self.show_spectrum = preset.show_spectrum;
        self.show_channels = preset.show_channels;
        self.show_bands = preset.show_bands;
        self.show_channel = preset.show_channel;
        if let Some(name) = preset.device_name {
            if let Some(index) = self.device_names.iter().position(|n| *n == name) {
                if index != self.device {
                    self.device = index;
                    if self.stream.is_some() {
                        self.stream = None;
                        self.rx = None;
                        self.status = match self.start() {
                            Ok(()) => "Audio running".into(),
                            Err(e) => format!("Audio: {e}"),
                        };
                    }
                }
            }
        }
    }
    fn reset_settings(&mut self) {
        self.release_notes();
        self.bands = std::array::from_fn(Band::new);
        self.level_channel = Band {
            address: "/beatd/level".into(),
            gain: 1.0,
            ..Band::new(0)
        };
        self.gain = 1.0;
        self.compressor = 1.0;
        self.bpm = 0.0;
        self.bpm_detect = true;
        self.bpm_address = "/beatd/bpm".into();
        self.waveform_address = "/beatd/waveform".into();
        self.spectrum_address = "/beatd/spectrum".into();
        self.last_beat = None;
        self.bpm_beat_age = f32::INFINITY;
        self.analyzer.set_bpm_range_min(0.0);
        self.analyzer.set_bpm_enabled(true);
        self.status = "Reset settings".into();
    }
    fn refresh_devices(&mut self) {
        match cpal::default_host().input_devices() {
            Ok(devices) => {
                self.devices = devices.collect();
                self.device_names = self
                    .devices
                    .iter()
                    .map(|d| d.name().unwrap_or_else(|_| "Unknown input".into()))
                    .collect();
                self.device = self.device.min(self.devices.len().saturating_sub(1));
            }
            Err(e) => self.status = format!("Audio devices: {e}"),
        }
    }
    fn start(&mut self) -> Result<(), String> {
        let device = self
            .devices
            .get(self.device)
            .ok_or("No audio input available")?;
        let config = device.default_input_config().map_err(|e| e.to_string())?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        let (tx, rx) = mpsc::sync_channel(FFT * 8);
        let err = |e| eprintln!("Audio stream error: {e}");
        let stream = match config.sample_format() {
            cpal::SampleFormat::I8 => device.build_input_stream(
                &config.into(),
                move |d: &[i8], _| push_samples(d, channels, &tx),
                err,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config.into(),
                move |d: &[i16], _| push_samples(d, channels, &tx),
                err,
                None,
            ),
            cpal::SampleFormat::I32 => device.build_input_stream(
                &config.into(),
                move |d: &[i32], _| push_samples(d, channels, &tx),
                err,
                None,
            ),
            cpal::SampleFormat::I64 => device.build_input_stream(
                &config.into(),
                move |d: &[i64], _| push_samples(d, channels, &tx),
                err,
                None,
            ),
            cpal::SampleFormat::U8 => device.build_input_stream(
                &config.into(),
                move |d: &[u8], _| push_samples(d, channels, &tx),
                err,
                None,
            ),
            cpal::SampleFormat::U16 => device.build_input_stream(
                &config.into(),
                move |d: &[u16], _| push_samples(d, channels, &tx),
                err,
                None,
            ),
            cpal::SampleFormat::U32 => device.build_input_stream(
                &config.into(),
                move |d: &[u32], _| push_samples(d, channels, &tx),
                err,
                None,
            ),
            cpal::SampleFormat::U64 => device.build_input_stream(
                &config.into(),
                move |d: &[u64], _| push_samples(d, channels, &tx),
                err,
                None,
            ),
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |d: &[f32], _| push_samples(d, channels, &tx),
                err,
                None,
            ),
            cpal::SampleFormat::F64 => device.build_input_stream(
                &config.into(),
                move |d: &[f64], _| push_samples(d, channels, &tx),
                err,
                None,
            ),
            other => return Err(format!("Unsupported audio format: {other:?}")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        let range = self.analyzer.bpm_range_min();
        self.analyzer = Analyzer::new(sample_rate);
        self.analyzer.set_bpm_range_min(range);
        self.analyzer.set_bpm_enabled(self.bpm_detect);
        self.bpm = 0.0;
        self.bpm_beat_age = f32::INFINITY;
        self.rx = Some(rx);
        self.stream = Some(stream);
        Ok(())
    }
    fn release_notes(&mut self) {
        for band in self
            .bands
            .iter_mut()
            .chain(std::iter::once(&mut self.level_channel))
        {
            if band.active {
                if self.osc_enabled && !band.address.trim().is_empty() {
                    let packet = OscPacket::Message(OscMessage {
                        addr: band.address.clone(),
                        args: vec![OscType::Float(0.0)],
                    });
                    if let (Some(socket), Ok(data)) = (&self.socket, rosc::encoder::encode(&packet))
                    {
                        if let Err(e) =
                            socket.send_to(&data, (self.destination.as_str(), self.osc_port))
                        {
                            self.status = format!("OSC: {e}");
                        }
                    }
                }
                band.active = false;
            }
            band.trigger.reset();
        }
    }
    fn trigger_bpm_beat(&mut self) {
        if self.osc_enabled && !self.bpm_address.trim().is_empty() {
            let packet = OscPacket::Message(OscMessage {
                addr: self.bpm_address.clone(),
                args: vec![OscType::Float(self.bpm)],
            });
            if let (Some(socket), Ok(data)) = (&self.socket, rosc::encoder::encode(&packet)) {
                if let Err(e) = socket.send_to(&data, (self.destination.as_str(), self.osc_port)) {
                    self.status = format!("OSC: {e}");
                }
            }
        }
    }
    fn send_osc_float(&mut self, address: &str, value: f32) {
        if !self.osc_enabled || address.trim().is_empty() {
            return;
        }
        let packet = OscPacket::Message(OscMessage {
            addr: address.to_owned(),
            args: vec![OscType::Float(value)],
        });
        if let (Some(socket), Ok(data)) = (&self.socket, rosc::encoder::encode(&packet)) {
            if let Err(e) = socket.send_to(&data, (self.destination.as_str(), self.osc_port)) {
                self.status = format!("OSC: {e}");
            }
        }
    }
    fn send_osc_blob(&mut self, address: &str, data: Vec<u8>) {
        let packet = OscPacket::Message(OscMessage {
            addr: address.to_owned(),
            args: vec![OscType::Blob(data)],
        });
        if let (Some(socket), Ok(data)) = (&self.socket, rosc::encoder::encode(&packet)) {
            if let Err(e) = socket.send_to(&data, (self.destination.as_str(), self.osc_port)) {
                self.status = format!("OSC: {e}");
            }
        }
    }
    fn send_waveform(&mut self) {
        if !self.osc_enabled || self.waveform_address.trim().is_empty() {
            return;
        }
        let data = self
            .analyzer
            .waveform()
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let address = self.waveform_address.clone();
        self.send_osc_blob(&address, data);
    }
    fn send_spectrum(&mut self) {
        if !self.osc_enabled || self.spectrum_address.trim().is_empty() {
            return;
        }
        const BINS: usize = 256;
        let spectrum = self.analyzer.spectrum();
        let nyquist = self.analyzer.sample_rate() as f32 / 2.0;
        let mut data = Vec::with_capacity(BINS * 4);
        for i in 0..BINS {
            let hz = 20.0 * (nyquist / 20.0).powf(i as f32 / (BINS - 1) as f32);
            let index = ((hz / nyquist * spectrum.len() as f32) as usize)
                .min(spectrum.len().saturating_sub(1));
            let value = spectrum.get(index).copied().unwrap_or(0.0);
            data.extend_from_slice(&value.to_le_bytes());
        }
        let address = self.spectrum_address.clone();
        self.send_osc_blob(&address, data);
    }
    fn analyze(&mut self) {
        let Some(rx) = &self.rx else {
            return;
        };
        let samples: Vec<f32> = rx.try_iter().collect();
        self.analyzer.set_spectrum_gain(self.gain);
        self.analyzer.set_spectrum_compression(self.compressor);
        let updated = self.analyzer.push(&samples);
        self.send_waveform();
        if self.bpm_detect
            && self
                .last_beat
                .is_none_or(|last| last.elapsed() > Duration::from_secs(3))
        {
            self.bpm = self.analyzer.bpm();
        }
        let beat_age = if self.bpm_detect && self.bpm > 0.0 {
            self.analyzer
                .beat_offsets(self.bpm)
                .into_iter()
                .reduce(f32::min)
                .unwrap_or(f32::INFINITY)
        } else {
            f32::INFINITY
        };
        if beat_age < self.bpm_beat_age && beat_age < 0.25 {
            self.trigger_bpm_beat();
        }
        self.bpm_beat_age = beat_age;
        if !updated {
            return;
        }
        self.send_spectrum();
        let now = Instant::now();
        let mut events = Vec::new();
        for (i, band) in self
            .bands
            .iter_mut()
            .chain(std::iter::once(&mut self.level_channel))
            .enumerate()
        {
            let level = if i == 4 {
                self.analyzer.level()
            } else {
                self.analyzer.band_level(band.low, band.high)
            };
            band.level = (level * band.gain).clamp(0.0, 1.0);
            if band.mode == TriggerMode::Level {
                band.active = band.level >= band.threshold;
                band.trigger.reset();
                if self.osc_enabled && !band.address.trim().is_empty() {
                    events.push((
                        band.address.clone(),
                        (band.level / band.threshold).clamp(0.0, 1.0),
                    ));
                }
            } else {
                let (active, event) = band.trigger.update(
                    band.active,
                    band.level >= band.threshold,
                    now,
                    band.on_delay,
                    band.off_delay,
                    band.decay,
                );
                band.active = active;
                if let Some(value) = event {
                    if self.osc_enabled && !band.address.trim().is_empty() {
                        events.push((band.address.clone(), if value { 1.0 } else { 0.0 }));
                    }
                }
            }
        }
        for (address, value) in events {
            self.send_osc_float(&address, value);
        }
    }
}
impl Drop for Beatd {
    fn drop(&mut self) {
        self.release_notes();
    }
}
impl eframe::App for Beatd {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.analyze();
        let palette = Palette::new(ctx.style().visuals.dark_mode);
        ctx.request_repaint_after(Duration::from_millis(25));
        egui::SidePanel::left("transport")
            .default_width(300.0)
            .width_range(280.0..=500.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.add_space(12.0);
                let background = ui.painter().add(egui::Shape::Noop);
                let bpm_button = ui
                    .add_sized(
                        [ui.available_width(), 60.0],
                        egui::Button::new(
                            egui::RichText::new(format!("{:.1}", self.bpm)).size(48.0),
                        )
                        .frame(false),
                    )
                    .on_hover_text("Tap tempo");
                if bpm_button.is_pointer_button_down_on() {
                    ui.painter().set(
                        background,
                        egui::Shape::rect_filled(
                            bpm_button.rect,
                            0.0,
                            ui.visuals().widgets.hovered.weak_bg_fill,
                        ),
                    );
                }
                if bpm_button.clicked() {
                    let now = Instant::now();
                    if let Some(last) = self.last_beat {
                        let interval = now.duration_since(last).as_secs_f32();
                        if (0.25..=2.0).contains(&interval) {
                            self.bpm = 60.0 / interval;
                        }
                    }
                    self.last_beat = Some(now);
                }
                egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    ui.make_persistent_id("bpm_detect"),
                    true,
                )
                .show_header(ui, |ui| {
                    ui.label(egui::RichText::new("BPM Detect").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.checkbox(&mut self.bpm_detect, "Enabled").changed() {
                            self.analyzer.set_bpm_enabled(self.bpm_detect);
                            self.bpm = 0.0;
                            self.last_beat = None;
                            self.bpm_beat_age = f32::INFINITY;
                        }
                    });
                })
                .body(|ui| {
                    ui.add_enabled_ui(self.bpm_detect, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("BPM Range").strong());
                            let minimum = self.analyzer.bpm_range_min();
                            let selected_text = if minimum == 0.0 {
                                "Auto".to_owned()
                            } else {
                                format!("{:.0}–{:.0} BPM", minimum, minimum * 2.0)
                            };
                            egui::ComboBox::from_id_salt("bpm_range")
                                .width(ui.available_width())
                                .selected_text(selected_text)
                                .show_ui(ui, |ui| {
                                    for minimum in [0.0, 50.0, 75.0, 100.0, 150.0] {
                                        let mut selected = self.analyzer.bpm_range_min();
                                        let label = if minimum == 0.0 {
                                            "Auto".to_owned()
                                        } else {
                                            format!("{:.0}–{:.0} BPM", minimum, minimum * 2.0)
                                        };
                                        if ui
                                            .selectable_value(&mut selected, minimum, label)
                                            .changed()
                                        {
                                            self.analyzer.set_bpm_range_min(minimum);
                                        }
                                    }
                                });
                        });
                    });
                });
                ui.separator();
                egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    ui.make_persistent_id("gain_compressor"),
                    true,
                )
                .show_header(ui, |ui| {
                    ui.label(egui::RichText::new("Input").strong());
                })
                .body(|ui| {
                    egui::Grid::new("gain_compressor_settings")
                        .min_col_width(72.0)
                        .num_columns(2)
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new("Gain").strong());
                            ui.add_sized(
                                [ui.available_width(), ui.spacing().interact_size.y],
                                egui::DragValue::new(&mut self.gain)
                                    .range(0.1..=10.0)
                                    .speed(0.1)
                                    .suffix("x"),
                            );
                            ui.end_row();
                            ui.label(egui::RichText::new("Compressor").strong());
                            ui.add_sized(
                                [ui.available_width(), ui.spacing().interact_size.y],
                                egui::DragValue::new(&mut self.compressor)
                                    .range(0.2..=3.0)
                                    .speed(0.01)
                                    .max_decimals(2),
                            );
                            ui.end_row();
                        });
                });
                ui.separator();
                egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    ui.make_persistent_id("osc_output"),
                    true,
                )
                .show_header(ui, |ui| {
                    ui.label(egui::RichText::new("OSC Output").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.checkbox(&mut self.osc_enabled, "Enabled");
                    });
                })
                .body(|ui| {
                    ui.add_enabled_ui(self.osc_enabled, |ui| {
                        egui::Grid::new("osc_connection")
                            .min_col_width(72.0)
                            .num_columns(2)
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("Address").strong());
                                ui.add_sized(
                                    [ui.available_width(), ui.spacing().interact_size.y],
                                    egui::TextEdit::singleline(&mut self.destination),
                                );
                                ui.end_row();
                                ui.label(egui::RichText::new("Protocol").strong());
                                egui::ComboBox::from_id_salt("osc_protocol")
                                    .width(ui.available_width())
                                    .selected_text("UDP")
                                    .show_ui(ui, |ui| {
                                        ui.label("UDP");
                                    });
                                ui.end_row();
                                ui.label(egui::RichText::new("Port").strong());
                                ui.add_sized(
                                    [ui.available_width(), ui.spacing().interact_size.y],
                                    egui::DragValue::new(&mut self.osc_port).range(1..=65535),
                                );
                                ui.end_row();
                                ui.label(egui::RichText::new("BPM").strong());
                                ui.add_sized(
                                    [ui.available_width(), ui.spacing().interact_size.y],
                                    egui::TextEdit::singleline(&mut self.bpm_address),
                                );
                                ui.end_row();
                                ui.label(egui::RichText::new("Waveform").strong());
                                ui.add_sized(
                                    [ui.available_width(), ui.spacing().interact_size.y],
                                    egui::TextEdit::singleline(&mut self.waveform_address),
                                );
                                ui.end_row();
                                ui.label(egui::RichText::new("Spectrum").strong());
                                ui.add_sized(
                                    [ui.available_width(), ui.spacing().interact_size.y],
                                    egui::TextEdit::singleline(&mut self.spectrum_address),
                                );
                                ui.end_row();
                            });
                    });
                });
                ui.separator();
                egui::CollapsingHeader::new(egui::RichText::new("View").strong())
                    .default_open(true)
                    .show(ui, |ui| {
                        check_label(ui, &mut self.show_waveform, "Waveform");
                        check_label(ui, &mut self.show_spectrum, "Spectrum");
                        ui.add_enabled_ui(self.show_spectrum, |ui| {
                            ui.indent("spectrum_view", |ui| {
                                check_label(ui, &mut self.show_bands, "Bands");
                            });
                        });
                        check_label(ui, &mut self.show_channels, "Channels");
                        ui.add_enabled_ui(self.show_channels, |ui| {
                            ui.indent("channels_view", |ui| {
                                for (i, label) in ["Bass", "Low Mid", "High Mid", "Treble", "Level"]
                                    .into_iter()
                                    .enumerate()
                                {
                                    check_label(ui, &mut self.show_channel[i], label);
                                }
                            });
                        });
                    });
                ui.separator();
                egui::CollapsingHeader::new(egui::RichText::new("Presets").strong())
                    .default_open(true)
                    .show(ui, |ui| {
                        for slot in 1..=3 {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(format!("Preset {slot}")).strong());
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let path = preset_path(slot);
                                        let exists = path.is_file();
                                        if ui
                                            .add_enabled(exists, egui::Button::new("Delete"))
                                            .clicked()
                                        {
                                            self.status = std::fs::remove_file(&path).map_or_else(
                                                |e| format!("Delete preset {slot}: {e}"),
                                                |_| format!("Deleted preset {slot}"),
                                            );
                                        }
                                        if ui
                                            .add_enabled(exists, egui::Button::new("Load"))
                                            .clicked()
                                        {
                                            self.load_settings(slot);
                                        }
                                        if ui.button("Save").clicked() {
                                            self.save_settings(slot);
                                        }
                                    },
                                );
                            });
                        }
                        if ui
                            .add_sized(
                                [ui.available_width(), ui.spacing().interact_size.y],
                                egui::Button::new("Reset"),
                            )
                            .clicked()
                        {
                            self.reset_settings();
                        }
                    });
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Audio input");
                    let previous_device = self.device;
                    egui::ComboBox::from_id_salt("audio")
                        .selected_text(
                            self.device_names
                                .get(self.device)
                                .map_or("No input", String::as_str),
                        )
                        .show_ui(ui, |ui| {
                            for (i, name) in self.device_names.iter().enumerate() {
                                ui.selectable_value(&mut self.device, i, name);
                            }
                        });
                    if self.device != previous_device && self.stream.is_some() {
                        self.release_notes();
                        self.stream = None;
                        self.rx = None;
                        self.status = match self.start() {
                            Ok(()) => "Audio running".into(),
                            Err(e) => format!("Audio: {e}"),
                        };
                    }
                    if ui.button("Refresh").clicked() {
                        self.refresh_devices();
                    }
                    if self.stream.is_none() {
                        if ui.button("Start").clicked() {
                            self.status = match self.start() {
                                Ok(()) => "Audio running".into(),
                                Err(e) => e,
                            };
                        }
                    } else if ui.button("Stop").clicked() {
                        self.release_notes();
                        self.stream = None;
                        self.rx = None;
                        self.analyzer.reset();
                        self.status = "Audio stopped".into();
                    }
                });
                if self.show_waveform {
                    ui.separator();
                    let (wave_rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 80.0),
                        egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(wave_rect, 0.0, palette.wave_bg);
                    ui.painter().line_segment(
                        [
                            egui::pos2(wave_rect.left(), wave_rect.center().y),
                            egui::pos2(wave_rect.right(), wave_rect.center().y),
                        ],
                        egui::Stroke::new(1.0_f32, palette.wave_center),
                    );
                    let width = wave_rect.width() / 250.0;
                    for (i, peak) in self.analyzer.waveform().iter().rev().enumerate() {
                        let x = wave_rect.right() - i as f32 * width;
                        let h = peak.min(1.0) * wave_rect.height() * 0.48;
                        ui.painter().line_segment(
                            [
                                egui::pos2(x, wave_rect.center().y - h),
                                egui::pos2(x, wave_rect.center().y + h),
                            ],
                            egui::Stroke::new(2.0_f32, palette.wave),
                        );
                    }
                    for age in self.analyzer.beat_offsets(self.bpm) {
                        if age <= 5.0 {
                            let x = wave_rect.right() - age / 5.0 * wave_rect.width();
                            ui.painter().line_segment(
                                [
                                    egui::pos2(x, wave_rect.top()),
                                    egui::pos2(x, wave_rect.bottom()),
                                ],
                                egui::Stroke::new(1.5_f32, palette.beat),
                            );
                        }
                    }
                }
                if self.show_spectrum {
                    ui.separator();
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 240.0),
                        egui::Sense::hover(),
                    );
                    let points: Vec<egui::Pos2> = self
                        .analyzer
                        .spectrum()
                        .iter()
                        .enumerate()
                        .skip(1)
                        .map(|(i, v)| {
                            let hz = i as f32 * self.analyzer.sample_rate() as f32 / FFT as f32;
                            let x = (hz / 20.0).ln()
                                / ((self.analyzer.sample_rate() as f32 / 2.0) / 20.0).ln();
                            let y =
                                ((20.0 * v.max(0.00001).log10() + 100.0) / 100.0).clamp(0.0, 1.0);
                            egui::pos2(
                                rect.left() + x * rect.width(),
                                rect.bottom() - y * rect.height(),
                            )
                        })
                        .collect();
                    ui.painter().add(egui::Shape::line(
                        points,
                        egui::Stroke::new(1.5_f32, palette.spectrum),
                    ));
                    let rate = self.analyzer.sample_rate() as f32;
                    let to_x = |hz: f32| {
                        rect.left()
                            + ((hz.max(20.0) / 20.0).ln() / ((rate / 2.0) / 20.0).ln())
                                .clamp(0.0, 1.0)
                                * rect.width()
                    };
                    if self.show_bands {
                        for (i, band) in self.bands.iter_mut().enumerate() {
                            let left = to_x(band.low);
                            let right = to_x(band.high);
                            let amplitude = band.threshold / band.gain;
                            let height = ((20.0 * amplitude.max(0.00001).log10() + 100.0) / 100.0)
                                .clamp(0.0, 1.0);
                            let top = rect.bottom() - height * rect.height();
                            let bar = egui::Rect::from_min_max(
                                egui::pos2(left, top),
                                egui::pos2(right.max(left + 14.0), rect.bottom()),
                            );
                            ui.painter()
                                .rect_filled(bar, 2.0, palette.band_fill(i, band.active));
                            ui.painter().line_segment(
                                [bar.left_top(), bar.right_top()],
                                egui::Stroke::new(
                                    2.0_f32,
                                    if band.active {
                                        palette.threshold_active
                                    } else {
                                        palette.threshold_inactive
                                    },
                                ),
                            );
                            ui.painter().text(
                                bar.center(),
                                egui::Align2::CENTER_CENTER,
                                ["Bass", "Low Mid", "High Mid", "Treble"][i],
                                egui::FontId::proportional(12.0),
                                egui::Color32::WHITE,
                            );
                            let center = egui::Rect::from_min_max(
                                egui::pos2(bar.left() + 5.0, bar.top().min(rect.bottom() - 12.0)),
                                bar.max - egui::vec2(5.0, 0.0),
                            );
                            let response = ui
                                .interact(center, ui.id().with(("band", i)), egui::Sense::drag())
                                .on_hover_cursor(egui::CursorIcon::Grab);
                            if response.dragged() {
                                let dy = ui.input(|input| input.pointer.delta().y);
                                band.threshold = (band.threshold
                                    * 10.0_f32.powf(-5.0 * dy / rect.height()))
                                .clamp(band.gain * 0.00001, 1.0);
                                let factor = (((rate / 2.0) / 20.0).ln()
                                    * ui.input(|input| input.pointer.delta().x)
                                    / rect.width())
                                .exp();
                                let upper = (rate / 2.0).min(24000.0) / band.high.max(1.0);
                                let lower = (20.0 / band.low.max(20.0)).min(upper);
                                let factor = factor.clamp(lower, upper.max(lower));
                                band.low = (band.low.max(20.0) * factor).max(20.0);
                                band.high *= factor;
                            }
                            let threshold_handle = egui::Rect::from_min_max(
                                egui::pos2(bar.left() + 5.0, (bar.top() - 6.0).max(rect.top())),
                                egui::pos2(bar.right() - 5.0, (bar.top() + 6.0).min(rect.bottom())),
                            );
                            let threshold_response = ui
                                .interact(
                                    threshold_handle,
                                    ui.id().with(("threshold", i)),
                                    egui::Sense::drag(),
                                )
                                .on_hover_cursor(egui::CursorIcon::ResizeVertical);
                            if threshold_response.dragged() {
                                let dy = ui.input(|input| input.pointer.delta().y);
                                band.threshold = (band.threshold
                                    * 10.0_f32.powf(-5.0 * dy / rect.height()))
                                .clamp(band.gain * 0.00001, 1.0);
                            }
                            for edge in [false, true] {
                                let x = if edge { bar.right() } else { bar.left() };
                                let handle = egui::Rect::from_center_size(
                                    egui::pos2(x, bar.center().y),
                                    egui::vec2(10.0, 24.0),
                                );
                                let response = ui
                                    .interact(
                                        handle,
                                        ui.id().with(("band_edge", i, edge)),
                                        egui::Sense::drag(),
                                    )
                                    .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                                if response.dragged() {
                                    if let Some(pointer) = response.interact_pointer_pos() {
                                        let hz = 20.0
                                            * ((rate / 2.0 / 20.0).ln()
                                                * ((pointer.x - rect.left()) / rect.width())
                                                    .clamp(0.0, 1.0))
                                            .exp();
                                        if edge {
                                            band.high = hz.clamp(
                                                (band.low + 1.0).min((rate / 2.0).min(24000.0)),
                                                (rate / 2.0).min(24000.0),
                                            );
                                        } else {
                                            band.low = hz.clamp(
                                                20.0_f32.min(band.high - 1.0),
                                                (band.high - 1.0).max(20.0),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    let (axis, _) = ui
                        .allocate_exact_size(egui::vec2(rect.width(), 22.0), egui::Sense::hover());
                    for hz in [
                        20.0_f32, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0,
                        20000.0,
                    ] {
                        if hz > rate / 2.0 {
                            continue;
                        }
                        let x = to_x(hz);
                        let label = if hz >= 1000.0 {
                            format!("{}k", hz / 1000.0)
                        } else {
                            format!("{hz:.0}")
                        };
                        ui.painter().text(
                            egui::pos2(x, axis.top()),
                            egui::Align2::CENTER_TOP,
                            label,
                            egui::FontId::proportional(11.0),
                            ui.visuals().weak_text_color(),
                        );
                    }
                }
                let visible_channels = self.show_channel.iter().filter(|shown| **shown).count();
                if self.show_channels && visible_channels > 0 {
                    ui.separator();
                    let colors = palette.bands;
                    ui.columns(visible_channels, |columns| {
                        let mut column = 0;
                        for i in 0..4 {
                            if !self.show_channel[i] {
                                continue;
                            }
                            let ui = &mut columns[column];
                            let band = &mut self.bands[i];
                            egui::Frame::new()
                                .fill(palette.band_frame(i))
                                .inner_margin(10.0)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.heading(
                                            egui::RichText::new(
                                                ["Bass", "Low Mid", "High Mid", "Treble"][i],
                                            )
                                            .color(colors[i])
                                            .strong(),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                let (rect, _) = ui.allocate_exact_size(
                                                    egui::vec2(14.0, 14.0),
                                                    egui::Sense::hover(),
                                                );
                                                ui.painter().circle_filled(
                                                    rect.center(),
                                                    6.0,
                                                    if band.active {
                                                        colors[i]
                                                    } else {
                                                        palette.dot_inactive
                                                    },
                                                );
                                            },
                                        );
                                    });
                                    ui.add_space(12.0);
                                    ui.horizontal(|ui| {
                                        let (rect, _) = ui.allocate_exact_size(
                                            egui::vec2(24.0, 160.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(rect, 3.0, palette.meter_bg);
                                        let fill = egui::Rect::from_min_max(
                                            egui::pos2(
                                                rect.left(),
                                                rect.bottom()
                                                    - rect.height()
                                                        * ((20.0
                                                            * (band.level / band.gain)
                                                                .max(0.00001)
                                                                .log10()
                                                            + 100.0)
                                                            / 100.0)
                                                            .clamp(0.0, 1.0),
                                            ),
                                            rect.right_bottom(),
                                        );
                                        ui.painter().rect_filled(fill, 3.0, colors[i]);
                                        let height = ((20.0
                                            * (band.threshold / band.gain).max(0.00001).log10()
                                            + 100.0)
                                            / 100.0)
                                            .clamp(0.0, 1.0);
                                        let y = rect.bottom() - rect.height() * height;
                                        ui.painter().line_segment(
                                            [
                                                egui::pos2(rect.left() - 5.0, y),
                                                egui::pos2(rect.right() + 5.0, y),
                                            ],
                                            egui::Stroke::new(
                                                2.0_f32,
                                                if band.active {
                                                    palette.threshold_active
                                                } else {
                                                    palette.threshold_inactive
                                                },
                                            ),
                                        );
                                        let handle = egui::Rect::from_center_size(
                                            egui::pos2(rect.center().x, y),
                                            egui::vec2(rect.width() + 10.0, 14.0),
                                        );
                                        let response = ui
                                            .interact(
                                                handle,
                                                ui.id().with(("meter_threshold", i)),
                                                egui::Sense::drag(),
                                            )
                                            .on_hover_cursor(egui::CursorIcon::ResizeVertical);
                                        if response.dragged() {
                                            let dy = ui.input(|input| input.pointer.delta().y);
                                            band.threshold = (band.threshold
                                                * 10.0_f32.powf(-5.0 * dy / rect.height()))
                                            .clamp(band.gain * 0.00001, 1.0);
                                        }
                                    });
                                    ui.add_space(10.0);
                                    ui.label(egui::RichText::new("Parameters").strong());
                                    let input_width = (ui.available_width() - 80.0).max(40.0);
                                    egui::Grid::new(("settings", i))
                                        .min_col_width(72.0)
                                        .num_columns(2)
                                        .show(ui, |ui| {
                                            ui.visuals_mut().extreme_bg_color = palette.widget_bg;
                                            ui.visuals_mut().widgets.inactive.bg_fill =
                                                palette.widget_bg;
                                            ui.visuals_mut().widgets.inactive.weak_bg_fill =
                                                palette.widget_bg;
                                            ui.visuals_mut().widgets.hovered.weak_bg_fill =
                                                palette.widget_bg;
                                            ui.visuals_mut().widgets.active.weak_bg_fill =
                                                palette.widget_bg;
                                            ui.visuals_mut().widgets.noninteractive.weak_bg_fill =
                                                palette.widget_bg;
                                            field_label(ui, "Low");
                                            ui.add_sized(
                                                [input_width, ui.spacing().interact_size.y],
                                                egui::DragValue::new(&mut band.low)
                                                    .range(0.0..=20000.0),
                                            );
                                            ui.end_row();
                                            field_label(ui, "High");
                                            ui.add_sized(
                                                [input_width, ui.spacing().interact_size.y],
                                                egui::DragValue::new(&mut band.high)
                                                    .range(band.low + 1.0..=24000.0),
                                            );
                                            ui.end_row();
                                            field_label(ui, "Gain");
                                            ui.add_sized(
                                                [input_width, ui.spacing().interact_size.y],
                                                egui::DragValue::new(&mut band.gain)
                                                    .range(0.1..=20.0)
                                                    .speed(0.1),
                                            );
                                            ui.end_row();
                                            field_label(ui, "On delay");
                                            ui.add_sized(
                                                [input_width, ui.spacing().interact_size.y],
                                                egui::DragValue::new(&mut band.on_delay)
                                                    .range(0.0..=10.0)
                                                    .speed(0.01)
                                                    .max_decimals(2)
                                                    .suffix(" s"),
                                            );
                                            ui.end_row();
                                            field_label(ui, "Off delay");
                                            ui.add_sized(
                                                [input_width, ui.spacing().interact_size.y],
                                                egui::DragValue::new(&mut band.off_delay)
                                                    .range(0.0..=10.0)
                                                    .speed(0.01)
                                                    .max_decimals(2)
                                                    .suffix(" s"),
                                            );
                                            ui.end_row();
                                            field_label(ui, "Decay");
                                            ui.add_sized(
                                                [input_width, ui.spacing().interact_size.y],
                                                egui::DragValue::new(&mut band.decay)
                                                    .range(0.0..=30.0)
                                                    .speed(0.1)
                                                    .max_decimals(1)
                                                    .suffix(" s"),
                                            );
                                            ui.end_row();
                                            ui.allocate_space(egui::vec2(1.0, 4.0));
                                            ui.end_row();
                                            field_label(ui, "Output");
                                            ui.end_row();
                                            field_label(ui, "Mode");
                                            mode_combo(
                                                ui,
                                                ("mode", i),
                                                &mut band.mode,
                                                input_width,
                                            );
                                            ui.end_row();
                                            field_label(ui, "OSC");
                                            let previous = band.address.clone();
                                            if ui
                                                .add(
                                                    egui::TextEdit::singleline(&mut band.address)
                                                        .desired_width(input_width),
                                                )
                                                .changed()
                                                && band.active
                                                && self.osc_enabled
                                                && !previous.trim().is_empty()
                                            {
                                                if let (Some(socket), Ok(data)) = (
                                                    &self.socket,
                                                    rosc::encoder::encode(&OscPacket::Message(
                                                        OscMessage {
                                                            addr: previous,
                                                            args: vec![OscType::Float(0.0)],
                                                        },
                                                    )),
                                                ) {
                                                    if let Err(e) = socket.send_to(
                                                        &data,
                                                        (self.destination.as_str(), self.osc_port),
                                                    ) {
                                                        self.status = format!("OSC: {e}");
                                                    }
                                                }
                                            }
                                            ui.end_row();
                                        });
                                });
                            column += 1;
                        }
                        if self.show_channel[4] {
                            egui::Frame::new()
                                .fill(palette.frame)
                                .inner_margin(10.0)
                                .show(&mut columns[column], |ui| {
                                    ui.horizontal(|ui| {
                                        ui.heading(
                                            egui::RichText::new("Level")
                                                .color(palette.level)
                                                .strong(),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                let (rect, _) = ui.allocate_exact_size(
                                                    egui::vec2(14.0, 14.0),
                                                    egui::Sense::hover(),
                                                );
                                                ui.painter().circle_filled(
                                                    rect.center(),
                                                    6.0,
                                                    if self.level_channel.active {
                                                        palette.level
                                                    } else {
                                                        palette.dot_inactive
                                                    },
                                                );
                                            },
                                        );
                                    });
                                    ui.add_space(12.0);
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(24.0, 160.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_filled(rect, 3.0, palette.meter_bg);
                                    let height = ((20.0
                                        * self.analyzer.level().max(0.00001).log10()
                                        + 100.0)
                                        / 100.0)
                                        .clamp(0.0, 1.0);
                                    let fill = egui::Rect::from_min_max(
                                        egui::pos2(
                                            rect.left(),
                                            rect.bottom() - rect.height() * height,
                                        ),
                                        rect.right_bottom(),
                                    );
                                    ui.painter().rect_filled(fill, 3.0, palette.level);
                                    let threshold = &mut self.level_channel.threshold;
                                    let y = rect.bottom()
                                        - rect.height()
                                            * ((20.0 * threshold.max(0.00001).log10() + 100.0)
                                                / 100.0)
                                                .clamp(0.0, 1.0);
                                    ui.painter().line_segment(
                                        [
                                            egui::pos2(rect.left() - 5.0, y),
                                            egui::pos2(rect.right() + 5.0, y),
                                        ],
                                        egui::Stroke::new(
                                            2.0_f32,
                                            if self.level_channel.active {
                                                palette.threshold_active
                                            } else {
                                                palette.threshold_inactive
                                            },
                                        ),
                                    );
                                    let handle = egui::Rect::from_center_size(
                                        egui::pos2(rect.center().x, y),
                                        egui::vec2(34.0, 14.0),
                                    );
                                    if ui
                                        .interact(
                                            handle,
                                            ui.id().with("level_threshold"),
                                            egui::Sense::drag(),
                                        )
                                        .on_hover_cursor(egui::CursorIcon::ResizeVertical)
                                        .dragged()
                                    {
                                        *threshold = (*threshold
                                            * 10.0_f32.powf(
                                                -5.0 * ui.input(|input| input.pointer.delta().y)
                                                    / rect.height(),
                                            ))
                                        .clamp(0.00001, 1.0);
                                    }
                                    ui.add_space(10.0);
                                    ui.label(egui::RichText::new("Parameters").strong());
                                    let input_width = (ui.available_width() - 80.0).max(40.0);
                                    egui::Grid::new("level_settings")
                                        .min_col_width(72.0)
                                        .num_columns(2)
                                        .show(ui, |ui| {
                                            ui.visuals_mut().extreme_bg_color = palette.widget_bg;
                                            ui.visuals_mut().widgets.inactive.bg_fill =
                                                palette.widget_bg;
                                            ui.visuals_mut().widgets.inactive.weak_bg_fill =
                                                palette.widget_bg;
                                            ui.visuals_mut().widgets.hovered.weak_bg_fill =
                                                palette.widget_bg;
                                            ui.visuals_mut().widgets.active.weak_bg_fill =
                                                palette.widget_bg;
                                            ui.visuals_mut().widgets.noninteractive.weak_bg_fill =
                                                palette.widget_bg;
                                            field_label(ui, "On delay");
                                            ui.add_sized(
                                                [input_width, ui.spacing().interact_size.y],
                                                egui::DragValue::new(
                                                    &mut self.level_channel.on_delay,
                                                )
                                                .range(0.0..=10.0)
                                                .speed(0.01)
                                                .max_decimals(2)
                                                .suffix(" s"),
                                            );
                                            ui.end_row();
                                            field_label(ui, "Off delay");
                                            ui.add_sized(
                                                [input_width, ui.spacing().interact_size.y],
                                                egui::DragValue::new(
                                                    &mut self.level_channel.off_delay,
                                                )
                                                .range(0.0..=10.0)
                                                .speed(0.01)
                                                .max_decimals(2)
                                                .suffix(" s"),
                                            );
                                            ui.end_row();
                                            field_label(ui, "Decay");
                                            ui.add_sized(
                                                [input_width, ui.spacing().interact_size.y],
                                                egui::DragValue::new(&mut self.level_channel.decay)
                                                    .range(0.0..=30.0)
                                                    .speed(0.1)
                                                    .max_decimals(1)
                                                    .suffix(" s"),
                                            );
                                            ui.end_row();
                                            for _ in 0..3 {
                                                ui.label(" ");
                                                ui.end_row();
                                            }
                                            ui.allocate_space(egui::vec2(1.0, 4.0));
                                            ui.end_row();
                                            field_label(ui, "Output");
                                            ui.end_row();
                                            field_label(ui, "Mode");
                                            mode_combo(
                                                ui,
                                                "mode_level",
                                                &mut self.level_channel.mode,
                                                input_width,
                                            );
                                            ui.end_row();
                                            field_label(ui, "OSC");
                                            let previous = self.level_channel.address.clone();
                                            if ui
                                                .add(
                                                    egui::TextEdit::singleline(
                                                        &mut self.level_channel.address,
                                                    )
                                                    .desired_width(input_width),
                                                )
                                                .changed()
                                                && self.level_channel.active
                                                && self.osc_enabled
                                                && !previous.trim().is_empty()
                                            {
                                                if let (Some(socket), Ok(data)) = (
                                                    &self.socket,
                                                    rosc::encoder::encode(&OscPacket::Message(
                                                        OscMessage {
                                                            addr: previous,
                                                            args: vec![OscType::Float(0.0)],
                                                        },
                                                    )),
                                                ) {
                                                    if let Err(e) = socket.send_to(
                                                        &data,
                                                        (self.destination.as_str(), self.osc_port),
                                                    ) {
                                                        self.status = format!("OSC: {e}");
                                                    }
                                                }
                                            }
                                            ui.end_row();
                                        });
                                });
                        }
                    });
                }
            });
        });
    }
}

pub fn run() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1400.0, 900.0])
        .with_min_inner_size([1100.0, 700.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../../icon.png")) {
        viewport = viewport.with_icon(icon);
    }
    eframe::run_native(
        "beatd",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(|_| Ok(Box::<Beatd>::default())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn preset_round_trip_keeps_all_settings() {
        let preset = Preset {
            bands: std::array::from_fn(Band::new),
            level_channel: Band {
                address: "/beatd/level".into(),
                gain: 1.0,
                ..Band::new(0)
            },
            gain: 2.0,
            compressor: 1.5,
            osc_enabled: true,
            destination: "192.168.0.10".into(),
            osc_port: 9000,
            waveform_address: "/beatd/waveform".into(),
            spectrum_address: "/beatd/spectrum".into(),
            bpm_detect: false,
            bpm_address: "/beatd/bpm".into(),
            bpm_range_min: 75.0,
            show_waveform: false,
            show_spectrum: true,
            show_channels: false,
            show_bands: false,
            show_channel: [true, false, true, false, true],
            device_name: Some("Test input".into()),
        };
        let value = serde_json::to_value(&preset).unwrap();
        assert!(value["bands"][0].get("level").is_none());
        assert!(value["bands"][0].get("active").is_none());
        let restored: Preset = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(value, serde_json::to_value(&restored).unwrap());
        assert!(valid_preset(&restored));
        let mut invalid = restored;
        invalid.gain = 0.0;
        assert!(!valid_preset(&invalid));
    }

    #[test]
    fn trigger_applies_on_and_off_delays() {
        let start = Instant::now();
        let mut trigger = Trigger::default();
        let mut step = |active, above, ms| {
            trigger.update(
                active,
                above,
                start + Duration::from_millis(ms),
                0.1,
                0.2,
                0.0,
            )
        };
        assert_eq!(step(false, true, 0), (false, None));
        assert_eq!(step(false, true, 50), (false, None));
        assert_eq!(step(false, true, 100), (true, Some(true)));
        assert_eq!(step(true, false, 150), (true, None));
        assert_eq!(step(true, false, 350), (false, Some(false)));
    }

    #[test]
    fn decay_requires_a_fall_before_retriggering() {
        let start = Instant::now();
        let mut trigger = Trigger::default();
        assert_eq!(
            trigger.update(false, true, start, 0.0, 0.0, 1.0),
            (true, Some(true))
        );
        assert_eq!(
            trigger.update(true, true, start + Duration::from_secs(1), 0.0, 0.0, 1.0),
            (false, Some(false))
        );
        assert_eq!(
            trigger.update(false, true, start + Duration::from_secs(2), 0.0, 0.0, 1.0),
            (false, None)
        );
        assert_eq!(
            trigger.update(false, false, start + Duration::from_secs(3), 0.0, 0.0, 1.0),
            (false, None)
        );
        assert_eq!(
            trigger.update(false, true, start + Duration::from_secs(4), 0.0, 0.0, 1.0),
            (true, Some(true))
        );
    }
}
