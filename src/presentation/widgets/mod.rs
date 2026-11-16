//! Reusable UI toolkit on the virtual 1280x720 stage. Every widget draws in
//! stage coordinates (the camera does letterboxing and DPI scaling) and gets
//! the mouse position in stage coordinates from `Stage::mouse()`.

pub mod button;
pub mod callouts;
pub mod flow_bar;
pub mod modal;
pub mod panel;
pub mod particles;
pub mod pills;
pub mod shapes;
pub mod stat;

pub use button::ActionButton;
pub use callouts::FloatingCallouts;
pub use flow_bar::{FlowMeterBar, Orientation};
pub use modal::{ModalAction, ModalKind, ModalOverlay};
pub use panel::RoundedPanel;
pub use particles::ParticleSystem;
pub use pills::{ModePillSelector, PillLayout};
pub use stat::StatBox;
