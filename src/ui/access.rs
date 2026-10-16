//! What a screen reader reads of a widget whose face says nothing to it.
//!
//! egui names a widget for accesskit by the text drawn on it. A button that
//! carries only an icon is named by a code point, and the hint a person reads
//! on hover is not given to accesskit at all; a widget drawn by hand has no name
//! unless it is given one. These are the names such widgets are given.
//!
//! The terminal itself is not here: what it shows is not yet given to accesskit.

/// Names the window for accesskit by its title.
///
/// The tree is built again on every pass, and the root of it carries no name
/// unless one is given on that pass: the title the desktop shows is not handed
/// to accesskit by the toolkit.
pub fn name_window(context: &egui::Context, title: &str) {
    context.accesskit_node_builder(egui::accesskit_root_id(), |node| {
        node.set_label(title);
    });
}

/// Names a widget for accesskit.
pub trait Named {
    /// Gives the widget the name a screen reader reads, in place of what is
    /// drawn on it.
    fn named(self, name: &str) -> Self;

    /// Shows `hint` on hover and gives it as the name of the widget too: for a
    /// widget that carries only an icon, the hint is what the icon means.
    fn tip(self, hint: &str) -> Self;
}

impl Named for egui::Response {
    fn named(self, name: &str) -> Self {
        self.ctx.accesskit_node_builder(self.id, |node| {
            node.set_label(name);
        });
        self
    }

    fn tip(self, hint: &str) -> Self {
        self.named(hint).on_hover_text(hint)
    }
}
