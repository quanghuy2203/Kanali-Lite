//! Read the user's installed KANALI preset artwork once and cache small RGB posters.
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
};

const THEMES: [&str; 6] = [
    "Cooling delivery",
    "Migration",
    "Exo-Ecologies",
    "Cyber Bunker",
    "Edge of Dream",
    "Thermal Energy · Prohibited",
];

pub fn title(file: &str) -> Option<&'static str> {
    let index = file
        .split('.')
        .next()?
        .strip_prefix("default_")?
        .parse::<usize>()
        .ok()?
        .checked_sub(1)?;
    THEMES.get(index).copied()
}

pub fn cache() -> Option<()> {
    cache_user_media();
    let path = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .flat_map(|d| {
            [
                d.join("KANALI/resources/app.asar"),
                d.join("Programs/KANALI/resources/app.asar"),
            ]
        })
        .find(|p| p.is_file())?;
    let mut archive = File::open(path).ok()?;
    let mut prefix = [0; 16];
    archive.read_exact(&mut prefix).ok()?;
    let start = 8 + u32::from_le_bytes(prefix[4..8].try_into().ok()?) as u64;
    let len = u32::from_le_bytes(prefix[12..16].try_into().ok()?) as usize;
    if len > 16 * 1024 * 1024 {
        return None;
    }
    let mut index = vec![0; len];
    archive.read_exact(&mut index).ok()?;
    let tree: serde_json::Value = serde_json::from_slice(&index).ok()?;
    let assets = tree
        .pointer("/files/out/files/renderer/files/assets/files")?
        .as_object()?;
    for (i, theme) in THEMES.iter().enumerate() {
        let name = format!("default_{:02}.mp4.h264_2240x1080", i + 1);
        let target = super::media::poster(&name).with_extension("numbered.rgb");
        if target.exists() {
            continue;
        }
        let clean = |s: &str| {
            s.chars()
                .filter(char::is_ascii_alphanumeric)
                .map(|c| c.to_ascii_lowercase())
                .collect::<String>()
        };
        let numbered = format!("preset_{:02}", i + 1);
        let find = |wanted: &str| {
            assets.iter().find(|(n, _)| {
                n.rsplit_once('.').is_some_and(|(base, ext)| {
                    matches!(ext, "png" | "jpg" | "jpeg" | "webp")
                        && clean(base.rsplit_once('-').map_or(base, |p| p.0)) == clean(wanted)
                })
            })
        };
        let Some((_, entry)) = find(&numbered).or_else(|| find(theme)) else {
            continue;
        };
        let offset = entry["offset"].as_str()?.parse::<u64>().ok()?;
        let size = entry["size"].as_u64()?;
        if size > 12 * 1024 * 1024 {
            continue;
        }
        archive
            .seek(SeekFrom::Start(start.checked_add(offset)?))
            .ok()?;
        let mut bytes = vec![0; size as usize];
        archive.read_exact(&mut bytes).ok()?;
        let Ok(img) = image::load_from_memory(&bytes) else {
            continue;
        };
        let frame = img
            .resize_to_fill(456, 320, image::imageops::FilterType::Triangle)
            .into_rgb8();
        let parent = target.parent()?;
        std::fs::create_dir_all(parent).ok()?;
        let mut tmp = tempfile::NamedTempFile::new_in(parent).ok()?;
        tmp.write_all(frame.as_raw()).ok()?;
        let _ = tmp.persist_noclobber(target);
    }
    Some(())
}

fn cache_user_media() {
    let Some(base) = std::env::var_os("APPDATA") else {
        return;
    };
    let root = PathBuf::from(base).join("KANALI");
    let Ok(media) = root.join("media").canonicalize() else {
        return;
    };
    for store in ["store.json", "settings.json"] {
        let path = root.join(store);
        if !path.metadata().is_ok_and(|m| m.len() <= 16 * 1024 * 1024) {
            continue;
        }
        if let Some(json) = kanali_rs::config::load_opt::<serde_json::Value>(&path) {
            cache_entries(&media, &json);
        }
    }
}

fn cache_entries(media: &std::path::Path, node: &serde_json::Value) {
    match node {
        serde_json::Value::Object(obj) => {
            if let (Some(name), Some(thumb)) = (
                obj.get("name").and_then(|v| v.as_str()),
                obj.get("thumb").and_then(|v| v.as_str()),
            ) {
                let thumb = std::path::Path::new(thumb);
                let dest = super::media::poster(&format!("{name}.h264_2240x1080"))
                    .with_extension("numbered.rgb");
                if !dest.exists() {
                    if let Ok(path) = thumb.canonicalize() {
                        if path.starts_with(media)
                            && path.metadata().is_ok_and(|m| m.len() < 12 * 1024 * 1024)
                        {
                            if let Ok(image) = image::open(path) {
                                let rgb = image
                                    .resize_exact(640, 310, image::imageops::FilterType::Triangle)
                                    .into_rgb8();
                                if let Some(parent) = dest.parent() {
                                    let _ = std::fs::create_dir_all(parent);
                                    if let Ok(mut tmp) = tempfile::NamedTempFile::new_in(parent) {
                                        if tmp.write_all(rgb.as_raw()).is_ok() {
                                            let _ = tmp.persist_noclobber(dest);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            for v in obj.values() {
                cache_entries(media, v);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                cache_entries(media, item)
            }
        }
        _ => (),
    }
}
