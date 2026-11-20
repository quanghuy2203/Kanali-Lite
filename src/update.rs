//! Tự cập nhật từ GitHub Releases, có kiểm tra SHA-256 trước khi thay chương trình.
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
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Clone, Debug)]
pub struct Found {
    pub latest: String,
    pub notes: String,
    pub url: String,
    pub download: String,
    pub sha_url: String,
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

fn allowed(url: &str) -> bool {
    url.starts_with("https://github.com/")
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
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let resp = match agent().get(&url).set("Accept", "application/vnd.github+json").call() {
        Ok(r) => r,
        Err(ureq::Error::Status(404, _)) => return Err("Kho chưa có bản phát hành nào".into()),
        Err(e) => return Err(format!("Không kết nối được GitHub: {e}")),
    };
    let rel: Release = resp.into_json().map_err(|e| format!("Phản hồi GitHub lỗi: {e}"))?;
    if rel.draft || rel.prerelease {
        return Ok(None);
    }
    let (Some(cur), Some(lat)) = (parse_ver(current), parse_ver(&rel.tag_name)) else {
        return Err(format!("Không đọc được số phiên bản: {}", rel.tag_name));
    };
    if lat <= cur {
        return Ok(None);
    }
    let sha_name = format!("{ASSET_NAME}.sha256");
    let exe = rel
        .assets
        .iter()
        .find(|a| a.name == ASSET_NAME)
        .ok_or("Bản phát hành thiếu file kanali-windows-x64.exe")?;
    let sha = rel
        .assets
        .iter()
        .find(|a| a.name == sha_name)
        .ok_or("Bản phát hành thiếu file .sha256")?;
    if !allowed(&exe.browser_download_url) || !allowed(&sha.browser_download_url) {
        return Err("Địa chỉ tải không hợp lệ".into());
    }
    Ok(Some(Found {
        latest: rel.tag_name.clone(),
        notes: rel.body.clone(),
        url: rel.html_url.clone(),
        download: exe.browser_download_url.clone(),
        sha_url: sha.browser_download_url.clone(),
    }))
}

/// Tải bản mới, kiểm tra SHA-256, rồi thay file đang chạy (đổi tên cũ thành .old).
/// Trả về Ok khi đã thay xong; người gọi phải tự khởi động lại chương trình.
pub fn download_and_replace(f: &Found, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
    let ag = agent();
    let sha_text = ag
        .get(&f.sha_url)
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    let expected = sha_text.split_whitespace().next().unwrap_or("").to_lowercase();
    if expected.len() != 64 || !expected.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("File .sha256 không hợp lệ".into());
    }

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let new = exe.with_extension("exe.new");
    let old = exe.with_extension("exe.old");

    let resp = ag.get(&f.download).call().map_err(|e| e.to_string())?;
    let total: u64 = resp.header("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut reader = resp.into_reader();
    let mut out = File::create(&new)
        .map_err(|e| format!("Không ghi được {}: {e}", new.display()))?;

    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    let mut first = [0u8; 2];
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("Tải lỗi: {e}"))?;
        if n == 0 {
            break;
        }
        if done < 2 {
            let start = done as usize;
            let count = n.min(2 - start);
            first[start..start + count].copy_from_slice(&buf[..count]);
        }
        hasher.update(&buf[..n]);
        out.write_all(&buf[..n]).map_err(|e| format!("Ghi lỗi: {e}"))?;
        done += n as u64;
        if total > 0 {
            progress((done as f32 / total as f32).min(1.0));
        }
    }
    drop(out);

    let got: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if got != expected {
        let _ = fs::remove_file(&new);
        return Err("Sai mã SHA-256, đã hủy cập nhật".into());
    }
    if first != *b"MZ" || done < 500_000 {
        let _ = fs::remove_file(&new);
        return Err("File tải về không phải chương trình Windows hợp lệ".into());
    }

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
