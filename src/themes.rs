//! Aplica un tema (paleta + parametros de cristal/brutalismo) al global `Pal` de la UI.
use crate::{App, Pal, ThemeEntry};
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};

include!("themes_data.rs");

pub const DEFAULT: &str = "catppuccin";

fn col(v: u32) -> Color {
    Color::from_rgb_u8(((v >> 16) & 0xff) as u8, ((v >> 8) & 0xff) as u8, (v & 0xff) as u8)
}

pub fn find(id: &str) -> &'static ThemeDef {
    THEMES.iter().find(|t| t.id == id).unwrap_or(&THEMES[0])
}

pub fn valid(id: &str) -> bool {
    THEMES.iter().any(|t| t.id == id)
}

pub fn entries() -> ModelRc<ThemeEntry> {
    let v: Vec<ThemeEntry> = THEMES
        .iter()
        .map(|t| ThemeEntry {
            id: SharedString::from(t.id),
            name: SharedString::from(t.name),
            c1: col(t.sw[0]),
            c2: col(t.sw[1]),
            c3: col(t.sw[2]),
        })
        .collect();
    ModelRc::new(VecModel::from(v))
}

pub fn apply(app: &App, id: &str) {
    let t = find(id);
    let p = app.global::<Pal>();
    p.set_bg(col(t.bg));
    p.set_bg_accent(col(t.bg_accent));
    p.set_surface(col(t.surface));
    p.set_panel(col(t.panel));
    p.set_panel_strong(col(t.panel_strong));
    p.set_input(col(t.input));
    p.set_line(col(t.line));
    p.set_line_strong(col(t.line_strong));
    p.set_text(col(t.text));
    p.set_muted(col(t.muted));
    p.set_accent(col(t.accent));
    p.set_accent2(col(t.accent2));
    p.set_success(col(t.success));
    p.set_danger(col(t.danger));
    p.set_warning(col(t.warning));
    p.set_on_accent(col(t.on_accent));
    p.set_log_bg(col(t.log_bg));
    p.set_log_text(col(t.log_text));
    p.set_log_border(col(t.log_border));
    p.set_log_cmd(col(t.log_cmd));
    p.set_log_info(col(t.log_info));
    p.set_log_ok(col(t.log_ok));
    p.set_log_err(col(t.log_err));
    p.set_c_search(col(t.c_search));
    p.set_c_preview(col(t.c_preview));
    p.set_c_layers(col(t.c_layers));
    p.set_c_sliders(col(t.c_sliders));
    p.set_c_folder(col(t.c_folder));
    p.set_c_file(col(t.c_file));
    p.set_c_terminal(col(t.c_terminal));
    p.set_hard_panel(col(t.hard_panel));
    p.set_hard_ctrl(col(t.hard_ctrl));
    p.set_brutal(t.brutal);
    if t.brutal {
        p.set_radius(4.0);
        p.set_radius_sm(2.0);
        p.set_border_w(3.0);
        p.set_a_panel(1.0);
        p.set_a_soft(1.0);
        p.set_a_strong(1.0);
        p.set_a_solid(1.0);
        p.set_a_border(1.0);
        p.set_highlight(Color::from_argb_u8(0, 255, 255, 255));
    } else {
        p.set_radius(12.0);
        p.set_radius_sm(8.0);
        p.set_border_w(1.0);
        p.set_a_panel(0.62);
        p.set_a_soft(0.46);
        p.set_a_strong(0.88);
        p.set_a_solid(0.96);
        p.set_a_border(0.36);
        p.set_highlight(Color::from_argb_u8(18, 255, 255, 255));
    }
    app.global::<crate::S>().set_theme(SharedString::from(t.id));
}
