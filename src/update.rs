//! Cập nhật từ artifact của GitHub Actions, đối chiếu digest trước khi giải nén.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    time::Duration,
};

/// Kho GitHub gắn lúc build (CI đặt biến KANALI_REPO = "owner/repo").
pub const DEFAULT_REPO: &str = match option_env!("KANALI_REPO") {
    Some(r) => r,
    None => "",
};
pub const ASSET_NAME: &str = "kanali-windows-x64.exe";

#[derive(Deserialize)]
struct Runs { workflow_runs: Vec<Run> }
#[derive(Deserialize)]
struct Run { id: u64, html_url: String }
#[derive(Deserialize)]
struct Artifacts { artifacts: Vec<Artifact> }
#[derive(Deserialize)]
struct Artifact { id: u64, name: String, digest: Option<String>, expired: bool }

#[derive(Clone, Debug)]
pub struct Found {
    pub latest: String,
    pub notes: String,
    pub url: String,
    pub download: String,
    digest: String,
}

pub fn parse_ver(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim().trim_start_matches('v');
    let core = s.split(|c| c == '-' || c == '+').next()?;
    let mut it = core.split('.');
    Some((
        it.next()?.parse().ok()?,
        it.next().unwrap_or("0").parse().ok()?,
        it.next().unwrap_or("0").parse().ok()?,
    ))
}

fn valid_repo(r: &str) -> bool {
    r.split('/').count() == 2
        && !r.contains("..")
        && r.chars().all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c))
        && r.split('/').all(|p| !p.is_empty())
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(30))
        .user_agent("kanali-rs")
        .build()
}

/// Ok(None) = đang là bản mới nhất.
pub fn check(repo: &str, current: &str) -> Result<Option<Found>, String> {
    let repo = repo.trim();
    if !valid_repo(repo) {
        return Err("Chưa thiết lập kho GitHub (dạng tên-người-dùng/tên-repo)".into());
    }
    let api = format!("https://api.github.com/repos/{repo}/actions");
    let client = agent();
    let runs: Runs = client.get(&format!("{api}/workflows/build.yml/runs?branch=main&event=push&status=success&per_page=1"))
        .call().map_err(|e| e.to_string())?.into_json().map_err(|e| e.to_string())?;
    let run = runs.workflow_runs.first().ok_or("Chưa có lượt build thành công")?;
    let artifacts: Artifacts = client.get(&format!("{api}/runs/{}/artifacts", run.id))
        .call().map_err(|e| e.to_string())?.into_json().map_err(|e| e.to_string())?;
    let artifact = artifacts.artifacts.iter().find(|a| !a.expired && a.name.starts_with("kanali-windows-x64-v"))
        .ok_or("Artifact không còn khả dụng")?;
    let latest = artifact.name.trim_start_matches("kanali-windows-x64-");
    if parse_ver(latest).ok_or("Phiên bản artifact không hợp lệ")? <= parse_ver(current).ok_or("Phiên bản không hợp lệ")? { return Ok(None); }
    let digest = artifact.digest.as_deref().and_then(|v| v.strip_prefix("sha256:")).ok_or("GitHub không cung cấp SHA-256")?;
    Ok(Some(Found { latest: latest.into(), notes: String::new(), url: run.html_url.clone(),
        download: format!("https://nightly.link/{repo}/actions/artifacts/{}.zip", artifact.id), digest: digest.into() }))
}

pub fn download_and_replace(f: &Found, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let new = exe.with_extension("exe.new");
    let old = exe.with_extension("exe.old");
    let response = agent().get(&f.download).call().map_err(|e| e.to_string())?;
    let total = response.header("Content-Length").and_then(|n| n.parse::<usize>().ok()).unwrap_or(0);
    let mut reader = response.into_reader();
    let mut archive = tempfile::tempfile().map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buf = [0; 65536];
    let mut size = 0;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 { break; }
        size += n;
        if size > 64 * 1024 * 1024 { return Err("Artifact quá lớn".into()); }
        archive.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        hash.update(&buf[..n]);
        if total > 0 { progress((size as f32 / total as f32).min(0.99)); }
    }
    if format!("{:x}", hash.finalize()) != f.digest { return Err("SHA-256 khác với GitHub Actions".into()); }
    let mut zip = zip::ZipArchive::new(archive).map_err(|e| e.to_string())?;
    let mut entry = zip.by_name(ASSET_NAME).map_err(|e| e.to_string())?;
    if !(500_000..=100_000_000).contains(&entry.size()) { return Err("Kích thước chương trình không hợp lệ".into()); }
    let mut out = File::create(&new).map_err(|e| e.to_string())?;
    std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    out.sync_all().map_err(|e| e.to_string())?;
    drop(out);
    let _ = fs::remove_file(&old);
    fs::rename(&exe, &old).map_err(|e| {
        format!("Không thay được chương trình (hãy đặt trong thư mục có quyền ghi): {e}")
    })?;
    if let Err(e) = fs::rename(&new, &exe) {
        let _ = fs::rename(&old, &exe);
        return Err(format!("Không thay được chương trình: {e}"));
    }
    progress(1.0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn so_sanh_phien_ban() {
        assert!(parse_ver("v0.2.1") > parse_ver("0.2.0"));
        assert_eq!(parse_ver("v1.2"), Some((1, 2, 0)));
        assert!(parse_ver("abc").is_none());
    }
    #[test]
    fn kiem_tra_repo() {
        assert!(valid_repo("quanghuy/kanali-rs"));
        assert!(!valid_repo("../x"));
        assert!(!valid_repo("a/b/c"));
        assert!(!valid_repo(""));
    }
}
