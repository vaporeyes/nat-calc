// ABOUTME: egui/eframe GUI: a reactive worksheet over the reduction engine.
// ABOUTME: Cells replay into a fresh Environment so delayed binding is live.

use eframe::egui;
use egui::{Color32, FontId, Frame, Margin, RichText, ScrollArea, Stroke};
use nat_calc::{Environment, EvalResult, eval};

// --- Palette -------------------------------------------------------------

const BG: Color32 = Color32::from_rgb(13, 14, 19);
const PANEL: Color32 = Color32::from_rgb(20, 22, 30);
const CARD: Color32 = Color32::from_rgb(26, 28, 38);
const STROKE: Color32 = Color32::from_rgb(40, 43, 56);
const TEXT: Color32 = Color32::from_rgb(226, 229, 238);
const DIM: Color32 = Color32::from_rgb(132, 138, 156);

const EAGER: Color32 = Color32::from_rgb(46, 196, 182); // numeric  -> teal
const LAZY: Color32 = Color32::from_rgb(157, 124, 255); // symbolic -> violet
const MATRIX: Color32 = Color32::from_rgb(240, 182, 92); // matrix  -> amber
const LAMBDA: Color32 = Color32::from_rgb(120, 200, 255); // lambda -> sky
const ERROR: Color32 = Color32::from_rgb(240, 92, 104); // error    -> red

// --- Model ---------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Eager,
    Lazy,
    Matrix,
    Lambda,
    Error,
}

impl Mode {
    fn color(self) -> Color32 {
        match self {
            Mode::Eager => EAGER,
            Mode::Lazy => LAZY,
            Mode::Matrix => MATRIX,
            Mode::Lambda => LAMBDA,
            Mode::Error => ERROR,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Mode::Eager => "EAGER",
            Mode::Lazy => "LAZY",
            Mode::Matrix => "MATRIX",
            Mode::Lambda => "LAMBDA",
            Mode::Error => "ERROR",
        }
    }
}

struct Cell {
    src: String,
    mode: Mode,
    output: String,
}

pub struct CalcApp {
    cells: Vec<Cell>,
    draft: String,
    history: Vec<String>,
    history_cursor: Option<usize>,
    history_stash: String,
    bindings: Vec<(String, String, Mode)>,
    focus_input: bool,
}

impl CalcApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_theme(&cc.egui_ctx);
        Self {
            cells: Vec::new(),
            draft: String::new(),
            history: Vec::new(),
            history_cursor: None,
            history_stash: String::new(),
            bindings: Vec::new(),
            focus_input: true,
        }
    }

    /// Re-evaluate every cell in order against a fresh environment. This is
    /// the spec's reactive contract: rebinding a variable recomputes every
    /// dependent cell. O(cells) and trivially correct.
    fn recompute(&mut self) {
        let mut env = Environment::new();
        for cell in &mut self.cells {
            match eval(&cell.src, &mut env) {
                Ok(r) => {
                    cell.mode = mode_of(&r);
                    cell.output = r.to_string();
                }
                Err(e) => {
                    cell.mode = Mode::Error;
                    cell.output = e.to_string();
                }
            }
        }
        self.bindings = env
            .bindings()
            .into_iter()
            .map(|(name, expr)| {
                let mut probe = env.clone();
                let mode = match eval(&name, &mut probe) {
                    Ok(r) => mode_of(&r),
                    Err(_) => Mode::Error,
                };
                (name, expr.to_string(), mode)
            })
            .collect();
    }

    fn submit(&mut self) {
        let src = self.draft.trim().to_string();
        if src.is_empty() {
            return;
        }
        self.history.push(src.clone());
        self.history_cursor = None;
        self.history_stash.clear();
        self.cells.push(Cell {
            src,
            mode: Mode::Eager,
            output: String::new(),
        });
        self.draft.clear();
        self.focus_input = true;
        self.recompute();
    }

    fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let index = match self.history_cursor {
            Some(0) => 0,
            Some(i) => i - 1,
            None => {
                self.history_stash.clone_from(&self.draft);
                self.history.len() - 1
            }
        };
        self.history_cursor = Some(index);
        self.draft.clone_from(&self.history[index]);
        self.focus_input = true;
    }

    fn history_down(&mut self) {
        let Some(i) = self.history_cursor else {
            return;
        };
        if i + 1 < self.history.len() {
            let next = i + 1;
            self.history_cursor = Some(next);
            self.draft.clone_from(&self.history[next]);
        } else {
            self.history_cursor = None;
            self.draft.clone_from(&self.history_stash);
            self.history_stash.clear();
        }
        self.focus_input = true;
    }
}

fn mode_of(r: &EvalResult) -> Mode {
    match r {
        EvalResult::Numeric(_) => Mode::Eager,
        EvalResult::Matrix(_) => Mode::Matrix,
        EvalResult::Symbolic(_) => Mode::Lazy,
        EvalResult::Lambda(_) => Mode::Lambda,
    }
}

// --- Theme ---------------------------------------------------------------

fn install_theme(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();
    let v = &mut style.visuals;
    v.dark_mode = true;
    v.override_text_color = Some(TEXT);
    v.panel_fill = PANEL;
    v.window_fill = BG;
    v.extreme_bg_color = BG;
    v.faint_bg_color = CARD;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, STROKE);
    v.widgets.inactive.bg_fill = CARD;
    v.widgets.inactive.weak_bg_fill = CARD;
    v.widgets.hovered.bg_fill = Color32::from_rgb(36, 39, 52);
    v.widgets.active.bg_fill = Color32::from_rgb(44, 47, 64);
    v.selection.bg_fill = LAZY.linear_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, LAZY);
    let r = egui::CornerRadius::same(10);
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.inactive.corner_radius = r;
    v.widgets.hovered.corner_radius = r;
    v.widgets.active.corner_radius = r;

    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    ctx.set_global_style(style);
}

fn badge(ui: &mut egui::Ui, mode: Mode) {
    let c = mode.color();
    Frame::new()
        .fill(c.linear_multiply(0.18))
        .stroke(Stroke::new(1.0, c))
        .corner_radius(7)
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(
                RichText::new(mode.label())
                    .color(c)
                    .monospace()
                    .size(11.0)
                    .strong(),
            );
        });
}

fn chip(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add(egui::Button::new(
        RichText::new(label).monospace().size(13.0).color(DIM),
    ))
    .clicked()
}

// --- App -----------------------------------------------------------------

impl eframe::App for CalcApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("header")
            .frame(Frame::new().fill(BG).inner_margin(Margin::symmetric(20, 14)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("∑ nat-calc")
                            .color(TEXT)
                            .size(24.0)
                            .strong(),
                    );
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("dual-mode reduction engine")
                            .color(DIM)
                            .size(13.0),
                    );
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if ui
                                .button(RichText::new("Clear").size(13.0))
                                .clicked()
                            {
                                self.cells.clear();
                                self.bindings.clear();
                            }
                            for cmd in ["derive(x, )", "expand()", "simplify()"] {
                                if chip(ui, cmd) {
                                    self.draft.push_str(cmd);
                                    self.focus_input = true;
                                }
                            }
                            ui.label(
                                RichText::new("insert:").color(DIM).size(12.0),
                            );
                        },
                    );
                });
            });

        egui::Panel::right("bindings")
            .resizable(false)
            .exact_size(250.0)
            .frame(
                Frame::new()
                    .fill(PANEL)
                    .inner_margin(Margin::symmetric(16, 16)),
            )
            .show_inside(ui, |ui| {
                ui.label(
                    RichText::new("BINDINGS")
                        .color(DIM)
                        .size(12.0)
                        .strong()
                        .monospace(),
                );
                ui.add_space(10.0);
                if self.bindings.is_empty() {
                    ui.label(
                        RichText::new("no variables yet")
                            .color(DIM)
                            .italics()
                            .size(13.0),
                    );
                }
                let bindings = self.bindings.clone();
                for (name, value, mode) in &bindings {
                    Frame::new()
                        .fill(CARD)
                        .stroke(Stroke::new(1.0, STROKE))
                        .corner_radius(9)
                        .inner_margin(Margin::same(10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(name)
                                        .color(mode.color())
                                        .monospace()
                                        .strong()
                                        .size(14.0),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(
                                        egui::Align::Center,
                                    ),
                                    |ui| {
                                        if ui
                                            .small_button(
                                                RichText::new("use")
                                                    .size(11.0)
                                                    .color(DIM),
                                            )
                                            .clicked()
                                        {
                                            self.draft.push_str(name);
                                            self.focus_input = true;
                                        }
                                    },
                                );
                            });
                            ui.label(
                                RichText::new(value)
                                    .color(DIM)
                                    .monospace()
                                    .size(12.0),
                            );
                        });
                    ui.add_space(8.0);
                }
            });

        egui::Panel::bottom("prompt")
            .frame(
                Frame::new()
                    .fill(BG)
                    .inner_margin(Margin::symmetric(20, 16)),
            )
            .show_inside(ui, |ui| {
                Frame::new()
                    .fill(CARD)
                    .stroke(Stroke::new(1.0, STROKE))
                    .corner_radius(12)
                    .inner_margin(Margin::symmetric(14, 12))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("❯")
                                    .color(LAZY)
                                    .monospace()
                                    .size(18.0)
                                    .strong(),
                            );
                            let resp = ui.add_sized(
                                [ui.available_width(), 26.0],
                                egui::TextEdit::singleline(&mut self.draft)
                                    .frame(Frame::NONE)
                                    .font(FontId::monospace(16.0))
                                    .hint_text(
                                        "x + x   ·   a = 2   ·   \
                                         expand((x+1)^2)   ·   derive(x, sin(x))",
                                    ),
                            );
                            if self.focus_input {
                                resp.request_focus();
                                self.focus_input = false;
                            }
                            if resp.has_focus()
                                && ui.input_mut(|i| {
                                    i.consume_key(
                                        egui::Modifiers::NONE,
                                        egui::Key::ArrowUp,
                                    )
                                })
                            {
                                self.history_up();
                            }
                            if resp.has_focus()
                                && ui.input_mut(|i| {
                                    i.consume_key(
                                        egui::Modifiers::NONE,
                                        egui::Key::ArrowDown,
                                    )
                                })
                            {
                                self.history_down();
                            }
                            if resp.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                self.submit();
                            }
                        });
                    });
            });

        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(BG)
                    .inner_margin(Margin::symmetric(20, 16)),
            )
            .show_inside(ui, |ui| {
                if self.cells.is_empty() {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("an empty worksheet")
                                .color(DIM)
                                .size(18.0),
                        );
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(
                                "Type below. Unbound names stay symbolic; \
                                 bind them and watch cells recompute.",
                            )
                            .color(DIM)
                            .size(13.0),
                        );
                    });
                }
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for (i, cell) in self.cells.iter().enumerate() {
                            cell_card(ui, i + 1, cell);
                            ui.add_space(10.0);
                        }
                    });
            });
    }
}

fn cell_card(ui: &mut egui::Ui, index: usize, cell: &Cell) {
    Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, STROKE))
        .corner_radius(12)
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("[{index}]"))
                        .color(DIM)
                        .monospace()
                        .size(13.0),
                );
                ui.label(
                    RichText::new(&cell.src)
                        .color(TEXT)
                        .monospace()
                        .size(15.0),
                );
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                badge(ui, cell.mode);
                ui.add_space(6.0);
                let color = if cell.mode == Mode::Error {
                    ERROR
                } else {
                    cell.mode.color()
                };
                ui.label(
                    RichText::new(&cell.output)
                        .color(color)
                        .monospace()
                        .size(16.0)
                        .strong(),
                );
            });
        });
}
