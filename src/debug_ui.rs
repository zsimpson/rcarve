// Debug UI collector + viewer.
//
// This module intentionally prioritizes programmer ergonomics for debugging.
// Callers should be able to drop in one-liner `add_*()` calls anywhere,
// and a single `show()` at the end to inspect everything.
//
// When the `debug_ui` feature is disabled (or `cli_only` is enabled), all APIs
// in this module become no-ops.

#[cfg(all(feature = "debug_ui", not(feature = "cli_only")))]
mod imp {
    use crate::im::{Im, Lum16Im, RGBAIm};
    use crate::im::MaskIm;
    use crate::region_tree::{PlyIm, RegionIm};
    use crate::toolpath::ToolPath;
    use eframe::egui;
    use std::sync::{Mutex, OnceLock};

    #[derive(Clone, Debug)]
    enum SourcePixels {
        U8_1 { arr: Vec<u8>, max: u8 },
        U8_4 { arr: Vec<u8> },
        U16_1 { arr: Vec<u16>, max: u16 },
    }

    #[derive(Clone, Debug)]
    struct SourceIm {
        w: usize,
        h: usize,
        pixels: SourcePixels,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum VizMode {
        GrayAutoMax,
        RgbaPassthrough,
    }

    #[derive(Clone, Copy, Debug)]
    struct VizParams {
        mul: f32,
    }

    impl SourceIm {
        fn source_text_at(&self, x: usize, y: usize) -> String {
            match &self.pixels {
                SourcePixels::U8_1 { arr, max } => {
                    let v = arr[y * self.w + x];
                    format!("src=u8({v}) max={max}")
                }
                SourcePixels::U8_4 { arr } => {
                    let base = (y * self.w + x) * 4;
                    let r = arr[base];
                    let g = arr[base + 1];
                    let b = arr[base + 2];
                    let a = arr[base + 3];
                    format!("src=rgba8({r},{g},{b},{a})")
                }
                SourcePixels::U16_1 { arr, max } => {
                    let v = arr[y * self.w + x];
                    format!("src=u16({v}) max={max}")
                }
            }
        }

        fn default_mode(&self) -> VizMode {
            match &self.pixels {
                SourcePixels::U8_4 { .. } => VizMode::RgbaPassthrough,
                SourcePixels::U8_1 { .. } | SourcePixels::U16_1 { .. } => VizMode::GrayAutoMax,
            }
        }

        fn render_to_rgba8(&self, mode: VizMode, params: VizParams, out_rgba: &mut RGBAIm) {
            debug_assert_eq!(out_rgba.w, self.w);
            debug_assert_eq!(out_rgba.h, self.h);
            debug_assert_eq!(out_rgba.arr.len(), self.w * self.h * 4);

            match (&self.pixels, mode) {
                (SourcePixels::U8_4 { arr }, VizMode::RgbaPassthrough) => {
                    out_rgba.arr.copy_from_slice(arr);
                }

                (SourcePixels::U8_1 { arr, max }, VizMode::GrayAutoMax) => {
                    let maxf = (*max as f32).max(1.0);
                    let mul = params.mul.max(0.0);
                    for y in 0..self.h {
                        for x in 0..self.w {
                            let v = arr[y * self.w + x] as f32;
                            let scaled = ((v / maxf) * 255.0 * mul).clamp(0.0, 255.0) as u8;
                            let base = (y * self.w + x) * 4;
                            out_rgba.arr[base] = scaled;
                            out_rgba.arr[base + 1] = scaled;
                            out_rgba.arr[base + 2] = scaled;
                            out_rgba.arr[base + 3] = 255;
                        }
                    }
                }

                (SourcePixels::U16_1 { arr, max }, VizMode::GrayAutoMax) => {
                    let maxf = (*max as f32).max(1.0);
                    let mul = params.mul.max(0.0);
                    for y in 0..self.h {
                        for x in 0..self.w {
                            let v = arr[y * self.w + x] as f32;
                            let scaled = ((v / maxf) * 255.0 * mul).clamp(0.0, 255.0) as u8;
                            let base = (y * self.w + x) * 4;
                            out_rgba.arr[base] = scaled;
                            out_rgba.arr[base + 1] = scaled;
                            out_rgba.arr[base + 2] = scaled;
                            out_rgba.arr[base + 3] = 255;
                        }
                    }
                }

                _ => {
                    out_rgba.arr.fill(0);
                    for i in (3..out_rgba.arr.len()).step_by(4) {
                        out_rgba.arr[i] = 255;
                    }
                }
            }
        }
    }

    #[derive(Clone, Debug)]
    struct DebugImageData {
        title: String,
        src: SourceIm,
    }

    #[derive(Clone, Debug)]
    struct DebugToolpathMovieData {
        title: String,
        base: Lum16Im,
        toolpaths: Vec<ToolPath>,
        tool_dia_pix: usize,
    }

    #[derive(Clone, Debug)]
    enum DebugItemData {
        Image(DebugImageData),
        ToolpathMovie(DebugToolpathMovieData),
    }

    #[derive(Default)]
    struct DebugUiState {
        title: String,
        items: Vec<DebugItemData>,
    }

    fn global_state() -> &'static Mutex<DebugUiState> {
        static G: OnceLock<Mutex<DebugUiState>> = OnceLock::new();
        G.get_or_init(|| {
            Mutex::new(DebugUiState {
                title: "rcarve debug".to_owned(),
                items: Vec::new(),
            })
        })
    }

    fn pack_u8_1<S>(im: &Im<u8, 1, S>) -> (Vec<u8>, u8) {
        let mut out = vec![0u8; im.w * im.h];
        let mut maxv = 0u8;
        for y in 0..im.h {
            for x in 0..im.w {
                let v = unsafe { *im.get_unchecked(x, y, 0) };
                maxv = maxv.max(v);
                out[y * im.w + x] = v;
            }
        }
        (out, maxv)
    }

    fn pack_u8_4<S>(im: &Im<u8, 4, S>) -> Vec<u8> {
        let mut out = vec![0u8; im.w * im.h * 4];
        for y in 0..im.h {
            for x in 0..im.w {
                let base = (y * im.w + x) * 4;
                out[base] = unsafe { *im.get_unchecked(x, y, 0) };
                out[base + 1] = unsafe { *im.get_unchecked(x, y, 1) };
                out[base + 2] = unsafe { *im.get_unchecked(x, y, 2) };
                out[base + 3] = unsafe { *im.get_unchecked(x, y, 3) };
            }
        }
        out
    }

    fn pack_u16_1<S>(im: &Im<u16, 1, S>) -> (Vec<u16>, u16) {
        let mut out = vec![0u16; im.w * im.h];
        let mut maxv = 0u16;
        for y in 0..im.h {
            for x in 0..im.w {
                let v = unsafe { *im.get_unchecked(x, y, 0) };
                maxv = maxv.max(v);
                out[y * im.w + x] = v;
            }
        }
        (out, maxv)
    }

    fn pack_lum16(im: &Lum16Im) -> Lum16Im {
        if im.s == im.w {
            return im.clone();
        }

        let mut packed = Lum16Im::new(im.w, im.h);
        for y in 0..im.h {
            let row0 = y * im.s;
            let row = &im.arr[row0..row0 + im.w];
            packed.arr[y * packed.s..y * packed.s + im.w].copy_from_slice(row);
        }
        packed
    }

    // Public API (collector)
    // -------------------------------------------------------------------------

    pub fn init(title: &str) {
        let mut g = global_state().lock().unwrap();
        g.title = title.to_owned();
        g.items.clear();
    }

    pub fn add_u8_1<S>(title: &str, im: &Im<u8, 1, S>) {
        let (arr, max) = pack_u8_1(im);
        let src = SourceIm {
            w: im.w,
            h: im.h,
            pixels: SourcePixels::U8_1 { arr, max },
        };

        let mut g = global_state().lock().unwrap();
        g.items.push(DebugItemData::Image(DebugImageData {
            title: title.to_owned(),
            src,
        }));
    }

    pub fn add_u8_4<S>(title: &str, im: &Im<u8, 4, S>) {
        let arr = pack_u8_4(im);
        let src = SourceIm {
            w: im.w,
            h: im.h,
            pixels: SourcePixels::U8_4 { arr },
        };

        let mut g = global_state().lock().unwrap();
        g.items.push(DebugItemData::Image(DebugImageData {
            title: title.to_owned(),
            src,
        }));
    }

    pub fn add_u16_1<S>(title: &str, im: &Im<u16, 1, S>) {
        let (arr, max) = pack_u16_1(im);
        let src = SourceIm {
            w: im.w,
            h: im.h,
            pixels: SourcePixels::U16_1 { arr, max },
        };

        let mut g = global_state().lock().unwrap();
        g.items.push(DebugItemData::Image(DebugImageData {
            title: title.to_owned(),
            src,
        }));
    }

    pub fn add_mask_im(title: &str, im: &MaskIm) {
        add_u8_1(title, im);
    }

    /// Create a new 1-channel mask image and set pixels along the rectangle border.
    ///
    /// Coordinates are interpreted like an ROI: left/top inclusive, right/bottom exclusive.
    /// The created image size defaults to `w=r` and `h=b` so the rect always fits.
    pub fn add_rect(l: usize, t: usize, r: usize, b: usize) {
        if r == 0 || b == 0 {
            return;
        }

        let mut im = MaskIm::new(r, b);

        // Clamp ROI to image bounds.
        let l = l.min(im.w);
        let t = t.min(im.h);
        let r = r.min(im.w);
        let b = b.min(im.h);
        if l >= r || t >= b {
            add_mask_im(&format!("rect empty l={l} t={t} r={r} b={b}"), &im);
            return;
        }

        let xm1 = r.saturating_sub(1);
        let ym1 = b.saturating_sub(1);

        // Horizontal edges.
        if t < im.h {
            let row = t * im.s;
            for x in l..r {
                im.arr[row + x] = 255;
            }
        }
        if b >= 2 {
            let y = ym1;
            let row = y * im.s;
            for x in l..r {
                im.arr[row + x] = 255;
            }
        }

        // Vertical edges.
        if l < im.w {
            for y in t..b {
                im.arr[y * im.s + l] = 255;
            }
        }
        if r >= 2 {
            let x = xm1;
            for y in t..b {
                im.arr[y * im.s + x] = 255;
            }
        }

        add_mask_im(&format!("rect l={l} t={t} r={r} b={b}"), &im);
    }

    pub fn add_ply_im(title: &str, im: &PlyIm) {
        add_u16_1(title, im);
    }

    pub fn add_region_im(title: &str, im: &RegionIm) {
        add_u16_1(title, im);
    }

    pub fn add_lum16(title: &str, im: &Lum16Im) {
        add_u16_1(title, im);
    }

    pub fn add_rgba(title: &str, im: &RGBAIm) {
        add_u8_4(title, im);
    }

    pub fn add_toolpath_movie(title: &str, base: &Lum16Im, toolpaths: &[ToolPath], tool_dia_pix: usize) {
        let mut g = global_state().lock().unwrap();
        g.items.push(DebugItemData::ToolpathMovie(DebugToolpathMovieData {
            title: title.to_owned(),
            base: pack_lum16(base),
            toolpaths: toolpaths.to_vec(),
            tool_dia_pix: tool_dia_pix.max(1),
        }));
    }

    // Legacy (single-window) APIs used by existing helpers
    // -------------------------------------------------------------------------

    pub fn show_u8_1<S>(im: &Im<u8, 1, S>, title: &str) -> Result<(), String> {
        let (arr, max) = pack_u8_1(im);
        let src = SourceIm {
            w: im.w,
            h: im.h,
            pixels: SourcePixels::U8_1 { arr, max },
        };
        run_single_image(title, src)
    }

    pub fn show_u8_4<S>(im: &Im<u8, 4, S>, title: &str) -> Result<(), String> {
        let arr = pack_u8_4(im);
        let src = SourceIm {
            w: im.w,
            h: im.h,
            pixels: SourcePixels::U8_4 { arr },
        };
        run_single_image(title, src)
    }

    pub fn show_u16_1<S>(im: &Im<u16, 1, S>, title: &str) -> Result<(), String> {
        let (arr, max) = pack_u16_1(im);
        let src = SourceIm {
            w: im.w,
            h: im.h,
            pixels: SourcePixels::U16_1 { arr, max },
        };
        run_single_image(title, src)
    }

    pub fn show_toolpath_movie(base: &Lum16Im, toolpaths: &[ToolPath], title: &str) -> Result<(), String> {
        let base = pack_lum16(base);
        let toolpaths = toolpaths.to_vec();
        run_single_movie(title, base, toolpaths, 20)
    }

    // Unified UI
    // -------------------------------------------------------------------------

    pub fn show() {
        let (title, items) = {
            let mut g = global_state().lock().unwrap();
            let title = g.title.clone();
            let items = std::mem::take(&mut g.items);
            (title, items)
        };

        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size(egui::vec2(1200.0, 800.0)),
            ..Default::default()
        };
        let window_title = title.clone();

        // Debugger ergonomics: if this fails, just panic.
        eframe::run_native(
            &window_title,
            options,
            Box::new(move |_cc| Ok(Box::new(DebugUiApp::new(&title, items)))),
        )
        .unwrap();
    }

    struct DebugUiApp {
        title: String,
        items: Vec<DebugItem>,
        selected: usize,
    }

    enum DebugItem {
        Image(ImageViewer),
        ToolpathMovie(ToolpathMovieViewer),
    }

    impl DebugUiApp {
        fn new(title: &str, items: Vec<DebugItemData>) -> Self {
            let mut out = Vec::with_capacity(items.len());
            for it in items {
                match it {
                    DebugItemData::Image(d) => out.push(DebugItem::Image(ImageViewer::new(&d.title, d.src))),
                    DebugItemData::ToolpathMovie(d) => out.push(DebugItem::ToolpathMovie(ToolpathMovieViewer::new(
                        &d.title,
                        d.base,
                        d.toolpaths,
                        d.tool_dia_pix,
                    ))),
                }
            }

            Self {
                title: title.to_owned(),
                items: out,
                selected: 0,
            }
        }

        fn item_titles(&self) -> Vec<String> {
            self.items
                .iter()
                .map(|it| match it {
                    DebugItem::Image(v) => format!("img: {}", v.title),
                    DebugItem::ToolpathMovie(v) => format!("movie: {}", v.title),
                })
                .collect()
        }

        fn selected_item_mut(&mut self) -> Option<&mut DebugItem> {
            if self.items.is_empty() {
                return None;
            }
            let i = self.selected.min(self.items.len().saturating_sub(1));
            Some(&mut self.items[i])
        }
    }

    impl eframe::App for DebugUiApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            egui::TopBottomPanel::top("top").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(&self.title);
                    ui.separator();
                    ui.monospace(format!("items={}", self.items.len()));
                });
            });

            // Make the Debug Items column wider by default (2x the previous implicit default).
            egui::SidePanel::left("left")
                .resizable(true)
                .default_width(400.0)
                .show(ctx, |ui| {
                ui.heading("Debug Items");
                ui.separator();

                if self.items.is_empty() {
                    ui.label("No debug items collected.");
                    return;
                }

                let titles = self.item_titles();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (i, t) in titles.iter().enumerate() {
                            let selected = i == self.selected;
                            if ui.selectable_label(selected, t).clicked() {
                                self.selected = i;
                            }
                        }
                    });
                });

            egui::CentralPanel::default().show(ctx, |ui| {
                let Some(item) = self.selected_item_mut() else {
                    ui.label("No selection");
                    return;
                };

                match item {
                    DebugItem::Image(v) => v.ui(ctx, ui),
                    DebugItem::ToolpathMovie(v) => v.ui(ctx, ui),
                }
            });

            ctx.request_repaint();
        }
    }

    // Image viewer component (reuses the old im::debug_ui behavior)
    // -------------------------------------------------------------------------

    struct ImageViewer {
        title: String,
        src: SourceIm,
        rgba: RGBAIm,
        mode: VizMode,
        params: VizParams,
        texture: Option<egui::TextureHandle>,
        hover_text: String,
        cmd: String,
        status: String,
        dirty: bool,
    }

    impl ImageViewer {
        fn new(title: &str, src: SourceIm) -> Self {
            let w = src.w;
            let h = src.h;
            let rgba = RGBAIm::new(w, h);
            let mode = src.default_mode();
            let params = VizParams { mul: 1.0 };
            Self {
                title: title.to_owned(),
                src,
                rgba,
                mode,
                params,
                texture: None,
                hover_text: String::new(),
                cmd: String::new(),
                status: "cmd: mul <f32> | reset | mode gray|rgba | help".to_owned(),
                dirty: true,
            }
        }

        fn render_if_needed(&mut self, ctx: &egui::Context) {
            if !self.dirty && self.texture.is_some() {
                return;
            }

            self.src.render_to_rgba8(self.mode, self.params, &mut self.rgba);

            let w = self.rgba.w;
            let h = self.rgba.h;
            let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &self.rgba.arr);

            match &mut self.texture {
                Some(tex) => tex.set(img, egui::TextureOptions::NEAREST),
                None => {
                    self.texture = Some(ctx.load_texture(
                        "im_debug",
                        img,
                        egui::TextureOptions::NEAREST,
                    ))
                }
            }

            self.dirty = false;
        }

        fn rgba_text_at(&self, x: usize, y: usize) -> String {
            let base = (y * self.rgba.w + x) * 4;
            let r = self.rgba.arr[base];
            let g = self.rgba.arr[base + 1];
            let b = self.rgba.arr[base + 2];
            let a = self.rgba.arr[base + 3];
            format!("viz=rgba8({r},{g},{b},{a})")
        }

        fn apply_cmd(&mut self, line: &str) {
            let mut it = line.split_whitespace();
            let Some(cmd) = it.next() else {
                return;
            };

            match cmd {
                "mul" => {
                    if let Some(v) = it.next() {
                        match v.parse::<f32>() {
                            Ok(m) if m.is_finite() => {
                                self.params.mul = m;
                                self.dirty = true;
                                self.status = format!("mul set to {}", self.params.mul);
                            }
                            _ => self.status = "mul expects a finite f32, e.g. `mul 1.5`".to_owned(),
                        }
                    } else {
                        self.status = "usage: mul <f32>".to_owned();
                    }
                }
                "reset" => {
                    self.params.mul = 1.0;
                    self.mode = self.src.default_mode();
                    self.dirty = true;
                    self.status = "reset params".to_owned();
                }
                "mode" => match it.next() {
                    Some("gray") => {
                        self.mode = VizMode::GrayAutoMax;
                        self.dirty = true;
                        self.status = "mode=gray".to_owned();
                    }
                    Some("rgba") => {
                        self.mode = VizMode::RgbaPassthrough;
                        self.dirty = true;
                        self.status = "mode=rgba".to_owned();
                    }
                    _ => self.status = "usage: mode gray|rgba".to_owned(),
                },
                "help" => {
                    self.status = "cmd: mul <f32> | reset | mode gray|rgba | help".to_owned();
                }
                _ => {
                    self.status = format!("unknown cmd: {cmd} (try `help`)");
                }
            }
        }

        fn ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
            self.render_if_needed(ctx);

            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(&self.title);
                    ui.separator();
                    ui.monospace(format!("mode={:?} mul={:.4}", self.mode, self.params.mul));
                    if !self.hover_text.is_empty() {
                        ui.separator();
                        ui.monospace(&self.hover_text);
                    }
                });

                let w = self.src.w;
                let h = self.src.h;
                let Some(tex) = &self.texture else { return };

                let image_size = egui::vec2(w as f32, h as f32);
                let response = ui.add(egui::Image::new((tex.id(), image_size)));

                if response.hovered() {
                    if let Some(pos) = response.hover_pos() {
                        let rect = response.rect;
                        let fx = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 0.999_999);
                        let fy = ((pos.y - rect.top()) / rect.height()).clamp(0.0, 0.999_999);
                        let x = (fx * (w as f32)) as usize;
                        let y = (fy * (h as f32)) as usize;

                        let src = self.src.source_text_at(x, y);
                        let viz = self.rgba_text_at(x, y);
                        self.hover_text = format!("x={x} y={y} {src} {viz}");
                    }
                }

                ui.separator();
                ui.horizontal(|ui| {
                    ui.monospace("cmd>");
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.cmd)
                            .desired_width(f32::INFINITY)
                            .hint_text("mul 2.0 | reset | mode gray"),
                    );

                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let line = self.cmd.trim().to_owned();
                        self.cmd.clear();
                        self.apply_cmd(&line);
                    }
                });
                if !self.status.is_empty() {
                    ui.monospace(&self.status);
                }
            });
        }
    }

    // Toolpath movie component (reuses the old sim::debug_ui behavior)
    // -------------------------------------------------------------------------

    struct ToolpathMovieViewer {
        title: String,

        // Inputs
        base: Lum16Im,
        movie_toolpaths: Vec<ToolPath>,
        tool_dia_pix: usize,

        // Movie state
        applied_count: usize,

        // Render state
        sim: Lum16Im,
        rgba: RGBAIm,
        params: VizParams,
        texture: Option<egui::TextureHandle>,
        dirty: bool,

        // UI state
        hover_text: String,
        cmd: String,
        status: String,
    }

    impl ToolpathMovieViewer {
        fn new(title: &str, base: Lum16Im, toolpaths: Vec<ToolPath>, tool_dia_pix: usize) -> Self {
            let w = base.w;
            let h = base.h;
            let sim = Lum16Im::new(w, h);
            let rgba = RGBAIm::new(w, h);

            Self {
                title: title.to_owned(),
                base,
                movie_toolpaths: toolpaths,
                tool_dia_pix: tool_dia_pix.max(1),
                applied_count: 0,
                sim,
                rgba,
                params: VizParams { mul: 1.0 },
                texture: None,
                dirty: true,
                hover_text: String::new(),
                cmd: String::new(),
                status: "cmd: tp <i> | frame <n> | next | prev | first | last | tool <pix> | mul <f32> | reset | help".to_owned(),
            }
        }

        fn toolpath_len(&self) -> usize {
            self.movie_toolpaths.len()
        }

        fn active_toolpath_index(&self) -> Option<usize> {
            self.applied_count.checked_sub(1)
        }

        fn clamp_applied_count(&self, n: usize) -> usize {
            n.min(self.toolpath_len())
        }

        fn set_applied_count(&mut self, n: usize) {
            let n = self.clamp_applied_count(n);
            if n != self.applied_count {
                self.applied_count = n;
                self.dirty = true;
            }
        }

        fn step_applied(&mut self, delta: i32) {
            let cur = self.applied_count as i32;
            let max = self.toolpath_len() as i32;
            let next = (cur + delta).clamp(0, max) as usize;
            self.set_applied_count(next);
        }

        fn src_text_at(&self, x: usize, y: usize) -> String {
            let i = y * self.sim.s + x;
            let v = self.sim.arr[i];
            let max = self.sim.arr.iter().copied().max().unwrap_or(0);
            format!("src=u16({v}) max={max}")
        }

        fn rgba_text_at(&self, x: usize, y: usize) -> String {
            let base = (y * self.rgba.w + x) * 4;
            let r = self.rgba.arr[base];
            let g = self.rgba.arr[base + 1];
            let b = self.rgba.arr[base + 2];
            let a = self.rgba.arr[base + 3];
            format!("viz=rgba8({r},{g},{b},{a})")
        }

        fn recompute_sim(&mut self) {
            debug_assert_eq!(self.base.w, self.sim.w);
            debug_assert_eq!(self.base.h, self.sim.h);
            self.sim.arr.copy_from_slice(&self.base.arr);

            if self.applied_count > 0 {
                let n = self.applied_count.min(self.movie_toolpaths.len());
                if n > 0 {
                    crate::sim::sim_toolpaths(&mut self.sim, &mut self.movie_toolpaths[..n]);
                }
            }
        }

        fn render_sim_to_rgba(&mut self) {
            let maxv = self.sim.arr.iter().copied().max().unwrap_or(0);
            let maxf = (maxv as f32).max(1.0);
            let mul = self.params.mul.max(0.0);

            for y in 0..self.sim.h {
                for x in 0..self.sim.w {
                    let v = self.sim.arr[y * self.sim.s + x] as f32;
                    let scaled = ((v / maxf) * 255.0 * mul).clamp(0.0, 255.0) as u8;
                    let base = (y * self.sim.w + x) * 4;
                    self.rgba.arr[base] = scaled;
                    self.rgba.arr[base + 1] = scaled;
                    self.rgba.arr[base + 2] = scaled;
                    self.rgba.arr[base + 3] = 255;
                }
            }
        }

        fn render_if_needed(&mut self, ctx: &egui::Context) {
            if !self.dirty && self.texture.is_some() {
                return;
            }

            self.recompute_sim();
            self.render_sim_to_rgba();

            let w = self.rgba.w;
            let h = self.rgba.h;
            let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &self.rgba.arr);

            match &mut self.texture {
                Some(tex) => tex.set(img, egui::TextureOptions::NEAREST),
                None => {
                    self.texture = Some(ctx.load_texture(
                        "sim_toolpath_movie",
                        img,
                        egui::TextureOptions::NEAREST,
                    ))
                }
            }

            self.dirty = false;
        }

        fn apply_cmd(&mut self, line: &str) {
            let mut it = line.split_whitespace();
            let Some(cmd) = it.next() else {
                return;
            };

            match cmd {
                "tp" => {
                    if let Some(v) = it.next() {
                        match v.parse::<usize>() {
                            Ok(i) if i < self.toolpath_len() => {
                                self.set_applied_count(i + 1);
                                self.status = format!("toolpath set to {i}");
                            }
                            Ok(i) => {
                                self.status = format!(
                                    "tp out of range: {i} (valid 0..{})",
                                    self.toolpath_len().saturating_sub(1)
                                );
                            }
                            Err(_) => self.status = "tp expects a usize, e.g. `tp 5`".to_owned(),
                        }
                    } else {
                        self.status = "usage: tp <toolpath_index>".to_owned();
                    }
                }
                "frame" => {
                    if let Some(v) = it.next() {
                        match v.parse::<usize>() {
                            Ok(n) => {
                                self.set_applied_count(n);
                                self.status = format!("frame(applied) set to {n}");
                            }
                            Err(_) => self.status = "frame expects a usize, e.g. `frame 10`".to_owned(),
                        }
                    } else {
                        self.status = "usage: frame <applied_count>".to_owned();
                    }
                }
                "next" => {
                    self.step_applied(1);
                    self.status = "next".to_owned();
                }
                "prev" => {
                    self.step_applied(-1);
                    self.status = "prev".to_owned();
                }
                "first" => {
                    self.set_applied_count(0);
                    self.status = "first".to_owned();
                }
                "last" => {
                    self.set_applied_count(self.toolpath_len());
                    self.status = "last".to_owned();
                }
                "tool" | "tool_dia" | "tool_dia_pix" => {
                    if let Some(v) = it.next() {
                        match v.parse::<usize>() {
                            Ok(pix) if pix >= 1 => {
                                self.tool_dia_pix = pix;
                                self.dirty = true;
                                self.status = format!("tool_dia_pix set to {pix}");
                            }
                            _ => {
                                self.status = "tool expects a usize >= 1, e.g. `tool 20`".to_owned();
                            }
                        }
                    } else {
                        self.status = "usage: tool <dia_pix>".to_owned();
                    }
                }
                "mul" => {
                    if let Some(v) = it.next() {
                        match v.parse::<f32>() {
                            Ok(m) if m.is_finite() => {
                                self.params.mul = m;
                                self.dirty = true;
                                self.status = format!("mul set to {}", self.params.mul);
                            }
                            _ => self.status = "mul expects a finite f32, e.g. `mul 1.5`".to_owned(),
                        }
                    } else {
                        self.status = "usage: mul <f32>".to_owned();
                    }
                }
                "reset" => {
                    self.params.mul = 1.0;
                    self.set_applied_count(0);
                    self.tool_dia_pix = 20;
                    self.dirty = true;
                    self.status = "reset".to_owned();
                }
                "help" => {
                    self.status = "cmd: tp <i> | frame <n> | next | prev | first | last | tool <pix> | mul <f32> | reset | help".to_owned();
                }
                _ => {
                    self.status = format!("unknown cmd: {cmd} (try `help`)");
                }
            }
        }

        fn handle_hotkeys(&mut self, ctx: &egui::Context) {
            if ctx.wants_keyboard_input() {
                return;
            }

            let next_step = ctx.input(|i| {
                if i.key_pressed(egui::Key::ArrowRight) {
                    Some(if i.modifiers.shift { 20 } else { 1 })
                } else if i.key_pressed(egui::Key::PageDown) || i.key_pressed(egui::Key::N) {
                    Some(1)
                } else {
                    None
                }
            });
            if let Some(step) = next_step {
                self.step_applied(step);
            }

            let prev_step = ctx.input(|i| {
                if i.key_pressed(egui::Key::ArrowLeft) {
                    Some(if i.modifiers.shift { -20 } else { -1 })
                } else if i.key_pressed(egui::Key::PageUp) || i.key_pressed(egui::Key::P) {
                    Some(-1)
                } else {
                    None
                }
            });
            if let Some(step) = prev_step {
                self.step_applied(step);
            }
        }

        fn ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
            self.handle_hotkeys(ctx);
            self.render_if_needed(ctx);

            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(&self.title);
                    ui.separator();

                    let len = self.toolpath_len();
                    match self.active_toolpath_index() {
                        Some(i) => {
                            ui.monospace(format!(
                                "toolpath={i}/{last} (applied={}/{len})",
                                self.applied_count,
                                last = len.saturating_sub(1)
                            ));

                            if let Some(tp) = self.movie_toolpaths.get(i) {
                                ui.separator();
                                ui.monospace(format!(
                                    "cut_pixels={} cut_depth_sum_thou={}",
                                    tp.cuts.pixels_changed, tp.cuts.depth_sum_thou
                                ));
                            }
                        }
                        None => {
                            ui.monospace(format!("toolpath=none (applied=0/{len})"));
                        }
                    };

                    ui.separator();
                    ui.monospace(format!("mul={:.4}", self.params.mul));

                    ui.separator();
                    ui.monospace(format!("tool_dia_pix={}", self.tool_dia_pix));

                    if !self.hover_text.is_empty() {
                        ui.separator();
                        ui.monospace(&self.hover_text);
                    }
                });

                let w = self.sim.w;
                let h = self.sim.h;
                let Some(tex) = &self.texture else { return };

                let image_size = egui::vec2(w as f32, h as f32);
                let response = ui.add(egui::Image::new((tex.id(), image_size)));

                // Overlay the active toolpath polyline.
                if let Some(tp_i) = self.active_toolpath_index() {
                    if let Some(tp) = self.movie_toolpaths.get(tp_i) {
                        if tp.points.len() >= 2 {
                            let rect = response.rect;
                            let painter = ui.painter_at(rect);
                            let sx = rect.width() / (w as f32);
                            let sy = rect.height() / (h as f32);

                            let mut pts: Vec<egui::Pos2> = Vec::with_capacity(tp.points.len());
                            for p in &tp.points {
                                let px = p.x.clamp(0, (w.saturating_sub(1)) as i32) as f32;
                                let py = p.y.clamp(0, (h.saturating_sub(1)) as i32) as f32;
                                pts.push(egui::pos2(
                                    rect.left() + (px + 0.5) * sx,
                                    rect.top() + (py + 0.5) * sy,
                                ));
                            }

                            let stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 40, 40));
                            painter.add(egui::Shape::line(pts.clone(), stroke));

                            if let (Some(start), Some(end)) = (pts.first().copied(), pts.last().copied()) {
                                painter.circle_filled(start, 3.0, egui::Color32::from_rgb(40, 255, 40));
                                painter.circle_filled(end, 3.0, egui::Color32::from_rgb(40, 160, 255));
                            }
                        }
                    }
                }

                if response.hovered() {
                    if let Some(pos) = response.hover_pos() {
                        let rect = response.rect;
                        let fx = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 0.999_999);
                        let fy = ((pos.y - rect.top()) / rect.height()).clamp(0.0, 0.999_999);
                        let x = (fx * (w as f32)) as usize;
                        let y = (fy * (h as f32)) as usize;

                        let src = self.src_text_at(x, y);
                        let viz = self.rgba_text_at(x, y);
                        self.hover_text = format!("x={x} y={y} {src} {viz}");
                    }
                }

                ui.separator();
                ui.horizontal(|ui| {
                    ui.monospace("cmd>");
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.cmd)
                            .desired_width(f32::INFINITY)
                            .hint_text("tp 10 | next | prev | mul 2.0"),
                    );

                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let line = self.cmd.trim().to_owned();
                        self.cmd.clear();
                        self.apply_cmd(&line);
                    }
                });
                ui.monospace("hotkeys: ArrowLeft/ArrowRight (Shift=±20) or PgUp/PgDn or p/n");
                if !self.status.is_empty() {
                    ui.monospace(&self.status);
                }
            });
        }
    }

    // Single-window runners (implemented by using the same components)
    // -------------------------------------------------------------------------

    fn run_single_image(title: &str, src: SourceIm) -> Result<(), String> {
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size(egui::vec2(1200.0, 800.0)),
            ..Default::default()
        };
        let title_owned = title.to_owned();
        eframe::run_native(
            title,
            options,
            Box::new(move |_cc| Ok(Box::new(SingleImageApp::new(&title_owned, src.clone())))),
        )
        .map_err(|e| e.to_string())
    }

    struct SingleImageApp {
        viewer: ImageViewer,
    }

    impl SingleImageApp {
        fn new(title: &str, src: SourceIm) -> Self {
            Self {
                viewer: ImageViewer::new(title, src),
            }
        }
    }

    impl eframe::App for SingleImageApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(ctx, |ui| {
                self.viewer.ui(ctx, ui);
            });
            ctx.request_repaint();
        }
    }

    fn run_single_movie(title: &str, base: Lum16Im, toolpaths: Vec<ToolPath>, tool_dia_pix: usize) -> Result<(), String> {
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size(egui::vec2(1200.0, 800.0)),
            ..Default::default()
        };
        let title_owned = title.to_owned();
        eframe::run_native(
            title,
            options,
            Box::new(move |_cc| {
                Ok(Box::new(SingleMovieApp {
                    viewer: ToolpathMovieViewer::new(&title_owned, base.clone(), toolpaths.clone(), tool_dia_pix),
                }))
            }),
        )
        .map_err(|e| e.to_string())
    }

    struct SingleMovieApp {
        viewer: ToolpathMovieViewer,
    }

    impl eframe::App for SingleMovieApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(ctx, |ui| {
                self.viewer.ui(ctx, ui);
            });
            ctx.request_repaint();
        }
    }
}

/// No-op implementations when debug_ui feature is disabled or cli_only is enabled.
#[cfg(not(all(feature = "debug_ui", not(feature = "cli_only"))))]
mod imp {
    use crate::im::{Im, Lum16Im, RGBAIm};
    use crate::im::MaskIm;
    use crate::region_tree::{PlyIm, RegionIm};
    use crate::toolpath::ToolPath;

    pub fn init(_title: &str) {}

    pub fn add_u8_1<S>(_title: &str, _im: &Im<u8, 1, S>) {}
    pub fn add_u8_4<S>(_title: &str, _im: &Im<u8, 4, S>) {}
    pub fn add_u16_1<S>(_title: &str, _im: &Im<u16, 1, S>) {}

    pub fn add_mask_im(_title: &str, _im: &MaskIm) {}
    pub fn add_ply_im(_title: &str, _im: &PlyIm) {}
    pub fn add_region_im(_title: &str, _im: &RegionIm) {}

    pub fn add_rect(_l: usize, _t: usize, _r: usize, _b: usize) {}

    pub fn add_lum16(_title: &str, _im: &Lum16Im) {}
    pub fn add_rgba(_title: &str, _im: &RGBAIm) {}

    pub fn add_toolpath_movie(_title: &str, _base: &Lum16Im, _toolpaths: &[ToolPath], _tool_dia_pix: usize) {}

    pub fn show_u8_1<S>(_im: &Im<u8, 1, S>, _title: &str) -> Result<(), String> {
        Ok(())
    }
    pub fn show_u8_4<S>(_im: &Im<u8, 4, S>, _title: &str) -> Result<(), String> {
        Ok(())
    }
    pub fn show_u16_1<S>(_im: &Im<u16, 1, S>, _title: &str) -> Result<(), String> {
        Ok(())
    }

    pub fn show_toolpath_movie(_base: &Lum16Im, _toolpaths: &[ToolPath], _title: &str) -> Result<(), String> {
        Ok(())
    }

    pub fn show() {}
}

pub use imp::*;
