use super::core::{Im, RGBAIm};
use eframe::egui;

#[derive(Clone, Debug)]
struct SourceIm {
    w: usize,
    h: usize,
    pixels: SourcePixels,
}

#[derive(Clone, Debug)]
enum SourcePixels {
    U8_1 { arr: Vec<u8>, max: u8 },
    U8_4 { arr: Vec<u8> },
    U16_1 { arr: Vec<u16>, max: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VizMode {
    /// Grayscale visualization for 1-channel sources.
    /// Uses the max value present in the source image for normalization.
    GrayAutoMax,
    /// Passthrough visualization for RGBA8 sources.
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

            // If the user forces an incompatible mode, just clear to black.
            _ => {
                out_rgba.arr.fill(0);
                for i in (3..out_rgba.arr.len()).step_by(4) {
                    out_rgba.arr[i] = 255;
                }
            }
        }
    }
}

struct DebugImApp {
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

impl DebugImApp {
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

        self.src
            .render_to_rgba8(self.mode, self.params, &mut self.rgba);

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
            "mode" => {
                match it.next() {
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
                }
            }
            "help" => {
                self.status = "cmd: mul <f32> | reset | mode gray|rgba | help".to_owned();
            }
            _ => {
                self.status = format!("unknown cmd: {cmd} (try `help`)");
            }
        }
    }
}

impl eframe::App for DebugImApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render_if_needed(ctx);

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.title);
                ui.separator();
                ui.monospace(format!("mode={:?} mul={:.4}", self.mode, self.params.mul));
                if !self.hover_text.is_empty() {
                    ui.separator();
                    ui.monospace(&self.hover_text);
                }
            });
        });

        egui::TopBottomPanel::bottom("bottom").show(ctx, |ui| {
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

        egui::CentralPanel::default().show(ctx, |ui| {
            let w = self.src.w;
            let h = self.src.h;
            let Some(tex) = &self.texture else { return };

            // Render at 1:1 logical size; use nearest sampling.
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
        });

        // Keep repainting so hover text updates smoothly.
        ctx.request_repaint();
    }
}

pub fn show_u8_1<S>(im: &Im<u8, 1, S>, title: &str) -> Result<(), String> {
    let expected = im.w;
    if im.s < expected {
        return Err(format!(
            "invalid stride: s={} < w*N_CH={} (w={}, N_CH=1)",
            im.s, expected, im.w
        ));
    }

    // Pack to tightly-strided rows so debug indexing is always y*w + x.
    let mut packed = Vec::with_capacity(im.w * im.h);
    for y in 0..im.h {
        let row0 = y * im.s;
        let row = &im.arr[row0..row0 + im.w];
        packed.extend_from_slice(row);
    }

    let max = packed.iter().copied().max().unwrap_or(0);
    let src = SourceIm {
        w: im.w,
        h: im.h,
        pixels: SourcePixels::U8_1 { arr: packed, max },
    };
    run_app(title, src)
}

pub fn show_u8_4<S>(im: &Im<u8, 4, S>, title: &str) -> Result<(), String> {
    let expected = im.w * 4;
    if im.s < expected {
        return Err(format!(
            "invalid stride: s={} < w*N_CH={} (w={}, N_CH=4)",
            im.s, expected, im.w
        ));
    }

    let mut packed = Vec::with_capacity(im.w * im.h * 4);
    for y in 0..im.h {
        let row0 = y * im.s;
        let row = &im.arr[row0..row0 + im.w * 4];
        packed.extend_from_slice(row);
    }

    let src = SourceIm {
        w: im.w,
        h: im.h,
        pixels: SourcePixels::U8_4 { arr: packed },
    };
    run_app(title, src)
}

pub fn show_u16_1<S>(im: &Im<u16, 1, S>, title: &str) -> Result<(), String> {
    let expected = im.w;
    if im.s < expected {
        return Err(format!(
            "invalid stride: s={} < w*N_CH={} (w={}, N_CH=1)",
            im.s, expected, im.w
        ));
    }

    let mut packed = Vec::with_capacity(im.w * im.h);
    for y in 0..im.h {
        let row0 = y * im.s;
        let row = &im.arr[row0..row0 + im.w];
        packed.extend_from_slice(row);
    }

    let max = packed.iter().copied().max().unwrap_or(0);
    let src = SourceIm {
        w: im.w,
        h: im.h,
        pixels: SourcePixels::U16_1 { arr: packed, max },
    };
    run_app(title, src)
}

fn run_app(title: &str, src: SourceIm) -> Result<(), String> {
    let options = eframe::NativeOptions {
        // Let the OS choose sensible defaults; image is rendered at 1:1.
        ..Default::default()
    };

    let title_owned = title.to_owned();

    eframe::run_native(
        title,
        options,
        Box::new(move |_cc| Box::new(DebugImApp::new(&title_owned, src))),
    )
    .map_err(|e| e.to_string())
}
