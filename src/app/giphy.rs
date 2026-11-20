use kanali_rs::config;
use std::time::Duration;
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
};
use wry::{WebContext, WebViewBuilder};

fn page_url(raw: &str) -> Option<url::Url> {
    let url = url::Url::parse(raw).ok()?;
    (url.scheme() == "https" && matches!(url.host_str(), Some("giphy.com" | "www.giphy.com")))
        .then_some(url)
}

pub fn browse() -> Result<(), String> {
    let Some(_lock) = config::acquire_lock("giphy.lock", Duration::ZERO) else {
        return Ok(());
    };
    let events = EventLoopBuilder::<String>::with_user_event().build();
    let window = WindowBuilder::new()
        .with_title("Kanali Lite · GIPHY")
        .with_inner_size(tao::dpi::LogicalSize::new(1040, 760))
        .build(&events)
        .map_err(|e| e.to_string())?;
    let mut session = WebContext::new(Some(config::data_dir().join("GiphyBrowser")));
    let proxy = events.create_proxy();
    let browser = WebViewBuilder::with_web_context(&mut session)
        .with_url("https://giphy.com/")
        .with_navigation_handler(move |raw| {
            let Some(url) = page_url(&raw) else {
                return false;
            };
            if url.path().starts_with("/gifs/") {
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
            true
        })
        .with_new_window_req_handler(move |raw| {
            if page_url(&raw).is_some() {
                let _ = proxy.send_event(raw);
            }
            false
        })
        .build(&window)
        .map_err(|e| format!("Không mở được GIPHY (cần Microsoft Edge WebView2): {e}"))?;
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
