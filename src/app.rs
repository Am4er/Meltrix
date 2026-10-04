use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use eframe::egui::{self, Align, Button, Color32, Layout, ProgressBar, RichText};

use crate::job::{DONE, FAIL, Job, Queue, STOP, Task, WAIT, WORK};
use crate::pic::{self, Fmt};
use crate::conv;

const RATES: [u32; 4] = [128, 192, 256, 320];

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Video,
    Pics,
}

pub struct App {
    tab: Tab,
    video: Queue,
    pics: Queue,
    rate: usize,
    fmt: Fmt,
    q: u8,
}

impl App {
    pub fn new() -> Self {
        Self { tab: Tab::Video, video: Queue::default(), pics: Queue::default(), rate: 1, fmt: Fmt::Webp, q: 85 }
    }

    fn drops(&mut self, ui: &egui::Ui) {
        let list: Vec<PathBuf> = ui.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if list.is_empty() {
            return;
        }
        let v = if self.video.busy() { 0 } else { self.video.add(&list, &conv::EXTS) };
        let p = if self.pics.busy() { 0 } else { self.pics.add(&list, &pic::EXTS) };
        if self.tab == Tab::Video && v == 0 && p > 0 {
            self.tab = Tab::Pics;
        } else if self.tab == Tab::Pics && p == 0 && v > 0 {
            self.tab = Tab::Video;
        }
    }

    fn video_ui(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        let busy = self.video.busy();
        let mut go = false;
        egui::Panel::top("video_opts").show(ui, |ui| {
            ui.add_space(6.0);
            route(ui, "MP4, MOV, M4V or MKV video", "MP3 audio");
            ui.add_space(6.0);
            egui::Grid::new("video_grid").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                ui.label("Bitrate");
                ui.add_enabled_ui(!busy, |ui| {
                    egui::ComboBox::from_id_salt("rate")
                        .selected_text(format!("{} kbps", RATES[self.rate]))
                        .show_ui(ui, |ui| {
                            for (i, r) in RATES.iter().enumerate() {
                                ui.selectable_value(&mut self.rate, i, format!("{r} kbps"));
                            }
                        })
                        .response
                        .on_hover_text("Higher means better sound and bigger files");
                });
                ui.end_row();

                ui.label("Save to");
                save_to(ui, frame, &mut self.video.out, busy, "same folder as each video");
                ui.end_row();
            });
            ui.add_space(8.0);
            go = actions(ui, frame, &mut self.video, busy, "Add videos", &conv::EXTS, "MP3");
            ui.add_space(6.0);
        });
        if go {
            let kbps = RATES[self.rate];
            let task: Task = Arc::new(move |src: &Path, out: &Path, stop: &AtomicBool, prog: &AtomicU32| conv::run(src, out, kbps, stop, prog));
            self.video.start("mp3", usize::MAX, task, ui.ctx());
        }
        status_bar(ui, "video_status", &self.video, busy);
        list(ui, "video_list", &mut self.video, busy, "MP3", "Drop videos here or press Add videos", "MP4, MOV, M4V and MKV. WebM only with Vorbis audio.");
    }

    fn pics_ui(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        let busy = self.pics.busy();
        let mut go = false;
        egui::Panel::top("pic_opts").show(ui, |ui| {
            ui.add_space(6.0);
            route(ui, "JPG, PNG, WebP, HEIC, AVIF, GIF, BMP, TIFF and more", self.fmt.name());
            ui.add_space(6.0);
            egui::Grid::new("pic_grid").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                ui.label("Convert to");
                ui.add_enabled_ui(!busy, |ui| {
                    ui.horizontal(|ui| {
                        for f in Fmt::ALL {
                            ui.selectable_value(&mut self.fmt, f, f.name()).on_hover_text(f.hint());
                        }
                    });
                });
                ui.end_row();

                ui.label("Quality");
                ui.add_enabled(!busy && self.fmt != Fmt::Png, egui::Slider::new(&mut self.q, 1..=100))
                    .on_hover_text("Higher means better quality and bigger files")
                    .on_disabled_hover_text(if self.fmt == Fmt::Png { "PNG is lossless" } else { "Can't change while converting" });
                ui.end_row();

                ui.label("Save to");
                save_to(ui, frame, &mut self.pics.out, busy, "same folder as each image");
                ui.end_row();
            });
            ui.add_space(8.0);
            go = actions(ui, frame, &mut self.pics, busy, "Add images", &pic::EXTS, self.fmt.name());
            ui.add_space(6.0);
        });
        if go {
            let (fmt, q) = (self.fmt, self.q);
            let task: Task = Arc::new(move |src: &Path, out: &Path, _: &AtomicBool, prog: &AtomicU32| pic::run(src, out, fmt, q, prog));
            self.pics.start(fmt.ext(), 8, task, ui.ctx());
        }
        status_bar(ui, "pic_status", &self.pics, busy);
        list(ui, "pic_list", &mut self.pics, busy, &self.fmt.ext().to_uppercase(), "Drop images or a folder here", "JPG, PNG, WebP, HEIC, AVIF, GIF, BMP, TIFF, ICO, TGA, QOI and PNM");
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.video.tidy();
        self.pics.tidy();
        self.drops(ui);
        let (vb, pb) = (self.video.busy(), self.pics.busy());

        egui::Panel::top("tabs").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                tab(ui, &mut self.tab, Tab::Video, "Video to MP3", vb);
                tab(ui, &mut self.tab, Tab::Pics, "Image to WebP / PNG / JPG", pb);
            });
            ui.add_space(4.0);
        });
        match self.tab {
            Tab::Video => self.video_ui(ui, frame),
            Tab::Pics => self.pics_ui(ui, frame),
        }
        overlay(ui, vb, pb);

        if vb || pb {
            ui.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.video.close();
        self.pics.close();
    }
}

fn tab(ui: &mut egui::Ui, cur: &mut Tab, t: Tab, text: &str, busy: bool) {
    let r = ui.add(Button::selectable(*cur == t, RichText::new(text).size(15.0)).min_size(egui::vec2(0.0, 28.0)));
    if busy {
        ui.spinner().on_hover_text("Converting");
    }
    if r.clicked() {
        *cur = t;
    }
}

fn hit(r: &egui::Response) -> bool {
    r.clicked() && !r.double_clicked() && !r.triple_clicked()
}

fn route(ui: &mut egui::Ui, from: &str, to: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.weak("From");
        ui.strong(from);
        ui.weak("to");
        ui.strong(to);
    });
}

fn save_to(ui: &mut egui::Ui, frame: &eframe::Frame, out: &mut Option<PathBuf>, busy: bool, none: &str) {
    ui.horizontal(|ui| {
        if ui.add_enabled(!busy, Button::new("Change...").small()).clicked()
            && let Some(d) = rfd::FileDialog::new().set_parent(frame).pick_folder()
        {
            *out = Some(d);
        }
        if out.is_some() && ui.add_enabled(!busy, Button::new("Reset").small()).on_hover_text(none).clicked() {
            *out = None;
        }
        match out.as_ref() {
            Some(d) => ui.add(egui::Label::new(d.display().to_string()).truncate()),
            None => ui.weak(none),
        };
    });
}

fn actions(ui: &mut egui::Ui, frame: &eframe::Frame, q: &mut Queue, busy: bool, add: &str, exts: &[&str], to: &str) -> bool {
    let mut go = false;
    ui.horizontal(|ui| {
        if ui.add_enabled(!busy, Button::new(format!("{add}..."))).clicked()
            && let Some(list) = rfd::FileDialog::new().add_filter(add.trim_start_matches("Add "), exts).set_parent(frame).pick_files()
        {
            q.add(&list, exts);
        }
        if ui.add_enabled(!busy && !q.jobs.is_empty(), Button::new("Clear list")).clicked() {
            q.jobs.clear();
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if busy {
                if hit(&ui.add(Button::new("Stop").min_size(egui::vec2(0.0, 26.0)))) {
                    q.stop();
                }
            } else {
                let n = q.left();
                let text = format!("Convert {n} {} to {to}", if n == 1 { "file" } else { "files" });
                if hit(&ui.add_enabled(n > 0, Button::new(RichText::new(text).strong()).min_size(egui::vec2(0.0, 26.0)))) {
                    go = true;
                }
            }
        });
    });
    go
}

fn status_bar(ui: &mut egui::Ui, id: &'static str, q: &Queue, busy: bool) {
    egui::Panel::bottom(id).show(ui, |ui| {
        ui.horizontal(|ui| {
            let (all, done, failed) = (q.jobs.len(), q.count(DONE), q.count(FAIL));
            ui.label(format!("{done} of {all} converted"));
            if failed > 0 {
                ui.colored_label(ui.visuals().error_fg_color, format!("{failed} failed"));
            }
            if busy {
                let sum: f32 = q.jobs.iter().map(|j| match j.state() {
                    WAIT => 0.0,
                    WORK => {
                        let p = j.prog.load(Ordering::Relaxed);
                        if p > 1000 { 0.0 } else { p as f32 / 1000.0 }
                    }
                    _ => 1.0,
                }).sum();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add(ProgressBar::new(sum / all.max(1) as f32).desired_width(160.0).show_percentage());
                    ui.weak("Converting...");
                });
            }
        });
    });
}

fn list(ui: &mut egui::Ui, id: &str, q: &mut Queue, busy: bool, to: &str, empty: &str, formats: &str) {
    egui::CentralPanel::default().show(ui, |ui| {
        if q.jobs.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.3);
                ui.heading(empty);
                ui.weak(formats);
            });
            return;
        }
        let mut gone = None;
        egui::ScrollArea::vertical().id_salt(id).auto_shrink(false).show(ui, |ui| {
            for (i, j) in q.jobs.iter().enumerate() {
                if row(ui, j, to, busy) {
                    gone = Some(i);
                }
            }
        });
        if let Some(i) = gone {
            q.jobs.remove(i);
        }
    });
}

fn row(ui: &mut egui::Ui, j: &Job, to: &str, busy: bool) -> bool {
    let state = j.state();
    let to = match state {
        DONE => j.out.lock().unwrap().extension().unwrap_or_default().to_string_lossy().to_uppercase(),
        _ => to.to_string(),
    };
    let mut gone = false;
    egui::Sides::new().shrink_left().truncate().show(
        ui,
        |ui| {
            ui.add(egui::Label::new(&j.name).show_tooltip_when_elided(false)).on_hover_text(j.src.display().to_string());
        },
        |ui| {
            gone = hit(&ui.add_enabled(!busy, Button::new("x").small()).on_hover_text("Remove from the list"));
            status(ui, j, state);
            ui.label(RichText::new(format!("{} -> {to}", j.from)).monospace().weak());
        },
    );
    gone
}

fn status(ui: &mut egui::Ui, j: &Job, state: u8) {
    ui.allocate_ui_with_layout(egui::vec2(200.0, 20.0), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_width(200.0);
        match state {
            WORK => {
                let p = j.prog.load(Ordering::Relaxed);
                let bar = if p > 1000 {
                    ProgressBar::new(0.0).animate(true)
                } else {
                    ProgressBar::new(p as f32 / 1000.0).show_percentage()
                };
                ui.add(bar.desired_width(200.0));
            }
            DONE => {
                let green = if ui.visuals().dark_mode { Color32::LIGHT_GREEN } else { Color32::DARK_GREEN };
                ui.colored_label(green, format!("Done, {}", size(j.out_size.load(Ordering::Relaxed))))
                    .on_hover_text(format!("Saved as {}", j.out.lock().unwrap().display()));
            }
            FAIL => {
                let msg = j.msg.lock().unwrap().clone();
                ui.add(egui::Label::new(RichText::new(&msg).color(ui.visuals().error_fg_color)).truncate().show_tooltip_when_elided(false))
                    .on_hover_text(msg);
            }
            STOP => {
                ui.weak("Stopped");
            }
            _ => {
                ui.weak("Waiting");
            }
        }
    });
}

fn size(b: u64) -> String {
    if b >= 1 << 20 { format!("{:.1} MB", b as f64 / 1048576.0) } else { format!("{} KB", b.div_ceil(1024)) }
}

fn overlay(ui: &egui::Ui, vb: bool, pb: bool) {
    if ui.input(|i| i.raw.hovered_files.is_empty()) {
        return;
    }
    let text = match (vb, pb) {
        (false, false) => "Drop to add",
        (true, false) => "Drop to add images",
        (false, true) => "Drop to add videos",
        (true, true) => "Both lists are busy",
    };
    let r = ui.content_rect();
    let p = ui.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop")));
    p.rect_filled(r, 0.0, Color32::from_black_alpha(190));
    p.text(r.center(), egui::Align2::CENTER_CENTER, text, egui::TextStyle::Heading.resolve(ui.style()), Color32::WHITE);
}
