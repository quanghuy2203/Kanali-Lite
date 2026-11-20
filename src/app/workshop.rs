//! Steam handles authentication, subscription and installation. Only the numeric
//! workshop item identifier crosses from the browser into the media importer.
use kanali_rs::config;
use std::{path::PathBuf, time::Duration};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
};
use wry::{WebContext, WebViewBuilder};

pub fn libraries() -> Result<Vec<PathBuf>, String> {
    let steam = steamlocate::SteamDir::locate().map_err(|_| "Không tìm thấy Steam trên máy")?;
    Ok(steam
        .libraries()
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|lib| lib.path().join("steamapps"))
        .collect())
}

pub fn selected() -> Option<u64> {
    config::load_opt(&config::data_dir().join("workshop-item.json"))
}

pub fn import(id: u64) -> Result<String, String> {
    let folders: Vec<_> = libraries()?
        .into_iter()
        .map(|lib| lib.join("workshop/content/431960").join(id.to_string()))
        .collect();
    for _ in 0..120 {
        for folder in &folders {
            let project = folder.join("project.json");
            if project.is_file() {
                return super::media::import(project, true);
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    Err(
        "Steam chưa tải xong. Kiểm tra Subscribe và mục Downloads của Steam rồi bấm nhập lại."
            .into(),
    )
}

fn page_url(raw: &str, gifs: bool) -> Option<url::Url> {
    let url = url::Url::parse(raw).ok()?;
    if gifs {
        return (url.scheme() == "https"
            && matches!(url.host_str(), Some("giphy.com" | "www.giphy.com")))
        .then_some(url);
    }
    (url.scheme() == "https"
        && matches!(
            url.host_str(),
            Some(
                "steamcommunity.com"
                    | "www.steamcommunity.com"
                    | "store.steampowered.com"
                    | "login.steampowered.com"
                    | "help.steampowered.com"
            )
        ))
    .then_some(url)
}

pub fn browse(gifs: bool) -> Result<(), String> {
    let Some(_lock) = config::acquire_lock(
        if gifs { "giphy.lock" } else { "workshop.lock" },
        Duration::ZERO,
    ) else {
        return Ok(());
    };
    let events = EventLoopBuilder::<String>::with_user_event().build();
    let window = WindowBuilder::new()
        .with_title(if gifs {
            "Kanali Lite · GIPHY"
        } else {
            "Kanali Lite · Steam Workshop · Wallpaper Engine"
        })
        .with_inner_size(tao::dpi::LogicalSize::new(1040, 760))
        .build(&events)
        .map_err(|e| e.to_string())?;
    let mut session = WebContext::new(Some(config::data_dir().join(if gifs {
        "GiphyBrowser"
    } else {
        "SteamWorkshop"
    })));
    let proxy = events.create_proxy();
    let browser = WebViewBuilder::with_web_context(&mut session)
        .with_url(if gifs {
            "https://giphy.com/"
        } else {
            "https://steamcommunity.com/workshop/browse/?appid=431960"
        })
        .with_navigation_handler(move |raw| {
            let Some(url) = page_url(&raw, gifs) else {
                return false;
            };
            if gifs && url.path().starts_with("/gifs/") {
                if let Some(id) = url
                    .path()
                    .trim_end_matches('/')
                    .rsplit('-')
                    .next()
                    .and_then(|v| v.rsplit('/').next())
                    .filter(|id| valid_gif_id(id))
                {
                    let _ = config::save(&config::data_dir().join("giphy-item.json"), &id);
                }
            }
            if url.host_str() == Some("steamcommunity.com")
                && matches!(
                    url.path().trim_end_matches('/'),
                    "/sharedfiles/filedetails" | "/workshop/filedetails"
                )
            {
                if let Some(id) = url
                    .query_pairs()
                    .find(|(k, _)| k == "id")
                    .and_then(|(_, v)| v.parse::<u64>().ok())
                {
                    let _ = config::save(&config::data_dir().join("workshop-item.json"), &id);
                }
            }
            true
        })
        .with_new_window_req_handler(move |raw| {
            if page_url(&raw, gifs).is_some() {
                let _ = proxy.send_event(raw);
            }
            false
        })
        .build(&window)
        .map_err(|e| format!("Không mở được Steam Workshop (cần Microsoft Edge WebView2): {e}"))?;
    events.run(move |event, _, flow| {
        let _keep = &window;
        *flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *flow = ControlFlow::Exit,
            Event::UserEvent(url) => {
                let _ = browser.load_url(&url);
            }
            _ => (),
        }
    })
}

fn valid_gif_id(id: &str) -> bool {
    (4..=64).contains(&id.len()) && id.bytes().all(|c| c.is_ascii_alphanumeric())
}

pub fn import_gif(id: &str) -> Result<String, String> {
    use std::io::Read;
    if !valid_gif_id(id) {
        return Err("ID GIPHY không hợp lệ".into());
    }
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = tmp.path().join(format!("giphy-{id}.gif"));
    let response = ureq::get(&format!("https://media.giphy.com/media/{id}/giphy.gif"))
        .timeout(Duration::from_secs(45))
        .call()
        .map_err(|e| e.to_string())?;
    let mut output = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    let limit = 64 * 1024 * 1024;
    let n = std::io::copy(&mut response.into_reader().take(limit + 1), &mut output)
        .map_err(|e| e.to_string())?;
    if n == 0 || n > limit {
        return Err("GIF trống hoặc lớn hơn 64 MB".into());
    }
    drop(output);
    super::media::import(path, false)
}

/// Enumerate installed Workshop and locally authored projects in all Steam libraries.
pub fn installed_projects() -> Result<Vec<(String, PathBuf)>, String> {
    let mut projects = Vec::new();
    for lib in libraries()? {
        for root in [
            lib.join("workshop/content/431960"),
            lib.join("common/wallpaper_engine/projects/myprojects"),
            lib.join("common/wallpaper_engine/projects/defaultprojects"),
        ] {
            let Ok(dirs) = std::fs::read_dir(root) else {
                continue;
            };
            for dir in dirs.flatten() {
                let path = dir.path().join("project.json");
                if !path
                    .metadata()
                    .is_ok_and(|m| m.is_file() && m.len() < 4 * 1024 * 1024)
                {
                    continue;
                }
                let Some(meta) = config::load_opt::<serde_json::Value>(&path) else {
                    continue;
                };
                if !meta["type"].as_str().is_some_and(|t| {
                    matches!(t.to_ascii_lowercase().as_str(), "video" | "web" | "scene")
                }) {
                    continue;
                }
                let name = meta["title"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| dir.file_name().to_string_lossy().into_owned());
                projects.push((name, path));
            }
        }
    }
    projects.sort_by_cached_key(|p| p.0.to_lowercase());
    Ok(projects)
}
