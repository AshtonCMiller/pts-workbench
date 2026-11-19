//! The look and motion every part of PTS Workbench shares: the app's colours and styles (light and dark), and its
//! smooth, slightly bouncy animations (tabs whose highlight slides, pages that slide and fade, side panels, cards over
//! everything). The app and each part's own screens (xtime-ui, hours-ui…) use these, so they all look and move alike.
//!
//!   style.rs  colours, fonts and the shared styles (cards, tabs, buttons, pills…)
//!   anim.rs   the animated widgets: tabs, page, side, modal
//!   scroll.rs a scrollable whose mouse wheel glides instead of jumping (use it in place of iced's)
//!   menu.rs   a menu that drops down under something while the mouse is over it

pub mod anim;
pub mod menu;
pub mod scroll;
pub mod style;

use iced::widget::{container, text};
use iced::{Color, Element};

/// A small coloured label: an RO's mode, a waiting flag, "Spiffed"
pub fn pill<'a, Message: 'a>(label: impl text::IntoFragment<'a>, rgb: [u8; 3]) -> Element<'a, Message> {
    container(text(label).size(11))
        .padding([1, 8])
        .style(style::pill(Color::from_rgb8(rgb[0], rgb[1], rgb[2])))
        .into()
}
