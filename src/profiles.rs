//! Transfer profiles, in a file of their own.
//!
//! They used to sit in the settings file. They are a list the user edits as a
//! list — added to, copied, thrown away — and nothing else in the settings is
//! shaped that way, so they live apart from it: `profiles.yaml` holds the
//! whole list and nothing but.

use crate::error::Result;
use zyt_config::ConfigStore;
use zyt_xfer::{TransferProfile, default_profiles};

/// Name of the file holding the transfer profiles.
pub const PROFILES_FILE: &str = "profiles.yaml";

/// The profiles as they stand on disk, with the shipped ones the file has
/// never heard of added to them.
///
/// A missing file is the first start: the shipped profiles are written out, so
/// what the application offers is always a file the user can read and edit.
pub fn load(store: &ConfigStore) -> Vec<TransferProfile> {
    let kept = match store.load::<Vec<TransferProfile>>(PROFILES_FILE) {
        Ok(Some(kept)) => kept,
        Ok(None) => Vec::new(),
        Err(error) => {
            log::error!("cannot read the transfer profiles: {error}");
            Vec::new()
        }
    };

    let mut profiles = kept.clone();
    add_missing(&mut profiles);
    if profiles != kept
        && let Err(error) = save(store, &profiles)
    {
        log::error!("cannot write the transfer profiles: {error}");
    }
    profiles
}

/// Writes the whole list.
pub fn save(store: &ConfigStore, profiles: &[TransferProfile]) -> Result<()> {
    store.save(PROFILES_FILE, &profiles.to_vec())?;
    Ok(())
}

/// The profile of that name, or the first one when the name names none.
///
/// A device remembers the profile it was last used with by name, and a name
/// can outlive the profile it named — it was thrown away, or the device no
/// longer offers it — so a list that holds anything answers with something.
///
/// It is handed a list of references and not the whole list, because what a
/// device may use is a list of its own: the profiles it offers, which is every
/// one of them until it says otherwise.
pub fn find<'a>(
    profiles: &[&'a TransferProfile],
    name: Option<&str>,
) -> Option<&'a TransferProfile> {
    name.and_then(|name| {
        profiles
            .iter()
            .copied()
            .find(|profile| profile.name == name)
    })
    .or_else(|| profiles.first().copied())
}

/// Adds every shipped profile the list has never heard of.
///
/// A profile the user threw away comes back this way, which is the price of
/// ever shipping a new one.
fn add_missing(profiles: &mut Vec<TransferProfile>) {
    for shipped in default_profiles() {
        if !profiles.iter().any(|kept| kept.name == shipped.name) {
            profiles.push(shipped);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Name the shell transfer profile carries.
    const SHELL_TRANSFER: &str = "Shell Transfer";

    fn store(case: &str) -> ConfigStore {
        let root =
            std::env::temp_dir().join(format!("zyterm-profiles-{case}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("the directory is made");
        ConfigStore::with_paths(root.clone(), root.clone(), root)
    }

    #[test]
    fn a_first_start_writes_what_is_shipped() {
        let store = store("first");
        let profiles = load(&store);

        assert_eq!(profiles, default_profiles());
        assert!(store.path(PROFILES_FILE).exists());
    }

    #[test]
    fn a_shipped_profile_the_file_never_heard_of_is_added() {
        let store = store("missing");
        let mut kept = default_profiles();
        kept.retain(|profile| profile.name != SHELL_TRANSFER);
        save(&store, &kept).expect("the file is written");

        let profiles = load(&store);

        assert_eq!(
            profiles
                .iter()
                .filter(|one| one.name == SHELL_TRANSFER)
                .count(),
            1,
            "the shipped profile comes back, once"
        );
    }

    #[test]
    fn what_the_user_wrote_is_kept_and_comes_first() {
        let store = store("kept");
        let mine = TransferProfile {
            name: "mine".to_string(),
            pty: true,
            send: zyt_xfer::TransferCommands::new(
                zyt_xfer::CommandStep::new(0, "cat {>file}"),
                zyt_xfer::CommandStep::default(),
            ),
            receive: zyt_xfer::TransferCommands::default(),
        };
        save(&store, std::slice::from_ref(&mine)).expect("the file is written");

        let profiles = load(&store);

        assert_eq!(profiles.first(), Some(&mine));
        assert!(profiles.len() > 1);
    }

    #[test]
    fn a_name_that_names_nothing_answers_with_the_first() {
        let kept = default_profiles();
        let profiles: Vec<&TransferProfile> = kept.iter().collect();
        assert_eq!(
            find(&profiles, Some("zmodem")).map(|one| one.name.as_str()),
            Some("zmodem")
        );
        assert_eq!(
            find(&profiles, Some("gone")).map(|one| one.name.as_str()),
            kept.first().map(|one| one.name.as_str())
        );
        assert_eq!(find(&profiles, None), kept.first());
        assert_eq!(find(&[], Some("zmodem")), None);
    }
}
