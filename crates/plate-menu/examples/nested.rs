//! A menu with entries inside entries: `Right` steps in, `Left` steps back,
//! typing searches the whole tree at once.
//!
//! ```sh
//! cargo run -p plate-menu --example nested
//! ```

use plate_menu::{MenuItem, PlateMenu};

fn main() -> eframe::Result {
    eframe::run_native(
        "plate-menu: nested",
        eframe::NativeOptions::default(),
        Box::new(|_| Ok(Box::new(Example::new()))),
    )
}

struct Example {
    menu: PlateMenu,
    profile: String,
    log: Vec<String>,
}

impl Example {
    fn new() -> Self {
        Self {
            // Five plates at a time, whatever the window is worth.
            menu: PlateMenu::new().max_plates(5),
            profile: "zmodem".to_string(),
            log: Vec::new(),
        }
    }
}

impl eframe::App for Example {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();

        ui.label(format!("profile: {}", self.profile));
        if ui.button("Open the menu").clicked()
            && let Err(error) = self.menu.open(entries(&self.profile))
        {
            self.log.push(error.to_string());
        }
        for line in self.log.iter().rev().take(5) {
            ui.label(line);
        }

        let Some(chosen) = self.menu.show(&context) else {
            return;
        };
        match chosen.id.strip_prefix("profile:") {
            Some(name) => self.profile = name.to_string(),
            None => self.log.push(chosen.id),
        }
    }
}

fn entries(profile: &str) -> Vec<MenuItem> {
    let profiles = ["zmodem", "xmodem", "Cat file"]
        .into_iter()
        .map(|name| MenuItem::new(format!("profile:{name}"), name).enabled(name != profile))
        .collect();

    vec![
        MenuItem::new("send", "Send").hint("Sends a file with the profile below"),
        MenuItem::new("receive", "Receive").detail("F6"),
        MenuItem::new("profile", format!("Profile: {profile}"))
            .enabled(false)
            .children(profiles),
        MenuItem::separator(),
        MenuItem::new("settings", "Settings").children(vec![
            MenuItem::new("settings.appearance", "Appearance"),
            MenuItem::new("settings.keys", "Key bindings"),
            MenuItem::new("settings.about", "About").enabled(false),
        ]),
    ]
}
