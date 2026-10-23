#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod ftp;
mod storage;
use anyhow::{Context, Result};
use std::{
    net::TcpListener,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use storage::{Config, Store};
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wry::WebViewBuilder;

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let root = if let Some(i) = args.iter().position(|s| s == "--data-dir") {
        PathBuf::from(args.get(i + 1).context("Missing --data-dir path")?)
    } else {
        std::env::current_exe()?
            .parent()
            .context("Executable folder")?
            .to_path_buf()
    };
    let config = Config::load(&root)?;
    // Acquire both ports before any migration or changes to production data.
    let ftp_listener = TcpListener::bind((config.ftp_host.as_str(), config.ftp_port))
        .context("FTP 連接埠已被使用，請先關閉舊監控程式")?;
    let web = tiny_http::Server::http(("127.0.0.1", config.web_port))
        .map_err(|e| anyhow::anyhow!("網頁連接埠已被使用：{e}"))?;
    let store = Arc::new(Mutex::new(Store::open(root, config.clone())?));
    let stop = Arc::new(AtomicBool::new(false));
    let _ftp = ftp::start(ftp_listener, store.clone(), stop.clone())?;
    let (s, done) = (store.clone(), stop.clone());
    let _http = thread::spawn(move || {
        while !done.load(Ordering::Relaxed) {
            if let Ok(Some(request)) = web.recv_timeout(Duration::from_millis(100)) {
                let route = request.url().split('?').next().unwrap_or("");
                let reply: Option<(Vec<u8>, &str)> = match route {
                    "/" => Some((
                        include_bytes!("../assets/index.html").to_vec(),
                        "text/html; charset=utf-8",
                    )),
                    "/logo.png" => {
                        Some((include_bytes!("../assets/logo.png").to_vec(), "image/png"))
                    }
                    "/favicon.ico" => Some((include_bytes!("../assets/app.ico").to_vec(), "image/x-icon")),
                    "/app.css" => Some((include_bytes!("../assets/app.css").to_vec(), "text/css; charset=utf-8")),
                    "/app.js" => Some((include_bytes!("../assets/app.js").to_vec(), "text/javascript; charset=utf-8")),
                    "/api/status" => {
                        let state = s.lock().unwrap();
                        Some((serde_json::to_vec(&serde_json::json!({"cameras":state.config.cameras.iter().filter_map(|c|state.cameras.get(&c.ip)).collect::<Vec<_>>(),"engine":"rust-2.0","max_photos":state.config.max_photos,"data_path":state.root.to_string_lossy()})).unwrap(),"application/json; charset=utf-8"))
                    }
                    _ => {
                        let state = s.lock().unwrap();
                        let filename = route.trim_start_matches('/');
                        let allowed = state.cameras.values().any(|c| {
                            c.latest.as_ref().is_some_and(|p| p.file == filename)
                                || c.ng.as_ref().is_some_and(|p| p.file == filename)
                        });
                        if allowed && !filename.contains(['/', '\\']) {
                            std::fs::read(state.root.join("data").join(filename))
                                .ok()
                                .map(|b| {
                                    (
                                        b,
                                        if filename.ends_with("png") {
                                            "image/png"
                                        } else if filename.ends_with("bmp") {
                                            "image/bmp"
                                        } else {
                                            "image/jpeg"
                                        },
                                    )
                                })
                        } else {
                            None
                        }
                    }
                };
                let response = if let Some((bytes, kind)) = reply {
                    tiny_http::Response::from_data(bytes)
                        .with_header(tiny_http::Header::from_bytes("Content-Type", kind).unwrap())
                        .with_header(
                            tiny_http::Header::from_bytes("Cache-Control", "no-store").unwrap(),
                        )
                } else {
                    tiny_http::Response::from_data(b"Not found".to_vec()).with_status_code(404)
                };
                let _ = request.respond(response);
            }
        }
    });
    if args.iter().any(|s| s == "--headless") {
        if let Some(i) = args.iter().position(|s| s == "--duration-seconds") {
            thread::sleep(Duration::from_secs(
                args.get(i + 1).context("Missing duration")?.parse()?,
            ));
            stop.store(true, Ordering::Relaxed);
            return Ok(());
        }
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_window_icon(Some(tao::window::Icon::from_rgba(include_bytes!("../assets/app-icon.rgba").to_vec(),64,64)?))
        .with_title("擎添工業｜轉接平台影像監控 · Rust")
        .with_inner_size(tao::dpi::LogicalSize::new(1440., 960.))
        .with_min_inner_size(tao::dpi::LogicalSize::new(900., 650.))
        .build(&event_loop)?;
    let _view = WebViewBuilder::new()
        .with_url(format!("http://127.0.0.1:{}", config.web_port))
        .build(&window)
        .context("無法開啟監控視窗，請確認已安裝 Microsoft Edge WebView2 Runtime")?;
    let smoke = args.iter().any(|s| s == "--ui-smoke-test");
    let deadline = std::time::Instant::now() + Duration::from_secs(4);
    event_loop.run(move |event, _, control| {
        *control = ControlFlow::Wait;
        if smoke {
            *control = ControlFlow::WaitUntil(deadline);
            if std::time::Instant::now() >= deadline {
                stop.store(true, Ordering::Relaxed);
                *control = ControlFlow::Exit;
            }
        }
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            stop.store(true, Ordering::Relaxed);
            *control = ControlFlow::Exit;
        }
    });
}
fn main() {
    if let Err(e) = run() {
        if std::env::args().any(|a| a == "--headless") {
            eprintln!("{e:#}");
            std::process::exit(1);
        }
        rfd::MessageDialog::new()
            .set_title("監控無法啟動")
            .set_description(format!("{e:#}"))
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
}
