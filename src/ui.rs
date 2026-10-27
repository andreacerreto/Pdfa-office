//! Native, offline desktop interface. Conversion runs on a separate worker thread.
use crate::{
    engine::{convert_pdf, discover_resources},
    model::{ConversionOptions, ConversionResult, EnginePaths, JobProgress, PdfAStandard},
};
use eframe::egui::{self, Align, Color32, FontId, Layout, RichText, Stroke, TextStyle, Vec2};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
        Arc,
    },
    time::Duration,
};

const NAVY: Color32 = Color32::from_rgb(25, 44, 76);
const BLUE: Color32 = Color32::from_rgb(35, 91, 190);
const BACKGROUND: Color32 = Color32::from_rgb(244, 247, 251);
const LINE: Color32 = Color32::from_rgb(223, 230, 239);
const MUTED: Color32 = Color32::from_rgb(91, 107, 128);
const GREEN: Color32 = Color32::from_rgb(22, 117, 83);
const RED: Color32 = Color32::from_rgb(167, 46, 52);
const SETTINGS_KEY: &str = "pdfa-office-settings-v1";

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    output_dir: PathBuf,
    standard: PdfAStandard,
    resource_dir: Option<PathBuf>,
}

enum Status {
    Pending,
    Working(JobProgress),
    Done(ConversionResult),
    Failed(String),
    Cancelled,
}

impl Status {
    fn can_run(&self) -> bool {
        matches!(self, Self::Pending | Self::Failed(_) | Self::Cancelled)
    }

    fn label(&self) -> String {
        match self {
            Self::Pending => "In attesa".into(),
            Self::Working(stage) => stage.to_string(),
            Self::Done(result) => format!("Conforme · {}", result.standard),
            Self::Failed(_) => "Non completato".into(),
            Self::Cancelled => "Annullato".into(),
        }
    }

    fn color(&self) -> Color32 {
        match self {
            Self::Done(_) => GREEN,
            Self::Failed(_) => RED,
            Self::Working(_) => BLUE,
            _ => MUTED,
        }
    }
}

struct QueueItem {
    id: u64,
    input: PathBuf,
    status: Status,
}

enum WorkerEvent {
    Stage(u64, JobProgress),
    Finished(u64, Result<ConversionResult, String>, bool),
    End,
}

pub struct PdfaApp {
    settings: Settings,
    engines: Result<EnginePaths, String>,
    queue: Vec<QueueItem>,
    next_id: u64,
    running: bool,
    cancel: Arc<AtomicBool>,
    receiver: Option<Receiver<WorkerEvent>>,
    allow_signed_copy: bool,
    notice: Option<String>,
    batch_total: usize,
    batch_finished: usize,
    batch_success: usize,
    batch_failed: usize,
    close_after_cancel: bool,
}

impl PdfaApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_style(&cc.egui_ctx);
        let settings: Settings = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, SETTINGS_KEY))
            .unwrap_or_default();
        let engines = discover_resources(settings.resource_dir.as_deref())
            .map_err(|error| format!("{error:#}"));
        Self {
            settings,
            engines,
            queue: Vec::new(),
            next_id: 1,
            running: false,
            cancel: Arc::new(AtomicBool::new(false)),
            receiver: None,
            allow_signed_copy: false,
            notice: None,
            batch_total: 0,
            batch_finished: 0,
            batch_success: 0,
            batch_failed: 0,
            close_after_cancel: false,
        }
    }

    fn add_files(&mut self, paths: Vec<PathBuf>) {
        if self.running {
            return;
        }
        let mut skipped = 0;
        let mut duplicates = 0;
        for input in paths {
            if !input.is_file()
                || !input
                    .extension()
                    .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("pdf"))
            {
                skipped += 1;
                continue;
            }
            let canonical = input.canonicalize().unwrap_or_else(|_| input.clone());
            if self.queue.iter().any(|item| {
                item.input
                    .canonicalize()
                    .unwrap_or_else(|_| item.input.clone())
                    == canonical
            }) {
                duplicates += 1;
                continue;
            }
            self.queue.push(QueueItem {
                id: self.next_id,
                input,
                status: Status::Pending,
            });
            self.next_id += 1;
        }
        self.notice = if skipped > 0 {
            Some(format!(
                "Ignorati {skipped} elementi: aggiungi singoli file con estensione .pdf."
            ))
        } else if duplicates > 0 {
            Some(format!("{duplicates} file erano già presenti nella coda."))
        } else {
            None
        };
    }

    fn pick_files(&mut self) {
        if let Some(files) = rfd::FileDialog::new()
            .set_title("Scegli i PDF da convertire")
            .add_filter("Documenti PDF", &["pdf"])
            .pick_files()
        {
            self.add_files(files);
        }
    }

    fn pick_output_dir(&mut self) {
        let mut dialog =
            rfd::FileDialog::new().set_title("Cartella per i PDF/A e i rapporti di verifica");
        if self.settings.output_dir.is_dir() {
            dialog = dialog.set_directory(&self.settings.output_dir);
        }
        if let Some(dir) = dialog.pick_folder() {
            self.settings.output_dir = dir;
            self.notice = None;
        }
    }

    fn start_batch(&mut self, ctx: &egui::Context) {
        if self.running {
            return;
        }
        let paths = match &self.engines {
            Ok(paths) => paths.clone(),
            Err(_) => {
                self.notice =
                    Some("Completa l'installazione dei componenti prima di convertire.".into());
                return;
            }
        };
        if self.settings.output_dir.as_os_str().is_empty() || !self.settings.output_dir.is_dir() {
            self.notice = Some("Scegli una cartella di destinazione esistente.".into());
            return;
        }
        let jobs: Vec<_> = self
            .queue
            .iter_mut()
            .filter(|item| item.status.can_run())
            .map(|item| {
                item.status = Status::Pending;
                (item.id, item.input.clone())
            })
            .collect();
        if jobs.is_empty() {
            return;
        }
        let options = ConversionOptions {
            standard: self.settings.standard,
            output_dir: self.settings.output_dir.clone(),
            allow_signed_copy: self.allow_signed_copy,
            timeout: Duration::from_secs(600),
        };
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.running = true;
        self.batch_total = jobs.len();
        self.batch_finished = 0;
        self.batch_success = 0;
        self.batch_failed = 0;
        self.notice = None;
        let cancel = self.cancel.clone();
        let context = ctx.clone();
        std::thread::spawn(move || {
            for (id, input) in jobs {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let progress_sender = sender.clone();
                let progress_context = context.clone();
                let result = convert_pdf(&paths, &input, &options, &cancel, |stage| {
                    let _ = progress_sender.send(WorkerEvent::Stage(id, stage));
                    progress_context.request_repaint();
                })
                .map_err(|error| format!("{error:#}"));
                let cancelled = cancel.load(Ordering::Relaxed) && result.is_err();
                let _ = sender.send(WorkerEvent::Finished(id, result, cancelled));
                context.request_repaint();
            }
            let _ = sender.send(WorkerEvent::End);
            context.request_repaint();
        });
    }

    fn poll_worker(&mut self) {
        let mut events = Vec::new();
        let mut disconnected = false;
        if let Some(receiver) = &self.receiver {
            loop {
                match receiver.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        let mut ended = false;
        for event in events {
            match event {
                WorkerEvent::Stage(id, stage) => {
                    if let Some(item) = self.queue.iter_mut().find(|item| item.id == id) {
                        item.status = Status::Working(stage);
                    }
                }
                WorkerEvent::Finished(id, result, cancelled) => {
                    self.batch_finished += 1;
                    let status = match result {
                        Ok(result) => {
                            self.batch_success += 1;
                            Status::Done(result)
                        }
                        Err(_) if cancelled => Status::Cancelled,
                        Err(error) => {
                            self.batch_failed += 1;
                            Status::Failed(error)
                        }
                    };
                    if let Some(item) = self.queue.iter_mut().find(|item| item.id == id) {
                        item.status = status;
                    }
                }
                WorkerEvent::End => ended = true,
            }
        }
        if ended {
            self.running = false;
            self.receiver = None;
            self.notice = Some(if self.cancel.load(Ordering::Relaxed) {
                format!("Operazione interrotta. {} file conformi salvati; puoi riprendere i file rimanenti.", self.batch_success)
            } else {
                format!(
                    "Elaborazione terminata: {} file conformi, {} non completati.",
                    self.batch_success, self.batch_failed
                )
            });
        } else if disconnected && self.running {
            self.running = false;
            self.receiver = None;
            for item in &mut self.queue {
                if matches!(item.status, Status::Working(_)) {
                    item.status = Status::Failed(
                        "Il processo di lavoro si è interrotto. Riprova il documento.".into(),
                    );
                }
            }
            self.notice = Some(
                "Il processo di lavoro si è interrotto. Gli originali restano invariati.".into(),
            );
        }
    }

    fn header(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .inner_margin(egui::Margin::symmetric(28, 20)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    egui::Frame::new()
                        .fill(NAVY)
                        .corner_radius(12)
                        .inner_margin(egui::Margin::symmetric(14, 11))
                        .show(ui, |ui| {
                            ui.label(RichText::new("A").size(30.0).strong().color(Color32::WHITE));
                        });
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("PDF/A Office")
                                .size(28.0)
                                .strong()
                                .color(NAVY),
                        );
                        ui.label(
                            RichText::new("Converti. Verifica. Archivia.")
                                .size(14.0)
                                .color(MUTED),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        badge(
                            ui,
                            "Elaborazione locale",
                            GREEN,
                            Color32::from_rgb(232, 246, 239),
                        );
                    });
                });
            });
    }

    fn footer(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("footer").frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(egui::Margin::symmetric(28, 15))).show(ctx, |ui| {
            if self.running {
                let fraction = self.batch_finished as f32 / self.batch_total.max(1) as f32;
                ui.add(egui::ProgressBar::new(fraction).animate(true).fill(BLUE).text(format!("{} / {} file elaborati", self.batch_finished, self.batch_total)));
                ui.add_space(9.0);
            }
            if let Some(notice) = &self.notice {
                ui.label(RichText::new(notice).size(13.0).color(MUTED));
                ui.add_space(7.0);
            }
            let remaining = self.queue.iter().filter(|item| item.status.can_run()).count();
            ui.horizontal(|ui| {
                ui.label(RichText::new(if self.running { "Conversione e verifica in corso".to_owned() } else { format!("{remaining} file da elaborare") }).strong().color(NAVY));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if self.running {
                        let requested = self.cancel.load(Ordering::Relaxed);
                        if ui.add_enabled(!requested, egui::Button::new(if requested { "Interruzione in corso..." } else { "Interrompi" }).min_size(Vec2::new(165.0, 42.0))).clicked() {
                            self.cancel.store(true, Ordering::Relaxed);
                        }
                    } else {
                        let ready = remaining > 0 && self.engines.is_ok() && self.settings.output_dir.is_dir();
                        if ui.add_enabled(ready, egui::Button::new(RichText::new("Converti e verifica").color(Color32::WHITE).strong()).fill(BLUE).min_size(Vec2::new(190.0, 42.0))).clicked() {
                            self.start_batch(ctx);
                        }
                    }
                    if ui.add_enabled(!self.settings.output_dir.as_os_str().is_empty(), egui::Button::new("Apri destinazione").min_size(Vec2::new(145.0, 42.0))).clicked() {
                        if let Err(error) = open_path(&self.settings.output_dir) { self.notice = Some(error); }
                    }
                });
            });
            ui.add_space(10.0);
            ui.label(RichText::new("Ogni risultato include il rapporto di validazione. Gli originali non vengono sovrascritti.").size(12.0).color(MUTED));
            ui.label(RichText::new("Controlla anche l'aspetto della copia. PDF cifrati e documenti con allegati incorporati non sono supportati.").size(12.0).color(MUTED));
        });
    }

    fn settings_card(&mut self, ui: &mut egui::Ui) {
        card().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Formato di destinazione").strong().color(NAVY));
                    ui.label(RichText::new("Scegli il profilo accettato dal destinatario.").size(12.0).color(MUTED));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_enabled_ui(!self.running, |ui| {
                        for standard in PdfAStandard::ALL.into_iter().rev() {
                            let selected = standard == self.settings.standard;
                            if ui.add(egui::Button::new(RichText::new(standard.to_string()).strong().color(if selected { Color32::WHITE } else { NAVY })).fill(if selected { BLUE } else { BACKGROUND }).min_size(Vec2::new(110.0, 36.0))).clicked() {
                                self.settings.standard = standard;
                            }
                        }
                    });
                });
            });
            ui.add_space(8.0);
            ui.label(RichText::new(self.settings.standard.description()).size(13.0).color(MUTED));
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Salva in").strong().color(NAVY));
                let dir = if self.settings.output_dir.as_os_str().is_empty() { "Scegli una cartella di destinazione".into() } else { self.settings.output_dir.display().to_string() };
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add_enabled(!self.running, egui::Button::new("Scegli cartella...")).clicked() { self.pick_output_dir(); }
                    ui.add(egui::Label::new(RichText::new(dir).size(13.0).color(MUTED)).truncate()).on_hover_text(self.settings.output_dir.display().to_string());
                });
            });
            ui.add_space(6.0);
            ui.add_enabled_ui(!self.running, |ui| {
                ui.checkbox(&mut self.allow_signed_copy, RichText::new("Consenti copie di PDF firmati: la firma digitale non sarà valida nella copia.").size(12.0));
            });
        });
    }

    fn readiness(&mut self, ui: &mut egui::Ui) {
        if let Err(error) = &self.engines {
            let error = error.clone();
            egui::Frame::new().fill(Color32::from_rgb(255, 245, 230)).stroke(Stroke::new(1.0_f32, Color32::from_rgb(235, 210, 168))).corner_radius(10).inner_margin(12).show(ui, |ui| {
                ui.label(RichText::new("Componenti di conversione non disponibili").strong().color(NAVY));
                ui.label("Reinstalla PDF/A Office con il suo installer completo oppure indica la cartella resources di un'installazione esistente.");
                egui::CollapsingHeader::new("Dettagli tecnici").show(ui, |ui| { ui.label(RichText::new(error).small()); });
                ui.horizontal(|ui| {
                    if ui.button("Indica cartella resources...").clicked() {
                        if let Some(dir) = rfd::FileDialog::new().set_title("Scegli la cartella resources di PDF/A Office").pick_folder() {
                            self.settings.resource_dir = Some(dir);
                            self.engines = discover_resources(self.settings.resource_dir.as_deref()).map_err(|error| format!("{error:#}"));
                        }
                    }
                    if ui.button("Rileva di nuovo").clicked() {
                        self.engines = discover_resources(self.settings.resource_dir.as_deref()).map_err(|error| format!("{error:#}"));
                    }
                    if self.settings.resource_dir.is_some() && ui.button("Usa componenti inclusi").clicked() {
                        self.settings.resource_dir = None;
                        self.engines = discover_resources(None).map_err(|error| format!("{error:#}"));
                    }
                });
            });
            ui.add_space(12.0);
        }
    }

    fn queue_area(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Documenti").size(17.0).strong().color(NAVY));
            ui.label(
                RichText::new(format!("{} file", self.queue.len()))
                    .size(13.0)
                    .color(MUTED),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_enabled_ui(!self.running, |ui| {
                    if ui.button("Aggiungi PDF...").clicked() {
                        self.pick_files();
                    }
                    if ui
                        .add_enabled(!self.queue.is_empty(), egui::Button::new("Svuota coda"))
                        .clicked()
                    {
                        self.queue.clear();
                        self.notice = None;
                    }
                    if self
                        .queue
                        .iter()
                        .any(|item| matches!(item.status, Status::Done(_)))
                        && ui.button("Rimuovi completati").clicked()
                    {
                        self.queue
                            .retain(|item| !matches!(item.status, Status::Done(_)));
                    }
                });
            });
        });
        ui.add_space(8.0);
        if self.queue.is_empty() {
            let response = card()
                .show(ui, |ui| {
                    ui.set_min_height(130.0);
                    ui.set_min_width(ui.available_width());
                    ui.vertical_centered(|ui| {
                        ui.add_space(23.0);
                        ui.label(
                            RichText::new("Trascina qui i tuoi PDF")
                                .size(22.0)
                                .strong()
                                .color(NAVY),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("oppure usa Aggiungi PDF per selezionare più documenti")
                                .color(MUTED),
                        );
                        ui.add_space(9.0);
                        ui.label(
                            RichText::new(
                                "I documenti restano sul computer. Nessun caricamento online.",
                            )
                            .small()
                            .color(MUTED),
                        );
                    });
                })
                .response;
            response.on_hover_text("Trascina uno o più file .pdf nella finestra");
            return;
        }
        let mut remove = None;
        let mut open = None;
        egui::ScrollArea::vertical()
            .id_salt("file_queue")
            .max_height(ui.available_height())
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for item in &self.queue {
                    card().show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            badge(ui, "PDF", BLUE, Color32::from_rgb(232, 239, 253));
                            ui.add_space(6.0);
                            let filename =
                                item.input.file_name().unwrap_or_default().to_string_lossy();
                            let filename_width = (ui.available_width() - 440.0).max(145.0);
                            ui.add_sized(
                                [filename_width, 24.0],
                                egui::Label::new(RichText::new(filename).strong().color(NAVY))
                                    .truncate(),
                            )
                            .on_hover_text(item.input.display().to_string());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .add_enabled(
                                        !self.running,
                                        egui::Button::new("Rimuovi").small(),
                                    )
                                    .clicked()
                                {
                                    remove = Some(item.id);
                                }
                                if let Status::Done(result) = &item.status {
                                    if ui.small_button("Apri PDF").clicked() {
                                        open = Some(result.output_path.clone());
                                    }
                                }
                                ui.label(
                                    RichText::new(item.status.label())
                                        .size(12.0)
                                        .strong()
                                        .color(item.status.color()),
                                );
                                if matches!(item.status, Status::Working(_)) {
                                    ui.spinner();
                                }
                            });
                        });
                        if let Status::Failed(error) = &item.status {
                            let first = error.lines().next().unwrap_or(error);
                            ui.add_space(4.0);
                            ui.add(
                                egui::Label::new(RichText::new(first).size(12.0).color(RED))
                                    .truncate(),
                            );
                        }
                        match &item.status {
                            Status::Done(result) => {
                                egui::CollapsingHeader::new("Risultato e rapporto")
                                    .id_salt(item.id)
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(format!(
                                                "{} pagine · {:.1} secondi",
                                                result.page_count,
                                                result.elapsed_ms as f64 / 1000.0
                                            ))
                                            .small()
                                            .color(MUTED),
                                        );
                                        ui.label(
                                            RichText::new(result.output_path.display().to_string())
                                                .small(),
                                        );
                                        for warning in &result.warnings {
                                            ui.label(RichText::new(warning).small().color(MUTED));
                                        }
                                        ui.horizontal(|ui| {
                                            if ui.small_button("Apri rapporto XML").clicked() {
                                                open = Some(result.report_path.clone());
                                            }
                                            if ui.small_button("Apri ricevuta JSON").clicked() {
                                                open = Some(result.receipt_path.clone());
                                            }
                                        });
                                    });
                            }
                            Status::Failed(error) => {
                                egui::CollapsingHeader::new("Dettagli dell'errore")
                                    .id_salt(item.id)
                                    .show(ui, |ui| {
                                        ui.label(RichText::new(error).small());
                                    });
                            }
                            _ => {}
                        }
                    });
                    ui.add_space(7.0);
                }
            });
        if let Some(id) = remove {
            self.queue.retain(|item| item.id != id);
        }
        if let Some(path) = open {
            if let Err(error) = open_path(&path) {
                self.notice = Some(error);
            }
        }
    }
}

impl eframe::App for PdfaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker();
        if ctx.input(|input| input.viewport().close_requested()) && self.running {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.cancel.store(true, Ordering::Relaxed);
            self.close_after_cancel = true;
            self.notice = Some("Interruzione della conversione prima della chiusura...".into());
        }
        if self.close_after_cancel && !self.running {
            self.close_after_cancel = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let dropped: Vec<_> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect()
        });
        if !dropped.is_empty() {
            self.add_files(dropped);
        }
        self.header(ctx);
        self.footer(ctx);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(BACKGROUND)
                    .inner_margin(egui::Margin::symmetric(28, 18)),
            )
            .show(ctx, |ui| {
                if self.engines.is_err() {
                    egui::ScrollArea::vertical()
                        .id_salt("setup_content")
                        .show(ui, |ui| {
                            self.readiness(ui);
                            self.settings_card(ui);
                            ui.add_space(18.0);
                            self.queue_area(ui);
                        });
                } else {
                    self.settings_card(ui);
                    ui.add_space(18.0);
                    self.queue_area(ui);
                }
            });
        if self.running {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
        let hovering = ctx.input(|input| !input.raw.hovered_files.is_empty());
        if hovering && !self.running {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("drop_overlay"),
            ));
            let rect = ctx.screen_rect().shrink(12.0);
            painter.rect_filled(rect, 14.0, Color32::from_rgba_unmultiplied(25, 44, 76, 225));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Rilascia per aggiungere i PDF",
                FontId::proportional(27.0),
                Color32::WHITE,
            );
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, SETTINGS_KEY, &self.settings);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn configure_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.override_text_color = Some(NAVY);
    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = Color32::WHITE;
    visuals.selection.bg_fill = BLUE;
    visuals.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(240, 244, 250);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(240, 244, 250);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, LINE);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(224, 234, 250);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(224, 234, 250);
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.spacing.item_spacing = Vec2::new(9.0, 6.0);
        style.spacing.button_padding = Vec2::new(12.0, 8.0);
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(14.0));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(13.0));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(12.0));
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(22.0));
    });
}

fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0_f32, LINE))
        .corner_radius(12)
        .inner_margin(14)
}

fn badge(ui: &mut egui::Ui, text: &str, foreground: Color32, background: Color32) {
    egui::Frame::new()
        .fill(background)
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(9, 5))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.0).strong().color(foreground));
        });
}

/// Use the operating system file association without constructing a shell command.
fn open_path(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err("Il file o la cartella non è più disponibile.".into());
    }
    #[cfg(target_os = "windows")]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new("explorer.exe");
        command.creation_flags(0x0800_0000);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut command = Command::new("xdg-open");
    command
        .arg(crate::engine::external_runtime_path(path))
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Impossibile aprire il percorso: {error}"))
}

/// A small code-drawn window icon; no external image dependency is needed.
pub fn window_icon() -> egui::IconData {
    let size = 64usize;
    let mut rgba = vec![0; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let offset = (y * size + x) * 4;
            let edge =
                (x < 7 && y < 7) || (x > 56 && y < 7) || (x < 7 && y > 56) || (x > 56 && y > 56);
            if edge {
                continue;
            }
            let page = (18..46).contains(&x) && (12..52).contains(&y);
            let line = page
                && (24..40).contains(&x)
                && ((25..28).contains(&y) || (33..36).contains(&y) || (41..44).contains(&y));
            let color = if line {
                BLUE
            } else if page {
                Color32::WHITE
            } else {
                NAVY
            };
            rgba[offset..offset + 4].copy_from_slice(&[color.r(), color.g(), color.b(), 255]);
        }
    }
    egui::IconData {
        rgba,
        width: size as u32,
        height: size as u32,
    }
}
