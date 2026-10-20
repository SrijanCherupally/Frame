//! UI-only palette. Colors here never alter the project's rendered footage.
use eframe::egui::{self, Color32, RichText, Sense, Stroke, Vec2};
use serde::{Deserialize, Serialize};
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Theme {
    #[default]
    Midnight,
    Graphite,
    Amethyst,
    Forest,
    Dawn,
}
pub(super) const THEMES: [Theme; 5] = [
    Theme::Midnight,
    Theme::Graphite,
    Theme::Amethyst,
    Theme::Forest,
    Theme::Dawn,
];
#[derive(Clone, Copy)]
pub(super) struct Palette {
    pub accent: Color32,
    pub cyan: Color32,
    pub violet: Color32,
    pub amber: Color32,
    pub coral: Color32,
    pub success: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub canvas: Color32,
    pub rail: Color32,
    pub panel: Color32,
    pub card: Color32,
    pub control: Color32,
    pub border: Color32,
    pub light: bool,
}
impl Palette {
    pub fn tracks(self) -> [Color32; 5] {
        [self.violet, self.cyan, self.accent, self.amber, self.coral]
    }
    pub fn on_accent(self) -> Color32 {
        if self.light {
            Color32::WHITE
        } else {
            Color32::from_rgb(8, 15, 26)
        }
    }
}
const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}
impl Theme {
    pub fn name(self) -> &'static str {
        match self {
            Self::Midnight => "Midnight",
            Self::Graphite => "Graphite",
            Self::Amethyst => "Amethyst",
            Self::Forest => "Forest",
            Self::Dawn => "Dawn",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Midnight => "Quiet charcoal & soft blue",
            Self::Graphite => "Charcoal & warm gold",
            Self::Amethyst => "Deep purple & lavender",
            Self::Forest => "Evergreen & sea glass",
            Self::Dawn => "Light slate & ink blue",
        }
    }
    pub fn colors(self) -> Palette {
        match self {
            Self::Midnight => Palette {
                accent: rgb(188, 205, 244),
                cyan: rgb(147, 190, 195),
                violet: rgb(183, 170, 209),
                amber: rgb(209, 186, 147),
                coral: rgb(215, 156, 163),
                success: rgb(151, 193, 169),
                text: rgb(235, 235, 239),
                muted: rgb(151, 153, 163),
                canvas: rgb(18, 18, 21),
                rail: rgb(23, 23, 27),
                panel: rgb(28, 28, 33),
                card: rgb(35, 35, 41),
                control: rgb(43, 43, 50),
                border: rgb(54, 54, 63),
                light: false,
            },
            Self::Graphite => Palette {
                accent: rgb(241, 184, 100),
                cyan: rgb(99, 211, 217),
                violet: rgb(189, 165, 239),
                amber: rgb(241, 184, 100),
                coral: rgb(246, 144, 153),
                success: rgb(119, 218, 173),
                text: rgb(243, 245, 249),
                muted: rgb(176, 185, 202),
                canvas: rgb(15, 16, 19),
                rail: rgb(22, 24, 29),
                panel: rgb(33, 36, 43),
                card: rgb(46, 50, 60),
                control: rgb(60, 65, 78),
                border: rgb(80, 88, 104),
                light: false,
            },
            Self::Amethyst => Palette {
                accent: rgb(202, 167, 255),
                cyan: rgb(104, 220, 229),
                violet: rgb(216, 177, 255),
                amber: rgb(255, 203, 134),
                coral: rgb(255, 156, 191),
                success: rgb(148, 225, 183),
                text: rgb(250, 242, 255),
                muted: rgb(202, 184, 222),
                canvas: rgb(19, 12, 31),
                rail: rgb(28, 18, 44),
                panel: rgb(42, 29, 62),
                card: rgb(59, 43, 83),
                control: rgb(77, 57, 106),
                border: rgb(104, 81, 139),
                light: false,
            },
            Self::Forest => Palette {
                accent: rgb(144, 229, 182),
                cyan: rgb(105, 219, 224),
                violet: rgb(196, 179, 245),
                amber: rgb(255, 206, 136),
                coral: rgb(250, 161, 151),
                success: rgb(144, 229, 182),
                text: rgb(239, 250, 244),
                muted: rgb(174, 204, 191),
                canvas: rgb(8, 19, 20),
                rail: rgb(12, 29, 29),
                panel: rgb(21, 43, 42),
                card: rgb(32, 61, 56),
                control: rgb(47, 80, 71),
                border: rgb(72, 107, 94),
                light: false,
            },
            Self::Dawn => Palette {
                accent: rgb(43, 80, 174),
                cyan: rgb(9, 103, 118),
                violet: rgb(110, 60, 162),
                amber: rgb(137, 81, 8),
                coral: rgb(168, 45, 69),
                success: rgb(20, 106, 68),
                text: rgb(27, 40, 62),
                muted: rgb(72, 89, 115),
                canvas: rgb(226, 233, 244),
                rail: rgb(241, 245, 251),
                panel: rgb(249, 251, 255),
                card: Color32::WHITE,
                control: rgb(229, 235, 246),
                border: rgb(171, 187, 211),
                light: true,
            },
        }
    }
    pub fn activate(self) {
        ACTIVE.set(self.colors());
    }
}
thread_local! {static ACTIVE:Cell<Palette>=Cell::new(Theme::Midnight.colors());}
pub(super) fn palette() -> Palette {
    ACTIVE.get()
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Preferences {
    pub theme: Theme,
}

pub(super) fn mix(base: Color32, color: Color32, amount: f32) -> Color32 {
    let c = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * amount).round() as u8;
    Color32::from_rgb(
        c(base.r(), color.r()),
        c(base.g(), color.g()),
        c(base.b(), color.b()),
    )
}
pub(super) fn surface(color: Color32, padding: i8) -> egui::Frame {
    egui::Frame::new()
        .fill(color)
        .inner_margin(padding)
        .stroke(Stroke::new(1.0_f32, palette().border.gamma_multiply(0.5)))
}
pub(super) fn card() -> egui::Frame {
    surface(palette().card, 12).corner_radius(10.)
}
pub(super) fn section_title(ui: &mut egui::Ui, title: &str, color: Color32) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::new(3., 14.), Sense::hover());
        ui.painter().rect_filled(r, 1., color);
        ui.label(RichText::new(title).strong().color(palette().text));
    });
}
