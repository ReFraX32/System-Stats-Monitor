use nvml_wrapper::Nvml;
use nvml_wrapper::error::NvmlError;
use nvml_wrapper::enum_wrappers::device::{TemperatureSensor, Clock};
use sysinfo::System;
use eframe::{egui, Frame};
use eframe::egui::{Visuals, Color32, Rounding};
use global_hotkey::{
    GlobalHotKeyManager,
    GlobalHotKeyEvent,
    hotkey::{HotKey, Code, Modifiers},
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct GpuStats {
    name: String,
    used_memory: f32,
    total_memory: f32,
    used_memory_percentage: i8,
    temperature: u32,
    core_clock: u32,
    memory_clock: u32,
    fan_speed: u32,
    power_usage: f64,
}

struct CpuStats {
    total_usage: i8,
    temperature: Option<f32>,
    _fan_speed: Option<u32>,
}

struct FpsCounter {
    frames: u32,
    last_fps_update: Instant,
    current_fps: u32,
}

impl FpsCounter {
    fn new() -> Self {
        Self {
            frames: 0,
            last_fps_update: Instant::now(),
            current_fps: 0,
        }
    }

    fn update(&mut self) {
        self.frames += 1;
        if self.last_fps_update.elapsed() >= Duration::from_secs(1) {
            self.current_fps = self.frames;
            self.frames = 0;
            self.last_fps_update = Instant::now();
        }
    }
}

struct StatsApp {
    gpu_stats: GpuStats,
    cpu_stats: CpuStats,
    system: Arc<Mutex<System>>,
    nvml: Arc<Nvml>,
    last_refresh: Instant,
    fps_counter: FpsCounter,
    visible: bool,
    _hotkey_manager: GlobalHotKeyManager,
    custom_hotkey: HotKey,
}

impl StatsApp {
    fn new(nvml: Nvml) -> Self {
        let system = System::new_all();
        let system = Arc::new(Mutex::new(system));

        let _hotkey_manager = GlobalHotKeyManager::new().expect("Failed to initialize hotkey manager");
        let custom_hotkey = HotKey::new(Some(Modifiers::ALT), Code::KeyT);
        _hotkey_manager.register(custom_hotkey).expect("Failed to register hotkey");

        Self {
            gpu_stats: GpuStats {
                name: String::new(),
                used_memory: 0.0,
                total_memory: 0.0,
                used_memory_percentage: 0,
                temperature: 0,
                core_clock: 0,
                memory_clock: 0,
                fan_speed: 0,
                power_usage: 0.0,
            },
            cpu_stats: CpuStats {
                total_usage: 0,
                temperature: None,
                _fan_speed: None,
            },
            system,
            nvml: Arc::new(nvml),
            last_refresh: Instant::now(),
            fps_counter: FpsCounter::new(),
            visible: true,
            _hotkey_manager,
            custom_hotkey,
        }
    }

    fn refresh_stats(&mut self) {
        let nvml = Arc::clone(&self.nvml);
        let system = Arc::clone(&self.system);
        
        let (gpu_stats, cpu_stats) = {
            let mut system = system.lock().unwrap();
            statscheck(&mut *system, &nvml).unwrap()
        };

        self.gpu_stats = gpu_stats;
        self.cpu_stats = cpu_stats;
    }
}

fn statscheck(system: &mut System, nvml: &Nvml) -> Result<(GpuStats, CpuStats), NvmlError> {
    let device = nvml.device_by_index(0)?;
    let memory_info = device.memory_info()?;

    let gpu_stats = GpuStats {
        name: device.name()?,
        used_memory: bytes_to_gigabytes(memory_info.used),
        total_memory: bytes_to_gigabytes(memory_info.total),
        used_memory_percentage: ((memory_info.used as f32 / memory_info.total as f32) * 100.0).round() as i8,
        temperature: device.temperature(TemperatureSensor::Gpu)?,
        core_clock: device.clock_info(Clock::Graphics)?,
        memory_clock: device.clock_info(Clock::Memory)?,
        fan_speed: device.fan_speed(0)?,
        power_usage: device.power_usage()? as f64 / 1000.0,
    };

    system.refresh_all();

    let cpu_stats = CpuStats {
        total_usage: system.global_cpu_usage() as i8,
        temperature: Some(0.0), // Placeholder for CPU temperature
        _fan_speed: None,
    };

    Ok((gpu_stats, cpu_stats))
}

fn bytes_to_gigabytes(bytes: u64) -> f32 {
    bytes as f32 / 1024000000.0
}

impl eframe::App for StatsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        ctx.set_visuals(Visuals::dark());
        
        if self.visible {
            self.fps_counter.update();
        }

        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.id == self.custom_hotkey.id() {
                self.visible = !self.visible;
            }
        }

        if !self.visible {
            ctx.request_repaint_after(Duration::from_secs(1));
            return;
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none()
                .fill(Color32::from_rgba_premultiplied(0, 0, 0, 200))
                .rounding(Rounding::same(6.0)))
            .show(ctx, |ui| {
                ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);
                
                ui.horizontal(|ui| {
                    ui.label("Performance Overlay");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format!("{} FPS", self.fps_counter.current_fps));
                    });
                });

                ui.separator();

                ui.label(format!("GPU: {}", self.gpu_stats.name));
                ui.horizontal(|ui| {
                    ui.label("VRAM Usage:");
                    ui.label(
                        format!("{:.1}/{:.1} GB ({}%)", 
                            self.gpu_stats.used_memory,
                            self.gpu_stats.total_memory,
                            self.gpu_stats.used_memory_percentage
                        )
                    );
                });
                
                ui.horizontal(|ui| {
                    ui.label("Temperature:");
                    ui.label(format!("{}°C", self.gpu_stats.temperature));
                });

                ui.horizontal(|ui| {
                    ui.label("Core Clock:");
                    ui.label(format!("{} MHz", self.gpu_stats.core_clock));
                });

                ui.horizontal(|ui| {
                    ui.label("Memory Clock:");
                    ui.label(format!("{} MHz", self.gpu_stats.memory_clock));
                });

                ui.horizontal(|ui| {
                    ui.label("Fan Speed:");
                    ui.label(format!("{}%", self.gpu_stats.fan_speed));
                });

                ui.horizontal(|ui| {
                    ui.label("Power Usage:");
                    ui.label(format!("{:.1} W", self.gpu_stats.power_usage));
                });

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("CPU Usage:");
                    ui.label(format!("{}%", self.cpu_stats.total_usage));
                });

                if let Some(temp) = self.cpu_stats.temperature {
                    ui.horizontal(|ui| {
                        ui.label("CPU Temperature:");
                        ui.label(format!("{:.1}°C", temp));
                    });
                }

                ui.add_space(4.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::RIGHT), |ui| {
                    ui.label("[Alt+T] Toggle Overlay");
                });
            });

        if self.last_refresh.elapsed() >= Duration::from_secs(1) {
            self.refresh_stats();
            self.last_refresh = Instant::now();
        }

        if self.visible {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
}

fn main() -> Result<(), eframe::Error> {
    let nvml = match Nvml::init() {
        Ok(nvml) => nvml,
        Err(e) => {
            eprintln!("Failed to initialize NVML: {:?}", e);
            return Ok(());
        }
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Performance Overlay")
            .with_inner_size([250.0, 400.0])
            .with_position([1670.0, 0.0])
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top(),
        ..Default::default()
    };

    eframe::run_native(
        "Performance Overlay",
        options,
        Box::new(|_cc| Ok(Box::new(StatsApp::new(nvml))))
    )
}