// ABOUTME: egui/eframe GUI: a reactive worksheet over the reduction engine.
// ABOUTME: Cells replay into a fresh Environment so delayed binding is live.

use eframe::egui;
use egui::{Color32, FontId, Frame, Margin, RichText, ScrollArea, Stroke};
use nat_calc::ast::Expr;
use nat_calc::graph::{graph_vars, plot_with_params};
use nat_calc::logic::{eval_logic, logic_vars};
use nat_calc::{Environment, EvalResult, eval, parse};
use std::collections::HashMap;

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
const LOGIC: Color32 = Color32::from_rgb(92, 214, 132); // logic   -> green
const ERROR: Color32 = Color32::from_rgb(240, 92, 104); // error    -> red

// --- Model ---------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Eager,
    Lazy,
    Matrix,
    Lambda,
    Logic,
    Error,
}

impl Mode {
    fn color(self) -> Color32 {
        match self {
            Mode::Eager => EAGER,
            Mode::Lazy => LAZY,
            Mode::Matrix => MATRIX,
            Mode::Lambda => LAMBDA,
            Mode::Logic => LOGIC,
            Mode::Error => ERROR,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Mode::Eager => "EAGER",
            Mode::Lazy => "LAZY",
            Mode::Matrix => "MATRIX",
            Mode::Lambda => "LAMBDA",
            Mode::Logic => "LOGIC",
            Mode::Error => "ERROR",
        }
    }
}

struct Cell {
    src: String,
    mode: Mode,
    output: String,
    result: Option<EvalResult>,
    logic_expr: Option<Expr>,
    logic_inputs: Vec<(String, bool)>,
    plot_spec: Option<PlotSpec>,
    plot_params: Vec<(String, f64)>,
}

#[derive(Clone)]
struct PlotSpec {
    expr: Expr,
    var: String,
    start: Expr,
    end: Expr,
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
            let parsed = parse(&cell.src).ok();
            match eval(&cell.src, &mut env) {
                Ok(r) => {
                    cell.mode = mode_of(&r);
                    cell.output = r.to_string();
                    cell.result = Some(r);
                    if let Some(expr) = parsed.as_ref().and_then(logic_target) {
                        cell.logic_expr = Some(expr.clone());
                        sync_logic_inputs(cell);
                    } else {
                        cell.logic_expr = None;
                        cell.logic_inputs.clear();
                    }
                    if let Some(spec) = parsed.as_ref().and_then(plot_target) {
                        cell.plot_spec = Some(spec);
                        sync_plot_params(cell);
                    } else {
                        cell.plot_spec = None;
                        cell.plot_params.clear();
                    }
                }
                Err(e) => {
                    cell.mode = Mode::Error;
                    cell.output = e.to_string();
                    cell.result = None;
                    cell.logic_expr = None;
                    cell.logic_inputs.clear();
                    cell.plot_spec = None;
                    cell.plot_params.clear();
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
            result: None,
            logic_expr: None,
            logic_inputs: Vec::new(),
            plot_spec: None,
            plot_params: Vec::new(),
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

fn plot_target(expr: &Expr) -> Option<PlotSpec> {
    match expr {
        Expr::Plot(e, var, start, end) => Some(PlotSpec {
            expr: (**e).clone(),
            var: var.clone(),
            start: (**start).clone(),
            end: (**end).clone(),
        }),
        _ => None,
    }
}

fn sync_plot_params(cell: &mut Cell) {
    let Some(spec) = &cell.plot_spec else {
        return;
    };
    let old: HashMap<String, f64> = cell.plot_params.iter().cloned().collect();
    cell.plot_params = graph_vars(&spec.expr)
        .into_iter()
        .filter(|name| name != &spec.var)
        .map(|name| {
            let value = old.get(&name).copied().unwrap_or(1.0);
            (name, value)
        })
        .collect();
}

fn logic_target(expr: &Expr) -> Option<&Expr> {
    match expr {
        Expr::Truth(e) | Expr::Circuit(e) | Expr::LogicSimplify(e) | Expr::KMap(_, e) => Some(e),
        Expr::Bool(_) | Expr::Not(_) | Expr::Logic(_, _, _) => Some(expr),
        _ => None,
    }
}

fn sync_logic_inputs(cell: &mut Cell) {
    let Some(expr) = &cell.logic_expr else {
        return;
    };
    let old: HashMap<String, bool> = cell.logic_inputs.iter().cloned().collect();
    cell.logic_inputs = logic_vars(expr)
        .into_iter()
        .map(|name| {
            let value = old.get(&name).copied().unwrap_or(false);
            (name, value)
        })
        .collect();
}

fn mode_of(r: &EvalResult) -> Mode {
    match r {
        EvalResult::Numeric(_) => Mode::Eager,
        EvalResult::Bool(_) => Mode::Logic,
        EvalResult::TruthTable(_) => Mode::Logic,
        EvalResult::CircuitDiagram(_) => Mode::Logic,
        EvalResult::EquivResult(_) => Mode::Logic,
        EvalResult::KMap(_) => Mode::Logic,
        EvalResult::AdderResult(_) => Mode::Logic,
        EvalResult::ValueTable(_) => Mode::Eager,
        EvalResult::Plot2D(_) => Mode::Eager,
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
                        for (i, cell) in self.cells.iter_mut().enumerate() {
                            cell_card(ui, i + 1, cell);
                            ui.add_space(10.0);
                        }
                    });
            });
    }
}

fn cell_card(ui: &mut egui::Ui, index: usize, cell: &mut Cell) {
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
                if let Some(EvalResult::TruthTable(table)) = &cell.result {
                    truth_table_view(ui, table);
                } else if let Some(EvalResult::CircuitDiagram(diagram)) = &cell.result {
                    ui.label(
                        RichText::new(diagram.to_string())
                            .color(LOGIC)
                            .monospace()
                            .size(15.0)
                            .strong(),
                    );
                } else if let Some(EvalResult::Plot2D(plot)) = cell.result.clone() {
                    if let Some(live_plot) = plot_controls(ui, cell) {
                        plot_view(ui, &live_plot);
                    } else {
                        plot_view(ui, &plot);
                    }
                } else {
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
                }
            });
            logic_controls(ui, cell);
        });
}

fn plot_controls(ui: &mut egui::Ui, cell: &mut Cell) -> Option<nat_calc::graph::Plot2D> {
    let spec = cell.plot_spec.clone()?;
    if cell.plot_params.is_empty() {
        return None;
    }
    ui.vertical(|ui| {
        ui.horizontal_wrapped(|ui| {
            for (name, value) in &mut cell.plot_params {
                ui.label(RichText::new(name.as_str()).color(DIM).monospace());
                ui.add(
                    egui::Slider::new(value, -10.0..=10.0)
                        .step_by(0.1)
                        .show_value(true),
                );
            }
        });
    });
    plot_with_params(
        &spec.expr,
        &spec.var,
        &spec.start,
        &spec.end,
        &cell.plot_params,
    )
    .ok()
}

fn plot_view(ui: &mut egui::Ui, plot: &nat_calc::graph::Plot2D) {
    let width = ui.available_width().clamp(320.0, 720.0);
    let size = egui::vec2(width, 260.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 8.0, PANEL);
    painter.rect_stroke(
        rect,
        8.0,
        Stroke::new(1.0, STROKE),
        egui::StrokeKind::Inside,
    );

    let bounds = plot_bounds(plot);
    let to_screen = |x: f64, y: f64| {
        let tx = ((x - bounds.0) / (bounds.1 - bounds.0)) as f32;
        let ty = ((y - bounds.2) / (bounds.3 - bounds.2)) as f32;
        egui::pos2(
            rect.left() + tx * rect.width(),
            rect.bottom() - ty * rect.height(),
        )
    };

    if bounds.0 <= 0.0 && bounds.1 >= 0.0 {
        let a = to_screen(0.0, bounds.2);
        let b = to_screen(0.0, bounds.3);
        painter.line_segment([a, b], Stroke::new(1.0, DIM.linear_multiply(0.55)));
    }
    if bounds.2 <= 0.0 && bounds.3 >= 0.0 {
        let a = to_screen(bounds.0, 0.0);
        let b = to_screen(bounds.1, 0.0);
        painter.line_segment([a, b], Stroke::new(1.0, DIM.linear_multiply(0.55)));
    }

    let colors = [EAGER, LAZY, MATRIX, LOGIC];
    for (i, curve) in plot.curves.iter().enumerate() {
        let color = colors[i % colors.len()];
        for pair in curve.points.windows(2) {
            let a = to_screen(pair[0].0, pair[0].1);
            let b = to_screen(pair[1].0, pair[1].1);
            painter.line_segment([a, b], Stroke::new(2.0, color));
        }
    }

    if let Some(pointer) = resp.hover_pos()
        && rect.contains(pointer)
        && let Some((curve_index, point)) = nearest_plot_point(plot, bounds, rect, pointer)
    {
        let color = colors[curve_index % colors.len()];
        let p = to_screen(point.0, point.1);
        painter.circle_filled(p, 4.0, color);
        painter.line_segment(
            [egui::pos2(p.x, rect.top()), egui::pos2(p.x, rect.bottom())],
            Stroke::new(1.0, color.linear_multiply(0.45)),
        );
        painter.text(
            p + egui::vec2(8.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            format!("({}, {})", graph_num(point.0), graph_num(point.1)),
            FontId::monospace(12.0),
            TEXT,
        );
    }

    let label = format!(
        "{}: {}..{}",
        plot.var,
        graph_num(bounds.0),
        graph_num(bounds.1)
    );
    painter.text(
        rect.left_top() + egui::vec2(10.0, 8.0),
        egui::Align2::LEFT_TOP,
        label,
        FontId::monospace(12.0),
        DIM,
    );
}

fn nearest_plot_point(
    plot: &nat_calc::graph::Plot2D,
    bounds: (f64, f64, f64, f64),
    rect: egui::Rect,
    pointer: egui::Pos2,
) -> Option<(usize, (f64, f64))> {
    let to_screen = |x: f64, y: f64| {
        let tx = ((x - bounds.0) / (bounds.1 - bounds.0)) as f32;
        let ty = ((y - bounds.2) / (bounds.3 - bounds.2)) as f32;
        egui::pos2(
            rect.left() + tx * rect.width(),
            rect.bottom() - ty * rect.height(),
        )
    };
    let mut best: Option<(usize, (f64, f64), f32)> = None;
    for (curve_index, curve) in plot.curves.iter().enumerate() {
        for point in &curve.points {
            let screen = to_screen(point.0, point.1);
            let dist = screen.distance_sq(pointer);
            if best.is_none_or(|(_, _, best_dist)| dist < best_dist) {
                best = Some((curve_index, *point, dist));
            }
        }
    }
    best.map(|(i, point, _)| (i, point))
}

fn plot_bounds(plot: &nat_calc::graph::Plot2D) -> (f64, f64, f64, f64) {
    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;
    for (_, y) in plot.curves.iter().flat_map(|curve| curve.points.iter()) {
        y_min = y_min.min(*y);
        y_max = y_max.max(*y);
    }
    if !y_min.is_finite() || !y_max.is_finite() {
        y_min = -1.0;
        y_max = 1.0;
    }
    if (y_max - y_min).abs() < 1e-9 {
        y_min -= 1.0;
        y_max += 1.0;
    }
    let pad = (y_max - y_min) * 0.08;
    (plot.x_min, plot.x_max, y_min - pad, y_max + pad)
}

fn graph_num(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    format!("{rounded:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn logic_controls(ui: &mut egui::Ui, cell: &mut Cell) {
    let Some(expr) = &cell.logic_expr else {
        return;
    };
    if cell.logic_inputs.is_empty() {
        return;
    }
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        for (name, value) in &mut cell.logic_inputs {
            ui.checkbox(value, RichText::new(name.as_str()).color(TEXT).monospace());
        }
        let env: HashMap<&str, bool> = cell
            .logic_inputs
            .iter()
            .map(|(name, value)| (name.as_str(), *value))
            .collect();
        if let Ok(value) = eval_logic(expr, &env) {
            ui.add_space(8.0);
            ui.label(RichText::new("out").color(DIM).monospace());
            bool_cell(ui, value);
        }
    });
}

fn truth_table_view(ui: &mut egui::Ui, table: &nat_calc::logic::TruthTable) {
    egui::Grid::new(ui.next_auto_id())
        .spacing(egui::vec2(16.0, 4.0))
        .striped(true)
        .show(ui, |ui| {
            for name in &table.vars {
                ui.label(RichText::new(name).color(DIM).monospace().strong());
            }
            ui.label(RichText::new("out").color(LOGIC).monospace().strong());
            ui.end_row();
            for (values, out) in &table.rows {
                for value in values {
                    bool_cell(ui, *value);
                }
                bool_cell(ui, *out);
                ui.end_row();
            }
        });
}

fn bool_cell(ui: &mut egui::Ui, value: bool) {
    let color = if value { LOGIC } else { DIM };
    ui.label(
        RichText::new(if value { "1" } else { "0" })
            .color(color)
            .monospace()
            .strong(),
    );
}
