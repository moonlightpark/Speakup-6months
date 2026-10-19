//! A web page inside the app window (playphrase.me clips in the listening lab): a native web
//! view (WKWebView on macOS, WebView2 on Windows) laid over the part of the screen a page
//! asks for. Screens ask each frame with `App::web_req`; nothing asked = hidden.

use std::path::PathBuf;

/// Where and what to show this frame (egui points).
pub struct WebRequest {
    pub url: String,
    pub rect: egui::Rect,
    /// Page zoom (1 = as the site lays it out), to enlarge a page drawn at a fixed size.
    pub zoom: f32,
}

/// Runs in every page: on YouGlish, scrolls past the site's header and search box so the
/// "How to pronounce …" title and the video are at the top of the frame.
const PAGE_SCRIPT: &str = r#"
if (location.host.endsWith('youglish.com')) {
  let n = 0;
  const t = setInterval(() => {
    const hits = [...document.querySelectorAll('body *')].filter(e => /^\s*How to pronounce/.test(e.textContent || ''));
    if (hits.length) {
      hits.sort((a, b) => a.textContent.length - b.textContent.length);
      hits[0].scrollIntoView({ block: 'start' });
      clearInterval(t);
    }
    if (++n > 40) clearInterval(t);
  }, 250);
}
"#;

#[derive(Default)]
pub struct WebPane {
    view: Option<wry::WebView>,
    context: Option<wry::WebContext>,
    url: String,
    zoom: f32,
    visible: bool,
    /// The web view couldn't be created (e.g. no WebView2 on Windows): the page says so.
    pub failed: Option<String>,
}

impl WebPane {
    /// Creates, moves, loads or hides the web view to match `req`. `zoom` is egui's zoom factor
    /// (points × zoom = logical pixels of the window).
    pub fn sync(&mut self, window: &eframe::Frame, req: Option<&WebRequest>, zoom: f32, data_dir: PathBuf) {
        let Some(req) = req else {
            if self.visible {
                if let Some(v) = &self.view {
                    let _ = v.set_visible(false);
                }
                self.visible = false;
            }
            return;
        };
        let r = req.rect;
        let bounds = wry::Rect {
            position: wry::dpi::LogicalPosition::new((r.min.x * zoom) as f64, (r.min.y * zoom) as f64).into(),
            size: wry::dpi::LogicalSize::new((r.width() * zoom).max(1.0) as f64, (r.height() * zoom).max(1.0) as f64).into(),
        };
        match &self.view {
            Some(v) => {
                let _ = v.set_bounds(bounds);
                if (self.zoom - req.zoom).abs() > 0.01 {
                    let _ = v.zoom(req.zoom as f64);
                    self.zoom = req.zoom;
                }
                if !self.visible {
                    let _ = v.set_visible(true);
                    self.visible = true;
                }
                if self.url != req.url {
                    crate::applog!("web: {}", req.url);
                    if let Err(e) = v.load_url(&req.url) {
                        crate::applog!("web: 열기 실패 · {e}");
                    }
                    self.url = req.url.clone();
                }
            }
            None if self.failed.is_none() => {
                let ctx = self.context.get_or_insert_with(|| wry::WebContext::new(Some(data_dir)));
                let built = wry::WebViewBuilder::new_with_web_context(ctx).with_url(&req.url).with_bounds(bounds).with_autoplay(true).with_initialization_script(PAGE_SCRIPT).build_as_child(window);
                match built {
                    Ok(v) => {
                        crate::applog!("web: 웹 화면 생성 · {}", req.url);
                        let _ = v.zoom(req.zoom as f64);
                        self.zoom = req.zoom;
                        self.view = Some(v);
                        self.url = req.url.clone();
                        self.visible = true;
                    }
                    Err(e) => {
                        crate::applog!("web: 웹 화면을 만들 수 없음 · {e}");
                        self.failed = Some(format!("{e}"));
                    }
                }
            }
            None => {}
        }
    }
}
