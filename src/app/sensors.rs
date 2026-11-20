//! Optional hardware readings published by LibreHardwareMonitor on Windows.
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::mpsc::{self, Receiver, SyncSender},
};
use wmi::{COMLibrary, WMIConnection};

pub struct Worker {
    poll: SyncSender<()>,
    result: Receiver<HashMap<String, String>>,
    pub latest: HashMap<String, String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Sensor {
    identifier: String,
    name: String,
    sensor_type: String,
    value: f32,
}

impl Worker {
    pub fn start() -> Self {
        let (poll, rx) = mpsc::sync_channel(1);
        let (tx, result) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let Ok(com) = COMLibrary::new() else { return };
            let nvidia = std::env::var_os("SystemRoot")
                .map(std::path::PathBuf::from)
                .map(|p| p.join("System32/nvml.dll"))
                .filter(|p| p.is_file())
                .and_then(|p| {
                    nvml_wrapper::Nvml::builder()
                        .lib_path(p.as_os_str())
                        .init()
                        .ok()
                });
            let native = WMIConnection::new(com.into()).ok();
            let mut connection = None;
            while rx.recv().is_ok() {
                if connection.is_none() {
                    connection = WMIConnection::with_namespace_path(
                        "ROOT\\LibreHardwareMonitor",
                        com.into(),
                    )
                    .ok();
                }
                let mut values = HashMap::<String, (bool, f32, &str)>::new();
                if let Some(c) = &connection {
                    let rows: wmi::WMIResult<Vec<Sensor>> =
                        c.raw_query("SELECT Identifier,Name,SensorType,Value FROM Sensor");
                    match rows {
                        Ok(rows) => {
                            for s in rows {
                                let device = if s.identifier.starts_with("/gpu-") {
                                    "GPU"
                                } else if s.identifier.contains("cpu/") {
                                    "CPU"
                                } else if s.identifier.starts_with("/ram")
                                    || s.identifier.starts_with("/dimm")
                                {
                                    "Memory"
                                } else {
                                    continue;
                                };
                                let (metric, unit) = match s.sensor_type.as_str() {
                                    "Temperature" => ("Temperature", "°C"),
                                    "Clock" => ("Frequency", "MHz"),
                                    "Power" => ("Power", "W"),
                                    "Load" if device == "GPU" => ("Usage", "%"),
                                    _ => continue,
                                };
                                if !s.value.is_finite() {
                                    continue;
                                }
                                let label = s.name.to_ascii_lowercase();
                                if metric == "Frequency"
                                    && device == "GPU"
                                    && !label.contains("core")
                                {
                                    continue;
                                }
                                let preferred = label.contains("package")
                                    || label == "gpu core"
                                    || label.contains("board power");
                                let slot = values
                                    .entry(format!("{device} {metric}"))
                                    .or_insert((preferred, s.value, unit));
                                if (preferred, s.value) > (slot.0, slot.1) {
                                    *slot = (preferred, s.value, unit);
                                }
                            }
                        }
                        Err(_) => connection = None,
                    }
                }
                let mut readings: HashMap<String, String> = values
                    .into_iter()
                    .map(|(k, (_, n, u))| (k, format!("{n:.0} {u}")))
                    .collect();
                if let Some(nv) = &nvidia {
                    use nvml_wrapper::enum_wrappers::device::{Clock, TemperatureSensor};
                    if let Ok(gpu) = nv.device_by_index(0) {
                        if let Ok(n) = gpu.temperature(TemperatureSensor::Gpu) {
                            readings
                                .entry("GPU Temperature".into())
                                .or_insert(format!("{n} °C"));
                        }
                        if let Ok(n) = gpu.power_usage() {
                            readings
                                .entry("GPU Power".into())
                                .or_insert(format!("{:.1} W", n as f32 / 1000.));
                        }
                        if let Ok(n) = gpu.clock_info(Clock::Graphics) {
                            readings
                                .entry("GPU Frequency".into())
                                .or_insert(format!("{n} MHz"));
                        }
                        if let Ok(rate) = gpu.utilization_rates() {
                            readings
                                .entry("GPU Usage".into())
                                .or_insert(format!("{} %", rate.gpu));
                        }
                    }
                }
                if let Some(native) = &native {
                    native_readings(native, &mut readings);
                }
                if tx.send(readings).is_err() {
                    break;
                }
            }
        });
        Self {
            poll,
            result,
            latest: HashMap::new(),
        }
    }
    pub fn sample(&mut self) {
        if let Ok(v) = self.result.try_recv() {
            self.latest = v;
        }
        let _ = self.poll.try_send(());
    }
}

fn native_readings(db: &WMIConnection, out: &mut HashMap<String, String>) {
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Engine {
        name: String,
        utilization_percentage: u64,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Cpu {
        current_clock_speed: u32,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Memory {
        configured_clock_speed: u32,
    }
    if !out.contains_key("GPU Usage") {
        if let Ok(rows)=db.raw_query::<Engine>("SELECT Name,UtilizationPercentage FROM Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine") {
            let mut engines=HashMap::<String,u64>::new();
            for row in rows {
                if let Some((_,key))=row.name.split_once("_luid_"){
                    *engines.entry(key.into()).or_default()+=row.utilization_percentage;
                }
            }
            if let Some(n)=engines.values().max(){out.insert("GPU Usage".into(),format!("{} %",(*n).min(100)));}
        }
    }
    if !out.contains_key("CPU Frequency") {
        if let Ok(rows) = db.raw_query::<Cpu>("SELECT CurrentClockSpeed FROM Win32_Processor") {
            if let Some(n) = rows
                .iter()
                .map(|r| r.current_clock_speed)
                .max()
                .filter(|v| *v > 0)
            {
                out.insert("CPU Frequency".into(), format!("{n} MHz"));
            }
        }
    }
    if !out.contains_key("Memory Frequency") {
        if let Ok(rows) =
            db.raw_query::<Memory>("SELECT ConfiguredClockSpeed FROM Win32_PhysicalMemory")
        {
            if let Some(n) = rows
                .iter()
                .map(|r| r.configured_clock_speed)
                .max()
                .filter(|v| *v > 0)
            {
                out.insert("Memory Frequency".into(), format!("{n} MHz"));
            }
        }
    }
}
