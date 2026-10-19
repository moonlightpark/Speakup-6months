//! A web page inside the app window (YouGlish videos in the listening lab): a native web
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

/// Runs in every page: on YouGlish, shows only the video, its control bar and the caption box
/// below it, as wide as the frame (everything else hidden). If the page isn't laid out as
/// expected (the site changed), it is left as is, scrolled to the "How to pronounce" title.
const PAGE_SCRIPT: &str = r#"
if (location.host.endsWith('youglish.com')) {
  const KEEP = ['videowrapper', 'controlIdParent', 'captioncont', 'overlay', 'captcha_container'];
  const isolate = () => {
    const kept = KEEP.map(id => document.getElementById(id)).filter(Boolean);
    if (!document.getElementById('videowrapper')) return false;
    const chain = new Set();
    for (const k of kept) for (let e = k; e && e !== document.documentElement; e = e.parentElement) chain.add(e);
    for (const e of chain) {
      if (kept.includes(e)) continue;
      for (const c of e.children) {
        if (!chain.has(c) && c.tagName !== 'SCRIPT' && c.tagName !== 'STYLE') c.style.setProperty('display', 'none', 'important');
      }
      if (e !== document.body) for (const [k, v] of [['margin', '0'], ['padding', '0'], ['max-width', 'none'], ['width', '100%'], ['border', 'none'], ['box-shadow', 'none']]) e.style.setProperty(k, v, 'important');
    }
    for (const id of ['videowrapper', 'controlIdParent', 'captioncont']) {
      const e = document.getElementById(id);
      if (e) { e.style.setProperty('max-width', 'none', 'important'); e.style.setProperty('margin', '0', 'important'); }
    }
    document.body.style.setProperty('margin', '0', 'important');
    document.body.style.setProperty('padding', '0', 'important');
    document.body.style.setProperty('overflow-x', 'hidden', 'important');
    window.scrollTo(0, 0);
    return true;
  };
  let n = 0, done = false;
  // Repeated for a while: the site adds parts of the page after loading.
  const t = setInterval(() => {
    if (isolate()) done = true;
    if (++n > 40) {
      clearInterval(t);
      if (!done) {
        const hits = [...document.querySelectorAll('body *')].filter(e => /^\s*How to pronounce/.test(e.textContent || ''));
        hits.sort((a, b) => a.textContent.length - b.textContent.length);
        if (hits[0]) hits[0].scrollIntoView({ block: 'start' });
      }
    }
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
