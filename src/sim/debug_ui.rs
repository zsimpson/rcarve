use crate::im::RGBAIm;
use crate::im::Lum16Im;
use crate::sim::sim_toolpaths;
use crate::toolpath::ToolPath;
use crate::toolpath::IV3;
use eframe::egui;

#[derive(Clone, Copy, Debug)]
struct VizParams {
    mul: f32,
}

struct ToolpathMovieApp {
    title: String,

    // Inputs
    base: Lum16Im,
    movie_toolpaths: Vec<ToolPath>,

    // Debug mode state
    segment_mode: bool,
    segment_toolpath: ToolPath,
    segment_mouse_xy: Option<(usize, usize)>,
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

impl ToolpathMovieApp {
    fn new(title: &str, base: Lum16Im, toolpaths: Vec<ToolPath>) -> Self {
        let w = base.w;
        let h = base.h;
        let sim = Lum16Im::new(w, h);
        let rgba = RGBAIm::new(w, h);

        let cx = w / 2;
        let cy = h / 2;
        let segment_toolpath = ToolPath {
            points: vec![
                IV3 {
                    x: cx as i32,
                    y: cy as i32,
                    z: 0,
                },
                IV3 {
                    x: cx as i32,
                    y: cy as i32,
                    z: 0,
                },
            ],
            tool_dia_pix: 20,
            tool_i: 0,
        };

        Self {
            title: title.to_owned(),
            base,
            movie_toolpaths: toolpaths,
            segment_mode: false,
            segment_toolpath,
            segment_mouse_xy: None,
            tool_dia_pix: 20,
            applied_count: 0,
            sim,
            rgba,
            params: VizParams { mul: 1.0 },
            texture: None,
            dirty: true,
            hover_text: String::new(),
            cmd: String::new(),
            status: "cmd: tp <i> | frame <n> | next | prev | first | last | tool <pix> | mode <movie|seg> | mul <f32> | reset | help".to_owned(),
        }
    }

    fn center_xy(&self) -> (usize, usize) {
        (self.sim.w / 2, self.sim.h / 2)
    }

    fn toolpath_len(&self) -> usize {
        if self.segment_mode {
            1
        } else {
            self.movie_toolpaths.len()
        }
    }

    fn active_toolpaths(&self) -> &[ToolPath] {
        if self.segment_mode {
            std::slice::from_ref(&self.segment_toolpath)
        } else {
            &self.movie_toolpaths
        }
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
        if self.segment_mode {
            return;
        }
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
            let tool_dia_pix = self.tool_dia_pix;

            if self.segment_mode {
                let n = self.applied_count.min(1);
                if n > 0 {
                    let toolpaths = std::slice::from_ref(&self.segment_toolpath);
                    sim_toolpaths(&mut self.sim, toolpaths, tool_dia_pix);
                }
            } else {
                let n = self.applied_count.min(self.movie_toolpaths.len());
                if n > 0 {
                    sim_toolpaths(&mut self.sim, &self.movie_toolpaths[..n], tool_dia_pix);
                }
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
        let Some(cmd) = it.next() else { return };

        match cmd {
            "tp" => {
                if self.segment_mode {
                    self.status = "tp is disabled in mode=seg".to_owned();
                    return;
                }
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
                if self.segment_mode {
                    self.status = "frame is disabled in mode=seg".to_owned();
                    return;
                }
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
            "mode" => {
                if let Some(v) = it.next() {
                    match v {
                        "seg" | "segment" => {
                            self.segment_mode = true;
                            self.applied_count = 1;
                            let (cx, cy) = self.center_xy();
                            self.segment_toolpath.points[0].x = cx as i32;
                            self.segment_toolpath.points[0].y = cy as i32;
                            self.segment_toolpath.points[1].x = cx as i32;
                            self.segment_toolpath.points[1].y = cy as i32;
                            self.segment_mouse_xy = None;
                            self.dirty = true;
                            self.status = "mode=seg (center->mouse)".to_owned();
                        }
                        "movie" => {
                            self.segment_mode = false;
                            self.set_applied_count(0);
                            self.dirty = true;
                            self.status = "mode=movie".to_owned();
                        }
                        _ => {
                            self.status = "mode expects `movie` or `seg`".to_owned();
                        }
                    }
                } else {
                    self.status = "usage: mode <movie|seg>".to_owned();
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
                self.segment_mode = false;
                self.dirty = true;
                self.status = "reset".to_owned();
            }
            "help" => {
                self.status = "cmd: tp <i> | frame <n> | next | prev | first | last | tool <pix> | mode <movie|seg> | mul <f32> | reset | help".to_owned();
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

        if self.segment_mode {
            return;
        }

        let next = ctx.input(|i| {
            i.key_pressed(egui::Key::ArrowRight)
                || i.key_pressed(egui::Key::PageDown)
                || i.key_pressed(egui::Key::N)
        });
        if next {
            self.step_applied(1);
        }

        let prev = ctx.input(|i| {
            i.key_pressed(egui::Key::ArrowLeft)
                || i.key_pressed(egui::Key::PageUp)
                || i.key_pressed(egui::Key::P)
        });
        if prev {
            self.step_applied(-1);
        }
    }
}

impl eframe::App for ToolpathMovieApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_hotkeys(ctx);
        self.render_if_needed(ctx);

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.title);
                ui.separator();

                let len = self.toolpath_len();
                match self.active_toolpath_index() {
                    Some(i) => ui.monospace(format!("toolpath={i}/{last} (applied={}/{len})", self.applied_count, last = len.saturating_sub(1))),
                    None => ui.monospace(format!("toolpath=none (applied=0/{len})")),
                };

                ui.separator();
                ui.monospace(format!("mul={:.4}", self.params.mul));

                ui.separator();
                ui.monospace(format!(
                    "tool_dia_pix={} mode={}",
                    self.tool_dia_pix,
                    if self.segment_mode { "seg" } else { "movie" }
                ));

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
                        .hint_text("tp 10 | next | prev | mul 2.0"),
                );

                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let line = self.cmd.trim().to_owned();
                    self.cmd.clear();
                    self.apply_cmd(&line);
                }
            });
            ui.monospace("hotkeys: ArrowLeft/ArrowRight or PgUp/PgDn or p/n");
            if !self.status.is_empty() {
                ui.monospace(&self.status);
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let w = self.sim.w;
            let h = self.sim.h;
            let Some(tex) = &self.texture else { return };

            let image_size = egui::vec2(w as f32, h as f32);
            let response = ui.add(egui::Image::new((tex.id(), image_size)));

            // Overlay the active toolpath polyline.
            if let Some(tp_i) = self.active_toolpath_index() {
                let toolpaths = self.active_toolpaths();
                if let Some(tp) = toolpaths.get(tp_i) {
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

                    if self.segment_mode {
                        let (cx, cy) = self.center_xy();
                        let new_xy = Some((x, y));
                        if self.segment_mouse_xy != new_xy {
                            self.segment_mouse_xy = new_xy;
                            self.segment_toolpath.points[0].x = cx as i32;
                            self.segment_toolpath.points[0].y = cy as i32;
                            self.segment_toolpath.points[1].x = x as i32;
                            self.segment_toolpath.points[1].y = y as i32;
                            self.applied_count = 1;
                            self.dirty = true;
                        }
                    }
                }
            }

        });

        // If segment mode updated in-panel, regenerate the texture immediately.
        if self.segment_mode && self.dirty {
            self.render_if_needed(ctx);
        }

        ctx.request_repaint();
    }
}

pub fn show_toolpath_movie(base: &Lum16Im, toolpaths: &[ToolPath], title: &str) -> Result<(), String> {
    if base.s < base.w {
        return Err(format!(
            "invalid stride: s={} < w={} (Lum16Im)",
            base.s, base.w
        ));
    }

    // Pack/copy so we own the memory for the app.
    let mut packed = Lum16Im::new(base.w, base.h);
    for y in 0..base.h {
        let row0 = y * base.s;
        let row = &base.arr[row0..row0 + base.w];
        packed.arr[y * packed.s..y * packed.s + base.w].copy_from_slice(row);
    }

    let options = eframe::NativeOptions { ..Default::default() };
    let title_owned = title.to_owned();
    let toolpaths_owned = toolpaths.to_vec();

    eframe::run_native(
        title,
        options,
        Box::new(move |_cc| {
            Box::new(ToolpathMovieApp::new(
                &title_owned,
                packed,
                toolpaths_owned,
            ))
        }),
    )
    .map_err(|e| e.to_string())
}
