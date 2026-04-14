mod config;
mod db;
mod indexer;
mod parser;
mod scanner;
mod server;

use eframe::egui;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

#[derive(Debug, Clone)]
enum OutputLine {
    Stdout(String),
    Stderr(String),
    Exit(i32),
}

struct GuiApp {
    project_path: String,
    status: String,
    output: String,
    running: bool,
    output_rx: Option<Receiver<OutputLine>>,
    stop_tx: Option<Sender<()>>,
}

impl Default for GuiApp {
    fn default() -> Self {
        Self {
            project_path: std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string()),
            status: "Idle".to_string(),
            output: String::new(),
            running: false,
            output_rx: None,
            stop_tx: None,
        }
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let recv = self.output_rx.take();
        if let Some(rx) = recv {
            let mut set_none = false;
            while let Ok(line) = rx.try_recv() {
                match line {
                    OutputLine::Stdout(s) | OutputLine::Stderr(s) => {
                        self.output.push_str(&s);
                    }
                    OutputLine::Exit(code) => {
                        self.running = false;
                        self.status = if code == 0 {
                            "Idle".to_string()
                        } else {
                            format!("Failed (exit {})", code)
                        };
                        set_none = true;
                    }
                }
            }
            if !set_none {
                self.output_rx = Some(rx);
            } else {
                self.stop_tx = None;
            }
            ctx.request_repaint();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Codebase Context Graph");
            ui.label("Choose a folder, run graph actions, and inspect generated context files.");
            ui.add_space(10.0);

            ui.group(|ui| {
                ui.label("Project Root");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.project_path);
                    if ui.button("Browse…").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.project_path = path.to_string_lossy().to_string();
                        }
                    }
                });
            });

            ui.add_space(10.0);

            ui.group(|ui| {
                ui.label("Actions");
                ui.horizontal(|ui| {
                    let buttons = ["Init", "Index", "Smoke", "Serve", "Watch"];
                    for name in buttons {
                        if ui
                            .add_enabled(!self.running, egui::Button::new(name))
                            .clicked()
                        {
                            self.run_command(name.to_lowercase());
                        }
                    }
                    if ui
                        .add_enabled(self.running, egui::Button::new("Stop"))
                        .clicked()
                    {
                        self.stop_command();
                    }
                });
            });

            ui.add_space(10.0);

            ui.group(|ui| {
                ui.label("Context Files");
                ui.horizontal(|ui| {
                    if ui.button("Open Graph").clicked() {
                        self.open_file("graph.db");
                    }
                    if ui.button("Open Config").clicked() {
                        self.open_file("config.json");
                    }
                    if ui.button("Reveal Folder").clicked() {
                        self.reveal_folder();
                    }
                });
            });

            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.label("Status:");
                ui.label(&self.status);
                if ui.button("Clear Output").clicked() {
                    self.output.clear();
                }
            });

            ui.add_space(10.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.monospace(&self.output);
            });
        });
    }
}

impl GuiApp {
    fn run_command(&mut self, command: String) {
        let project = self.project_path.clone();
        self.running = true;
        self.status = format!("Running {}…", command);
        self.output.push_str(&format!(
            "$ codebase-context-graph {} --project-root {}\n",
            command, project
        ));

        let (output_tx, output_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();
        self.output_rx = Some(output_rx);
        self.stop_tx = Some(stop_tx);

        let binary = std::env::current_exe().unwrap();
        let parent = binary.parent().unwrap().join("codebase-context-graph");
        let binary_path = if parent.exists() { parent } else { binary };

        thread::spawn(move || {
            let mut child = Command::new(&binary_path)
                .arg(&command)
                .arg("--project-root")
                .arg(&project)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("Failed to start process");

            let stdout = child.stdout.take().unwrap();
            let stderr = child.stderr.take().unwrap();

            let stdout_tx = output_tx.clone();
            thread::spawn(move || {
                use std::io::BufRead;
                for line in std::io::BufReader::new(stdout).lines() {
                    if let Ok(l) = line {
                        let _ = stdout_tx.send(OutputLine::Stdout(format!("{}\n", l)));
                    }
                }
            });

            let stderr_tx = output_tx.clone();
            thread::spawn(move || {
                use std::io::BufRead;
                for line in std::io::BufReader::new(stderr).lines() {
                    if let Ok(l) = line {
                        let _ = stderr_tx.send(OutputLine::Stderr(format!("{}\n", l)));
                    }
                }
            });

            loop {
                if stop_rx.try_recv().is_ok() {
                    let _ = child.kill();
                    break;
                }
                match child.try_wait() {
                    Ok(Some(status)) => {
                        let _ = output_tx.send(OutputLine::Exit(status.code().unwrap_or(-1)));
                        break;
                    }
                    Ok(None) => {
                        thread::sleep(std::time::Duration::from_millis(50));
                    }
                    Err(_) => {
                        let _ = output_tx.send(OutputLine::Exit(-1));
                        break;
                    }
                }
            }
        });
    }

    fn stop_command(&mut self) {
        if let Some(tx) = &self.stop_tx {
            let _ = tx.send(());
        }
        self.running = false;
        self.status = "Stopped".to_string();
    }

    fn open_file(&self, name: &str) {
        let path = PathBuf::from(&self.project_path)
            .join(".codebase-context")
            .join(name);
        if path.exists() {
            let _ = Command::new("open").arg(&path).spawn();
        }
    }

    fn reveal_folder(&self) {
        let path = PathBuf::from(&self.project_path).join(".codebase-context");
        if path.exists() {
            let _ = Command::new("open").arg(&path).spawn();
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([860.0, 620.0])
            .with_min_inner_size([720.0, 480.0])
            .with_title("Codebase Context Graph"),
        ..Default::default()
    };

    eframe::run_native(
        "Codebase Context Graph",
        options,
        Box::new(|_cc| Ok(Box::new(GuiApp::default()))),
    )
}
