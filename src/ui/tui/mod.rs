//! The interactive, btop-styled TUI frontend: an [`input`] mapping that turns
//! keystrokes into core actions, a [`theme`] of fade-aware colors, the [`render`]
//! pass that draws each frame, and the [`renderer`] that owns the terminal.

mod input;
mod render;
mod renderer;
mod theme;

pub use input::map_key;
pub use renderer::TuiRenderer;