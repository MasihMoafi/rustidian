//! Isolated CEF experiment: local page -> real DOM callback -> CPU PNG.
//! No vault API, Node/CommonJS shim, application integration or security bypass.
use cef::{args::Args, *};
use std::{
    error::Error,
    path::PathBuf,
    process::ExitCode,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
type Shared = Arc<Mutex<Progress>>;
const PREFIX: &str = "RUSTIDIAN_HTML_HOST_DOM:";

struct Bitmap {
    sequence: u64,
    width: u32,
    height: u32,
    bgra: Vec<u8>,
}

#[derive(Default)]
struct Progress {
    frame: Option<Bitmap>,
    sequence: u64,
    pending: Option<(String, u64)>,
    dom: Option<serde_json::Value>,
    error: Option<String>,
    closed: bool,
}

struct Options {
    file: PathBuf,
    output: PathBuf,
    profile: PathBuf,
    width: i32,
    height: i32,
    click: Option<(ClickTarget, PathBuf)>,
}

enum ClickTarget {
    Point(i32, i32),
    ElementId(String),
}

impl Options {
    fn parse() -> Result<Self> {
        let mut args = std::env::args().skip(1);
        let (mut file, mut output, mut profile, mut point, mut after) =
            (None, None, None, None, None);
        let (mut width, mut height) = (960, 640);
        while let Some(flag) = args.next() {
            let value = args.next().ok_or("Every option needs a value")?;
            match flag.as_str() {
                "--file" => file = Some(PathBuf::from(value).canonicalize()?),
                "--output" => output = Some(PathBuf::from(value)),
                "--profile" => profile = Some(PathBuf::from(value)),
                "--width" => width = value.parse()?,
                "--height" => height = value.parse()?,
                "--click" => {
                    let (x, y) = value.split_once(',').ok_or("Use --click X,Y")?;
                    point = Some(ClickTarget::Point(x.parse::<i32>()?, y.parse::<i32>()?));
                }
                "--click-id" => point = Some(ClickTarget::ElementId(value)),
                "--after-output" => after = Some(PathBuf::from(value)),
                _ => return Err(format!("Unknown option: {flag}").into()),
            }
        }
        if !(1..=4096).contains(&width) || !(1..=4096).contains(&height) {
            return Err("View dimensions must be within 1..=4096".into());
        }
        let click = match (point, after) {
            (Some(target), Some(path)) => Some((target, path)),
            (None, None) => None,
            _ => {
                return Err(
                    "Supply --click X,Y or --click-id ID, and --after-output together".into(),
                );
            }
        };
        let file = file.ok_or("Missing --file")?;
        if !file.is_file() {
            return Err("--file must identify a local HTML fixture".into());
        }
        let profile = profile.ok_or("Missing --profile: use a disposable directory")?;
        std::fs::create_dir_all(&profile)?;
        Ok(Self {
            file,
            output: output.ok_or("Missing --output")?,
            profile: profile.canonicalize()?,
            width,
            height,
            click,
        })
    }
}

// This script observes the actual loaded page. It never clicks or changes it.
fn inspect(frame: &Frame, phase: &str) {
    let code = r#"(async () => {
      const phase = __PHASE__;
      try {
        await Promise.all([...document.images].map(img => img.complete ? null :
          new Promise(resolve => { img.addEventListener('load', resolve, {once:true});
                                   img.addEventListener('error', resolve, {once:true}); })));
        await document.fonts.ready;
        await new Promise(requestAnimationFrame); await new Promise(requestAnimationFrame);
        const rect = el => { const r = el.getBoundingClientRect();
          return {x:r.x, y:r.y, width:r.width, height:r.height}; };
        console.log(__PREFIX__ + JSON.stringify({phase, url:location.href,
          title:document.title, readyState:document.readyState,
          text:document.body.innerText, dataset:{...document.body.dataset},
          count:document.getElementById('count')?.textContent ?? null,
          status:document.getElementById('status')?.textContent ?? null,
          background:getComputedStyle(document.body).backgroundColor,
          images:[...document.images].map(img => ({src:img.src, complete:img.complete,
            width:img.naturalWidth, height:img.naturalHeight})),
          controls:[...document.querySelectorAll('button,input,[data-probe]')].map(el =>
            ({tag:el.tagName, id:el.id, text:el.textContent, rect:rect(el),
              color:getComputedStyle(el).color, background:getComputedStyle(el).backgroundColor}))}));
      } catch (error) { console.log(__PREFIX__ + JSON.stringify({phase,error:String(error)})); }
    })();"#
        .replace("__PHASE__", &serde_json::to_string(phase).unwrap())
        .replace("__PREFIX__", &serde_json::to_string(PREFIX).unwrap());
    frame.execute_java_script(Some(&code.as_str().into()), None, 0);
}

wrap_render_handler! {
    struct Paint { shared: Shared, width: i32, height: i32, }
    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(rect) = rect { rect.width = self.width; rect.height = self.height; }
        }
        fn screen_info(&self, _browser: Option<&mut Browser>, info: Option<&mut ScreenInfo>) -> i32 {
            if let Some(info) = info {
                info.device_scale_factor = 1.0;
                info.depth = 32; info.depth_per_component = 8;
                info.rect = Rect { x:0, y:0, width:self.width, height:self.height };
                info.available_rect = info.rect.clone();
                1
            } else { 0 }
        }
        fn on_paint(&self, _browser: Option<&mut Browser>, type_: PaintElementType,
            _dirty: Option<&[Rect]>, buffer: *const u8, width: i32, height: i32) {
            if type_ != PaintElementType::default() || buffer.is_null()
                || !(1..=4096).contains(&width) || !(1..=4096).contains(&height) { return; }
            // CEF owns the BGRA buffer only for this callback. Copy it now.
            let bgra = unsafe { std::slice::from_raw_parts(buffer, width as usize * height as usize * 4) }.to_vec();
            let mut state = self.shared.lock().unwrap();
            state.sequence += 1;
            state.frame = Some(Bitmap { sequence:state.sequence, width:width as u32, height:height as u32, bgra });
        }
    }
}

wrap_load_handler! {
    struct Load { shared: Shared, }
    impl LoadHandler {
        fn on_load_end(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, _status: i32) {
            if let Some(frame) = frame && frame.is_main() != 0 { inspect(frame, "loaded"); }
        }
        fn on_load_error(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>,
            _code: Errorcode, text: Option<&CefString>, _url: Option<&CefString>) {
            if frame.is_some_and(|frame| frame.is_main() != 0) {
                self.shared.lock().unwrap().error = Some(text.map(|s| s.to_string()).unwrap_or_else(|| "Main-frame load failed".into()));
            }
        }
    }
}

wrap_display_handler! {
    struct Display { shared: Shared, }
    impl DisplayHandler {
        fn on_console_message(&self, browser: Option<&mut Browser>, _level: LogSeverity,
            message: Option<&CefString>, _source: Option<&CefString>, _line: i32) -> i32 {
            let message = message.map(|s| s.to_string()).unwrap_or_default();
            let Some(json) = message.strip_prefix(PREFIX) else { return 0; };
            match serde_json::from_str::<serde_json::Value>(json) {
                Ok(value) => {
                    // This is a real renderer -> browser process console callback.
                    println!("{value}");
                    let mut state = self.shared.lock().unwrap();
                    state.dom = Some(value.clone());
                    if let Some(error) = value.get("error") { state.error = Some(error.to_string()); }
                    else if let Some(phase) = value["phase"].as_str() {
                        state.pending = Some((phase.to_owned(), state.sequence));
                    }
                }
                Err(error) => self.shared.lock().unwrap().error = Some(error.to_string()),
            }
            if let Some(host) = browser.and_then(|b| b.host()) { host.invalidate(PaintElementType::default()); }
            1
        }
    }
}

wrap_life_span_handler! {
    struct Life { shared: Shared, }
    impl LifeSpanHandler {
        fn on_before_close(&self, _browser: Option<&mut Browser>) { self.shared.lock().unwrap().closed = true; }
    }
}

wrap_client! {
    struct PageClient { paint: RenderHandler, load: LoadHandler, display: DisplayHandler, life: LifeSpanHandler, }
    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> { Some(self.paint.clone()) }
        fn load_handler(&self) -> Option<LoadHandler> { Some(self.load.clone()) }
        fn display_handler(&self) -> Option<DisplayHandler> { Some(self.display.clone()) }
        fn life_span_handler(&self) -> Option<LifeSpanHandler> { Some(self.life.clone()) }
    }
}

fn save_frame(frame: Bitmap, output: &PathBuf) -> Result<()> {
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut rgba = frame.bgra;
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    image::save_buffer_with_format(
        output,
        &rgba,
        frame.width,
        frame.height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )?;
    eprintln!(
        "Saved {} ({}x{})",
        output.display(),
        frame.width,
        frame.height
    );
    Ok(())
}

fn capture(browser: &Browser, options: &Options, shared: &Shared) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut click_report_at = None;
    let host = browser.host().ok_or("Browser has no host")?;
    host.was_hidden(0);
    host.set_focus(1);
    loop {
        do_message_loop_work();
        if Instant::now() >= deadline {
            return Err("Timed out waiting for DOM callback and subsequent paint".into());
        }
        let ready = {
            let mut state = shared.lock().unwrap();
            if let Some(error) = state.error.take() {
                return Err(error.into());
            }
            match (&state.pending, &state.frame) {
                (Some((_, after)), Some(frame)) if frame.sequence > *after => {
                    Some((state.pending.take().unwrap().0, state.frame.take().unwrap()))
                }
                _ => None,
            }
        };
        if let Some((phase, frame)) = ready {
            if phase == "loaded" {
                save_frame(frame, &options.output)?;
                let Some((target, _)) = &options.click else {
                    return Ok(());
                };
                let (x, y) = match target {
                    ClickTarget::Point(x, y) => (*x, *y),
                    ClickTarget::ElementId(id) => {
                        let state = shared.lock().unwrap();
                        let controls = state
                            .dom
                            .as_ref()
                            .and_then(|dom| dom["controls"].as_array())
                            .ok_or("No renderer control rectangles")?;
                        let rect = &controls
                            .iter()
                            .find(|control| control["id"].as_str() == Some(id.as_str()))
                            .ok_or("Click ID missing from renderer report")?["rect"];
                        let left = rect["x"].as_f64().ok_or("Missing control x")?;
                        let top = rect["y"].as_f64().ok_or("Missing control y")?;
                        let width = rect["width"].as_f64().ok_or("Missing control width")?;
                        let height = rect["height"].as_f64().ok_or("Missing control height")?;
                        if width <= 0.0 || height <= 0.0 {
                            return Err("Click control has no visible size".into());
                        }
                        (
                            (left + width / 2.0).round() as i32,
                            (top + height / 2.0).round() as i32,
                        )
                    }
                };
                if !(0..options.width).contains(&x) || !(0..options.height).contains(&y) {
                    return Err("Native click point is outside the view".into());
                }
                let event = MouseEvent { x, y, modifiers: 0 };
                host.send_mouse_move_event(Some(&event), 0);
                host.send_mouse_click_event(Some(&event), MouseButtonType::LEFT, 0, 1);
                host.send_mouse_click_event(Some(&event), MouseButtonType::LEFT, 1, 1);
                click_report_at = Some(Instant::now() + Duration::from_millis(200));
            } else if phase == "clicked" {
                save_frame(
                    frame,
                    &options.click.as_ref().ok_or("Unexpected click phase")?.1,
                )?;
                return Ok(());
            }
        }
        if click_report_at.is_some_and(|at| Instant::now() >= at) {
            inspect(
                &browser.main_frame().ok_or("No main frame after click")?,
                "clicked",
            );
            click_report_at = None;
        }
        // Bounded experiment only: no application event loop or idle host exists.
        thread::sleep(Duration::from_millis(10));
    }
}

fn run() -> Result<i32> {
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let args = Args::new();
    let code = execute_process(Some(args.as_main_args()), None, std::ptr::null_mut());
    if code >= 0 {
        return Ok(code);
    }
    let options = Options::parse()?;
    let url =
        url::Url::from_file_path(&options.file).map_err(|_| "Cannot form fixture file URL")?;
    let settings = Settings {
        windowless_rendering_enabled: 1,
        external_message_pump: 1,
        root_cache_path: options
            .profile
            .to_str()
            .ok_or("Profile path must be UTF-8")?
            .into(),
        log_file: options
            .profile
            .join("cef.log")
            .to_str()
            .ok_or("Log path must be UTF-8")?
            .into(),
        ..Default::default()
    };
    // Default no_sandbox=0. Do not copy upstream example's bypass/debug switches.
    if initialize(
        Some(args.as_main_args()),
        Some(&settings),
        None,
        std::ptr::null_mut(),
    ) != 1
    {
        return Err("CEF initialization failed; inspect runtime/sandbox diagnostics".into());
    }
    let shared = Shared::default();
    let mut client = PageClient::new(
        Paint::new(shared.clone(), options.width, options.height),
        Load::new(shared.clone()),
        Display::new(shared.clone()),
        Life::new(shared.clone()),
    );
    let window = WindowInfo::default().set_as_windowless(Default::default());
    let browser_settings = BrowserSettings {
        windowless_frame_rate: 30,
        ..Default::default()
    };
    let Some(browser) = browser_host_create_browser_sync(
        Some(&window),
        Some(&mut client),
        Some(&url.as_str().into()),
        Some(&browser_settings),
        None,
        None,
    ) else {
        shutdown();
        return Err("CEF did not create the offscreen browser".into());
    };
    let outcome = capture(&browser, &options, &shared);
    if let Some(host) = browser.host() {
        host.close_browser(1);
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while !shared.lock().unwrap().closed && Instant::now() < deadline {
        do_message_loop_work();
        thread::sleep(Duration::from_millis(10));
    }
    if !shared.lock().unwrap().closed {
        return Err("CEF browser did not acknowledge close; shutdown withheld".into());
    }
    drop(browser);
    drop(client);
    shutdown();
    outcome?;
    Ok(0)
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(error) => {
            eprintln!("HTML host spike failed: {error}");
            ExitCode::FAILURE
        }
    }
}
