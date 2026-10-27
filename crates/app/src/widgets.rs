//! Controls that follow the Look: in Smooth a pill switch for on/off and
//! small dim caps headers; in Classic egui's checkbox and plain headings.

use egui::{Id, Response, RichText, Sense, Ui, Widget, WidgetText};

fn look_id() -> Id {
    Id::new("das-meter smooth look")
}

/// Tells this context's controls which Look to draw in.
pub fn set_smooth(ctx: &egui::Context, smooth: bool) {
    ctx.data_mut(|d| d.insert_temp(look_id(), smooth));
}

pub fn is_smooth(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp(look_id())).unwrap_or(false)
}

/// An on/off setting: a pill switch in Smooth, a checkbox in Classic.
pub struct Toggle<'a> {
    on: &'a mut bool,
    text: WidgetText,
}

impl<'a> Toggle<'a> {
    pub fn new(on: &'a mut bool, text: impl Into<WidgetText>) -> Toggle<'a> {
        Toggle {
            on,
            text: text.into(),
        }
    }
}

impl Widget for Toggle<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        if !is_smooth(ui.ctx()) {
            return ui.checkbox(self.on, self.text);
        }
        // In a column (a menu, the settings) the label leads and the switch
        // sits at the row's right end, lined up with the choices' right ends.
        if ui.layout().is_vertical() {
            let Toggle { on, text } = self;
            let width = row_width(ui);
            return ui
                .allocate_ui_with_layout(
                    egui::vec2(width, ui.spacing().interact_size.y),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        let label = ui.add(egui::Label::new(text).sense(Sense::click()));
                        let mut response = ui
                            .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                switch(ui, on)
                            })
                            .inner;
                        if label.clicked() {
                            *on = !*on;
                            response.mark_changed();
                        }
                        response | label
                    },
                )
                .inner;
        }
        let Toggle { on, text } = self;
        ui.horizontal(|ui| {
            let mut response = switch(ui, on);
            let label = ui.add(egui::Label::new(text).sense(Sense::click()));
            if label.clicked() {
                *on = !*on;
                response.mark_changed();
            }
            response | label
        })
        .inner
    }
}

/// How wide a settings row is: the column's width, at most [`ROW_MAX_WIDTH`]
/// so in a wide window the controls stay near their labels.
pub fn row_width(ui: &Ui) -> f32 {
    ui.available_width().min(ROW_MAX_WIDTH)
}

/// The widest a settings row gets, in logical px.
pub const ROW_MAX_WIDTH: f32 = 340.0;

/// The pill switch alone.
fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let height = ui.spacing().interact_size.y * 0.8;
    let size = egui::vec2(height * 1.8, height);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let how_on = ui.ctx().animate_bool_responsive(response.id, *on);
        let visuals = ui.style().interact(&response);
        let text = visuals.fg_stroke.color;
        // Off: a faint track. On: light gray, the knob in full text colour.
        let fill = text
            .gamma_multiply(0.18)
            .lerp_to_gamma(text.gamma_multiply(0.55), how_on);
        let radius = rect.height() / 2.0;
        ui.painter().rect_filled(rect, radius, fill);
        let knob = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
        ui.painter().circle_filled(
            egui::pos2(knob, rect.center().y),
            radius * 0.78,
            text.gamma_multiply(0.7 + 0.3 * how_on),
        );
    }
    response
}

/// A heading over a group of settings: small dim caps, in Smooth and
/// Classic alike, so it stands apart from the settings under it.
pub fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text.to_uppercase()).small().weak());
}

/// A section of a menu: a thin line, then its heading, keeping what
/// belongs together under one name.
pub fn section(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.separator();
    heading(ui, text);
    ui.add_space(2.0);
}
