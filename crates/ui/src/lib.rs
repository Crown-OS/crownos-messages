//! The CrownUI window of CrownOS Messages.

pub mod actions;
pub mod root;
pub mod state;
#[cfg(test)]
mod tests;
pub mod util;
pub mod views;
pub mod worker;

use crownui::{
    AppConfig,
    kit::{INTER_FAMILY, inter_font},
    prelude::BLACK,
};

pub use root::{Launch, root};

/// The desktop entry's basename, which is also the Wayland app id.
pub const APP_ID: &str = "org.crownos.messages";
const INITIAL_SIZE: (u32, u32) = (1040, 720);
const MIN_SIZE: (u32, u32) = (720, 480);

/// The window as the binary opens it.
#[must_use]
pub fn app_config() -> AppConfig {
    AppConfig::new("Messages")
        .app_id(APP_ID)
        .size(INITIAL_SIZE.0, INITIAL_SIZE.1)
        .min_size(MIN_SIZE.0, MIN_SIZE.1)
        .clear_color(BLACK)
        .font(inter_font().to_vec(), Some(INTER_FAMILY))
}
