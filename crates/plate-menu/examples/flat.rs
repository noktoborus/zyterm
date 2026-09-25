//! The smallest menu there is: a few entries, and what was chosen printed.
//!
//! ```sh
//! cargo run -p plate-menu --example flat
//! ```

use plate_menu::{MenuItem, PlateMenu};

fn main() -> eframe::Result {
    eframe::run_native(
        "plate-menu: flat",
        eframe::NativeOptions::default(),
        Box::new(|_| Ok(Box::new(Example::default()))),
    )
}

#[derive(Default)]
struct Example {
    menu: PlateMenu,
    chosen: Option<String>,
}

impl eframe::App for Example {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();

        if ui.button("Open the menu").clicked()
            && let Err(error) = self.menu.open(entries())
        {
            self.chosen = Some(error.to_string());
        }

        match &self.chosen {
            Some(chosen) => ui.label(format!("chosen: {chosen}")),
            None => ui.label("nothing chosen yet"),
        };

        if let Some(chosen) = self.menu.show(&context) {
            let held = if chosen.held.shift { " with shift" } else { "" };
            self.chosen = Some(format!("{}{held}", chosen.id));
        }
    }
}

fn entries() -> Vec<MenuItem> {
    vec![
        MenuItem::new("copy", "Copy").detail("ctrl+shift+c"),
        MenuItem::new("paste", "Paste").detail("ctrl+shift+v"),
        MenuItem::separator(),
        MenuItem::new("clear", "Clear the screen").hint("Drops the scrollback as well"),
        MenuItem::new("quit", "Quit").detail("ctrl+shift+q"),
    ]
}
