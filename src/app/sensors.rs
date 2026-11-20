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
                let readings = values
                    .into_iter()
                    .map(|(k, (_, n, u))| (k, format!("{n:.0} {u}")))
                    .collect();
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
