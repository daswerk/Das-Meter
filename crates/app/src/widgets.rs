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
        let Toggle { on, text } = self;
        ui.horizontal(|ui| {
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
                let selection = ui.visuals().selection.stroke.color;
                let off = visuals.fg_stroke.color.gamma_multiply(0.18);
                let fill = off.lerp_to_gamma(selection, how_on);
                let radius = rect.height() / 2.0;
                ui.painter().rect_filled(rect, radius, fill);
                let knob = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
                ui.painter().circle_filled(
                    egui::pos2(knob, rect.center().y),
                    radius * 0.78,
                    visuals.fg_stroke.color.gamma_multiply(0.7 + 0.3 * how_on),
                );
            }
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

/// A heading over a group of settings: small dim caps in Smooth, a strong
/// label in Classic.
pub fn heading(ui: &mut Ui, text: &str) {
    if is_smooth(ui.ctx()) {
        ui.add_space(6.0);
        ui.label(RichText::new(text.to_uppercase()).small().weak());
    } else {
        ui.label(RichText::new(text).strong());
    }
}
