//! egui application: the three-pane Lancer-standard layout.

use apktool_core::{build, decode, patches, BuildOptions, DecodeOptions};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};

/// Top-level GUI state.
pub struct LancerApp {
    apk_path: Option<PathBuf>,
    project_dir: Option<PathBuf>,
    log: Vec<String>,
    detail: String,
    preset: String,
    busy: bool,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

#[allow(dead_code)]
enum Msg {
    Line(String),
    Done(String),
}

impl Default for LancerApp {
    fn default() -> Self {
        let (tx, rx) = channel();
        LancerApp {
            apk_path: None,
            project_dir: None,
            log: vec!["Ready. Open an APK to begin.".into()],
            detail: String::new(),
            preset: "pin-bypass".into(),
            busy: false,
            tx,
            rx,
        }
    }
}

impl LancerApp {
    fn spawn<F>(&mut self, label: &str, f: F)
    where
        F: FnOnce() -> anyhow::Result<String> + Send + 'static,
    {
        self.busy = true;
        self.log.push(format!("▶ {label}…"));
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let msg = match f() {
                Ok(s) => Msg::Done(format!("✔ {s}")),
                Err(e) => Msg::Done(format!("✖ {e}")),
            };
            let _ = tx.send(msg);
        });
    }

    fn drain(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Line(l) => self.log.push(l),
                Msg::Done(l) => {
                    self.log.push(l);
                    self.busy = false;
                }
            }
        }
    }
}

impl eframe::App for LancerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain();
        ctx.request_repaint_after(std::time::Duration::from_millis(200));

        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open APK…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("APK", &["apk"]).pick_file() {
                            self.apk_path = Some(p.clone());
                            self.log.push(format!("Opened {}", p.display()));
                        }
                        ui.close_menu();
                    }
                    if ui.button("Open project folder…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().pick_folder() {
                            self.project_dir = Some(p.clone());
                            self.log.push(format!("Project: {}", p.display()));
                        }
                        ui.close_menu();
                    }
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        self.detail = format!(
                            "lancer-apktool {}\nRust clone of apktool with native GUI.",
                            env!("CARGO_PKG_VERSION")
                        );
                        ui.close_menu();
                    }
                });
                ui.separator();
                ui.label(if self.busy { "working…" } else { "idle" });
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Status: {}", if self.busy { "busy" } else { "idle" }));
                ui.separator();
                ui.label(format!("Log lines: {}", self.log.len()));
            });
        });

        egui::SidePanel::left("inputs").resizable(true).default_width(240.0).show(ctx, |ui| {
            ui.heading("Inputs");
            ui.separator();
            ui.label("APK:");
            ui.monospace(self.apk_path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "—".into()));
            ui.label("Project:");
            ui.monospace(self.project_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "—".into()));
            ui.separator();

            ui.add_enabled_ui(self.apk_path.is_some() && !self.busy, |ui| {
                if ui.button("Decode").clicked() {
                    if let Some(apk) = self.apk_path.clone() {
                        let opts = DecodeOptions { force: true, ..Default::default() };
                        self.project_dir = apk.file_stem().map(|s| PathBuf::from(s));
                        self.spawn("Decode", move || {
                            let out = decode::decode(&apk, &opts)?;
                            Ok(format!("decoded to {}", out.display()))
                        });
                    }
                }
            });

            ui.add_enabled_ui(self.project_dir.is_some() && !self.busy, |ui| {
                if ui.button("Build").clicked() {
                    if let Some(dir) = self.project_dir.clone() {
                        let opts = BuildOptions::default();
                        self.spawn("Build", move || {
                            let out = build::build(&dir, &opts)?;
                            Ok(format!("built {}", out.display()))
                        });
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("Preset:");
                    egui::ComboBox::from_id_source("preset")
                        .selected_text(self.preset.clone())
                        .show_ui(ui, |ui| {
                            for p in ["pin-bypass", "frida-gadget", "debuggable", "webview-debug", "nsc-bypass", "proxy-ca"] {
                                ui.selectable_value(&mut self.preset, p.to_string(), p);
                            }
                        });
                });
                if ui.button("Apply patch").clicked() {
                    if let (Some(dir), Some(preset)) = (self.project_dir.clone(), patches::Preset::parse(&self.preset)) {
                        self.spawn("Patch", move || {
                            let report = patches::apply(&dir, preset, &Default::default())?;
                            Ok(format!("patched {} file(s)", report.changed_files.len()))
                        });
                    }
                }
            });
        });

        egui::SidePanel::right("detail").resizable(true).default_width(320.0).show(ctx, |ui| {
            ui.heading("Detail");
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.monospace(&self.detail);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Activity");
            ui.separator();
            egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                for line in &self.log {
                    if ui.selectable_label(false, line).clicked() {
                        self.detail = line.clone();
                    }
                }
            });
        });
    }
}
