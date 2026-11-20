use kanali_rs::{config::Layout, device, proto};
use std::fs::File;
use windows::Win32::{
    Foundation::FILETIME,
    System::{
        SystemInformation::{GetLocalTime, GlobalMemoryStatusEx, MEMORYSTATUSEX},
        Threading::GetSystemTimes,
    },
};

#[derive(Default)]
pub struct Sampler {
    sensors: Option<super::sensors::Worker>,
    ticks: Option<(u64, u64)>,
}

impl Sampler {
    pub fn readings(&mut self, metrics: &[String]) -> std::collections::BTreeMap<String, String> {
        if metrics
            .iter()
            .any(|m| !matches!(m.as_str(), "CPU" | "RAM" | "Date & Time"))
        {
            self.sensors
                .get_or_insert_with(super::sensors::Worker::start)
                .sample();
        } else {
            self.sensors = None;
        }
        let mut values = std::collections::BTreeMap::new();
        for metric in metrics {
            let value = match metric.as_str() {
                "CPU" => {
                    let (mut idle, mut kernel, mut user) = (
                        FILETIME::default(),
                        FILETIME::default(),
                        FILETIME::default(),
                    );
                    let ticks =
                        |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
                    if unsafe {
                        GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user))
                    }
                    .is_err()
                    {
                        continue;
                    }
                    let now = (ticks(idle), ticks(kernel) + ticks(user));
                    let prev = self.ticks.replace(now).unwrap_or(now);
                    let dt = now.1.saturating_sub(prev.1);
                    if dt == 0 {
                        continue;
                    }
                    format!(
                        "CPU  {}%",
                        100 * dt.saturating_sub(now.0.saturating_sub(prev.0)) / dt
                    )
                }
                "RAM" => {
                    let mut mem = MEMORYSTATUSEX {
                        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
                        ..Default::default()
                    };
                    if unsafe { GlobalMemoryStatusEx(&mut mem) }.is_err() {
                        continue;
                    }
                    format!("RAM  {}%", mem.dwMemoryLoad)
                }
                "Date & Time" => {
                    let t = unsafe { GetLocalTime() };
                    format!(
                        "{:02}/{:02}  {:02}:{:02}",
                        t.wDay, t.wMonth, t.wHour, t.wMinute
                    )
                }
                name => match self.sensors.as_ref().and_then(|s| s.latest.get(name)) {
                    Some(value) => format!("{name}  {value}"),
                    None => continue,
                },
            };
            values.insert(metric.clone(), value);
        }
        values
    }

    pub fn publish(&mut self, file: &mut File, layout: &Layout) -> Result<(), String> {
        if layout.metrics.is_empty() {
            self.sensors = None;
            return Ok(());
        }
        let values = self.readings(&layout.metrics);
        let mut labels = Vec::new();
        for (i, key) in layout.metrics.iter().take(3).enumerate() {
            let value = values
                .get(key)
                .cloned()
                .unwrap_or_else(|| format!("{key}  —"));
            labels.push(proto::f_bytes(
                2,
                &[proto::f_varint(1, i as u64 + 1), proto::f_str(2, &value)].concat(),
            ));
        }
        let mut group = proto::f_varint(1, 1);
        group.extend(labels.concat());
        device::send(
            file,
            &[
                proto::f_bytes(1, &[]),
                proto::f_bytes(300, &proto::f_bytes(1, &group)),
            ]
            .concat(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_preview_has_numbers_without_usb() {
        let metrics = [
            "CPU".to_string(),
            "RAM".to_string(),
            "Date & Time".to_string(),
        ];
        let mut s = Sampler::default();
        s.readings(&metrics);
        std::thread::sleep(std::time::Duration::from_millis(150));
        let result = s.readings(&metrics);
        for k in metrics {
            assert!(result[&k].contains(|c: char| c.is_ascii_digit()));
        }
    }
}
