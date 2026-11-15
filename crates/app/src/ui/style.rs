//! The app's colors (the Electron app's, light and dark) and the styles its screens share.

use iced::widget::{button, container, text};
use iced::{Background, Border, Color, Font, Shadow, Theme, font, theme::Palette};

use crate::settings::ThemeChoice;

/// 0x1a5fb4 → the color
const fn rgb(hex: u32) -> Color {
    Color {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

/// Colors iced's palette doesn't have names for
pub struct Colors {
    pub panel: Color,
    pub line: Color,
    pub muted: Color,
    pub hover: Color,
    pub sel: Color,
    pub accent: Color,
}

const LIGHT: Colors = Colors {
    panel: rgb(0xffffff),
    line: rgb(0xdde1e6),
    muted: rgb(0x5d6670),
    hover: rgb(0xeef2f7),
    sel: rgb(0xe3ecf8),
    accent: rgb(0x1a5fb4),
};
const DARK: Colors = Colors {
    panel: rgb(0x1d2126),
    line: rgb(0x2e343b),
    muted: rgb(0x9aa3ad),
    hover: rgb(0x262b31),
    sel: rgb(0x23344a),
    accent: rgb(0x4a8fe0),
};

pub fn theme(choice: ThemeChoice) -> Theme {
    match choice {
        ThemeChoice::Light => Theme::custom(
            "PTS Workbench Light".into(),
            Palette {
                background: rgb(0xf4f5f7),
                text: rgb(0x16191d),
                primary: rgb(0x1a5fb4),
                success: rgb(0x15803d),
                danger: rgb(0xb42318),
            },
        ),
        ThemeChoice::Dark => Theme::custom(
            "PTS Workbench Dark".into(),
            Palette {
                background: rgb(0x15181c),
                text: rgb(0xe7eaee),
                primary: rgb(0x4a8fe0),
                success: rgb(0x22c55e),
                danger: rgb(0xf97066),
            },
        ),
    }
}

pub fn colors(theme: &Theme) -> &'static Colors {
    if theme.extended_palette().is_dark { &DARK } else { &LIGHT }
}

pub const BOLD: Font = Font {
    weight: font::Weight::Bold,
    ..Font::DEFAULT
};
pub const MONO: Font = Font::MONOSPACE;

/// A panel: the top bar, the sidebar, cards
pub fn panel(theme: &Theme) -> container::Style {
    let c = colors(theme);
    container::Style {
        background: Some(c.panel.into()),
        border: Border {
            color: c.line,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

/// A card with rounded corners
pub fn card(theme: &Theme) -> container::Style {
    let c = colors(theme);
    container::Style {
        background: Some(c.panel.into()),
        border: Border {
            color: c.line,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

/// A card floating over everything (Settings, Feedback): rounded, with a soft shadow
pub fn floating_card(theme: &Theme) -> container::Style {
    let c = colors(theme);
    container::Style {
        background: Some(c.panel.into()),
        border: Border {
            color: c.line,
            width: 1.0,
            radius: 12.0.into(),
        },
        shadow: Shadow {
            color: Color { a: 0.28, ..Color::BLACK },
            offset: iced::Vector::new(0.0, 14.0),
            blur_radius: 36.0,
        },
        ..Default::default()
    }
}

/// A card that stands out (a new report): an accent edge
pub fn card_highlight(theme: &Theme) -> container::Style {
    let c = colors(theme);
    container::Style {
        background: Some(c.panel.into()),
        border: Border {
            color: c.accent,
            width: 1.5,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

/// The window's background (behind cards)
pub fn page(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(theme.palette().background.into()),
        ..Default::default()
    }
}

/// Grey text for help and secondary details
pub fn muted(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(colors(theme).muted),
    }
}

/// A small coloured label (an RO's mode, a waiting flag)
pub fn pill(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        text_color: Some(color),
        background: Some(Color { a: 0.12, ..color }.into()),
        border: Border {
            color: Color { a: 0.5, ..color },
            width: 1.0,
            radius: 999.0.into(),
        },
        ..Default::default()
    }
}

/// A tab inside [`super::anim::tabs`]: the sliding highlight behind it shows which is selected, so the tab itself
/// only turns its text white when selected (and shades lightly on hover when it isn't)
pub fn tab(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let c = colors(theme);
        let hover = !active && matches!(status, button::Status::Hovered);
        button::Style {
            background: Some(Background::Color(if hover { c.hover } else { Color::TRANSPARENT })),
            text_color: if active { Color::WHITE } else { theme.palette().text },
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: 6.0.into(),
            },
            shadow: Shadow::default(),
        }
    }
}

/// An on/off button (Chat, My ROs): tinted with an accent edge when on
pub fn toggle(on: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let c = colors(theme);
        let hover = matches!(status, button::Status::Hovered);
        button::Style {
            background: Some(Background::Color(if on {
                c.sel
            } else if hover {
                c.hover
            } else {
                c.panel
            })),
            text_color: theme.palette().text,
            border: Border {
                color: if on { c.accent } else { c.line },
                width: 1.0,
                radius: 6.0.into(),
            },
            shadow: Shadow::default(),
        }
    }
}

/// A plain bordered button ("ghost" in the Electron app)
pub fn ghost(theme: &Theme, status: button::Status) -> button::Style {
    let c = colors(theme);
    button::Style {
        background: Some(Background::Color(if matches!(status, button::Status::Hovered) {
            c.hover
        } else {
            c.panel
        })),
        text_color: if matches!(status, button::Status::Disabled) {
            c.muted
        } else {
            theme.palette().text
        },
        border: Border {
            color: c.line,
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: Shadow::default(),
    }
}

/// A row in a list (the sidebar's ROs): marked when selected
pub fn list_item(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let c = colors(theme);
        let bg = if selected {
            c.sel
        } else if matches!(status, button::Status::Hovered) {
            c.hover
        } else {
            Color::TRANSPARENT
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: theme.palette().text,
            border: Border {
                color: if selected { c.accent } else { Color::TRANSPARENT },
                width: if selected { 1.0 } else { 0.0 },
                radius: 6.0.into(),
            },
            shadow: Shadow::default(),
        }
    }
}
