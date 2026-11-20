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
    "Quantum time capsule",
    "Racing",
    "Shuttle",
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
        let target = super::media::poster(&name).with_extension("preset.rgb");
        if target.exists() {
            continue;
        }
        let clean = |s: &str| {
            s.chars()
                .filter(char::is_ascii_alphanumeric)
                .map(|c| c.to_ascii_lowercase())
                .collect::<String>()
        };
        let Some((_, entry)) = assets.iter().find(|(n, _)| {
            n.rsplit_once('.').is_some_and(|(base, ext)| {
                matches!(ext, "png" | "jpg" | "jpeg" | "webp")
                    && clean(base.rsplit_once('-').map_or(base, |p| p.0)) == clean(theme)
            })
        }) else {
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
            .resize_exact(640, 310, image::imageops::FilterType::Triangle)
            .into_rgb8();
        let parent = target.parent()?;
        std::fs::create_dir_all(parent).ok()?;
        let mut tmp = tempfile::NamedTempFile::new_in(parent).ok()?;
        tmp.write_all(frame.as_raw()).ok()?;
        let _ = tmp.persist_noclobber(target);
    }
    Some(())
}
