//! Colors that follow the kit's theme as it changes.

use crownui::{
    kit::Theme,
    prelude::{Color, IntoProp, Runtime, Signal, bind, rgb},
};

/// Avatar tints: identity colors, the same in light and dark mode.
const AVATAR_TINTS: [u32; 8] = [
    0x5a8dee, 0x8f6dfb, 0x2fb37a, 0xf08a3c, 0xe5527a, 0x2fa8c8, 0xb05ad6, 0xc9a227,
];

/// A color property bound to one slot of `theme`.
pub fn tint(theme: Signal<Theme>, pick: impl Fn(&Theme) -> Color + 'static) -> impl IntoProp<Color> {
    bind(move |runtime: &mut Runtime| theme.with(runtime, &pick))
}

#[must_use]
pub fn avatar_tint(index: usize) -> Color {
    rgb(AVATAR_TINTS.get(index % AVATAR_TINTS.len()).copied().unwrap_or(AVATAR_TINTS[0]))
}

#[must_use]
pub const fn avatar_tint_count() -> usize {
    AVATAR_TINTS.len()
}

/// The bubble color of the phone's own messages.
#[must_use]
pub fn outgoing_bubble(theme: &Theme) -> Color {
    theme.accent.end
}

/// The bubble color of messages from other people.
#[must_use]
pub fn incoming_bubble(theme: &Theme) -> Color {
    theme.surface.raised
}
