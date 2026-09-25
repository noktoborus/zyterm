//! Round trip of configuration files in a temporary directory.

use serde::{Deserialize, Serialize};
use zyt_config::{ConfigError, ConfigStore};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Settings {
    baud_rate: u32,
    name: String,
}

fn store(case: &str) -> (ConfigStore, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("zyt-config-{case}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let store = ConfigStore::with_paths(root.join("config"), root.join("data"), root.join("locks"));
    (store, root)
}

#[test]
fn save_and_load_round_trip() {
    let (store, root) = store("round-trip");
    let settings = Settings {
        baud_rate: 115_200,
        name: "device".to_string(),
    };

    assert_eq!(store.load::<Settings>("settings.yaml").unwrap(), None);
    store.save("settings.yaml", &settings).unwrap();
    assert!(store.exists("settings.yaml"));
    assert_eq!(
        store.load::<Settings>("settings.yaml").unwrap(),
        Some(settings)
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn load_or_create_writes_the_fallback() {
    let (store, root) = store("fallback");
    let created: Settings = store
        .load_or_create("settings.yaml", || Settings {
            baud_rate: 9600,
            name: "default".to_string(),
        })
        .unwrap();
    assert_eq!(created.baud_rate, 9600);
    assert!(store.path("settings.yaml").is_file());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn broken_file_reports_a_decode_error() {
    let (store, root) = store("broken");
    std::fs::create_dir_all(store.config_dir()).unwrap();
    std::fs::write(store.path("settings.yaml"), "baud_rate: [1, 2]\n").unwrap();

    let error = store.load::<Settings>("settings.yaml").unwrap_err();
    assert!(matches!(error, ConfigError::Decode { .. }));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_file_name_may_name_a_directory_of_its_own() {
    let (store, root) = store("nested");
    let value = Settings {
        baud_rate: 115_200,
        name: "nested".to_string(),
    };

    store.save("consoles/one.yaml", &value).unwrap();
    assert!(store.path("consoles/one.yaml").is_file());
    assert_eq!(
        store.load::<Settings>("consoles/one.yaml").unwrap(),
        Some(value)
    );

    let _ = std::fs::remove_dir_all(root);
}

/// A lock says a copy is running now, which stops being true when the machine
/// restarts. It therefore belongs in the temporary directory of the platform
/// and not in a configuration directory, which on a roaming profile or a
/// synchronised home is carried to every other machine.
#[test]
fn the_lock_directory_stands_apart_from_the_settings() {
    let id = zyt_config::AppId {
        qualifier: "org".to_string(),
        organization: "zyt-config-test".to_string(),
        application: "zyt-config-test".to_string(),
    };
    let Ok(store) = ConfigStore::new(&id) else {
        return;
    };

    assert!(
        store.lock_dir().starts_with(tempfile::env::temp_dir()),
        "the locks are not under the temporary directory: {}",
        store.lock_dir().display()
    );
    assert!(
        !store.lock_dir().starts_with(store.config_dir()),
        "the locks stand inside the configuration directory"
    );
    assert!(
        !store.lock_dir().starts_with(store.data_dir()),
        "the locks stand inside the data directory"
    );
}

#[test]
fn saying_again_what_a_file_already_says_leaves_it_alone() {
    let (store, root) = store("unchanged");
    let settings = Settings {
        baud_rate: 115_200,
        name: "device".to_string(),
    };
    store.save("settings.yaml", &settings).unwrap();

    let written = std::fs::metadata(store.path("settings.yaml"))
        .unwrap()
        .modified()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    store.save("settings.yaml", &settings).unwrap();

    assert_eq!(
        std::fs::metadata(store.path("settings.yaml"))
            .unwrap()
            .modified()
            .unwrap(),
        written,
        "a write that would change nothing was made anyway"
    );

    let changed = Settings {
        baud_rate: 9600,
        ..settings
    };
    store.save("settings.yaml", &changed).unwrap();
    assert_eq!(
        store.load::<Settings>("settings.yaml").unwrap(),
        Some(changed),
        "and one that would change something was not"
    );

    let _ = std::fs::remove_dir_all(root);
}
