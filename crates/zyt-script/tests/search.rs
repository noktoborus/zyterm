//! What was found in the directories searched, and which copy of it won.
//!
//! One directory per script and `Manifest.yaml` in it. What a script is, is a
//! question about files — and that is the point of the manifest: starting the
//! program finds everything that can be done without running a line of
//! anybody's Lua.

mod common;

use common::scripts;
use std::path::{Path, PathBuf};
use zyt_script::{Direction, Library, ScriptError};

/// A directory of scripts of this test, with the named scripts in it.
fn root(case: &str, scripts: &[(&str, &str, &str)]) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("zyt-script-search-{case}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);

    for (name, manifest, body) in scripts {
        let directory = root.join(name);
        std::fs::create_dir_all(&directory).expect("the directory is made");
        std::fs::write(directory.join("Manifest.yaml"), manifest).expect("the manifest is written");
        if !body.is_empty() {
            std::fs::write(directory.join("init.lua"), body).expect("the script is written");
        }
    }
    root
}

/// A manifest that says that much and offers one direction.
fn says(name: &str, order: i64, direction: &str) -> String {
    format!("name: {name}\norder: {order}\n{direction}:\n  target: none\n")
}

/// A script that does nothing in both directions.
const NOTHING: &str = "return { send = function() end, receive = function() end }\n";

#[test]
fn every_script_of_a_directory_is_found_in_the_order_it_is_offered() {
    let root = root(
        "order",
        &[
            ("later", &says("Later", 20, "send"), NOTHING),
            ("sooner", &says("Sooner", 10, "send"), NOTHING),
            ("named", &says("Named", 20, "send"), NOTHING),
        ],
    );

    let library = Library::load(std::slice::from_ref(&root));
    let found: Vec<&str> = library
        .entries()
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(found, ["sooner", "later", "named"], "order, then name");
    assert!(library.problems().is_empty());

    std::fs::remove_dir_all(&root).expect("taken away");
}

#[test]
fn a_script_carries_the_name_a_person_reads_and_the_name_of_its_directory() {
    let root = root(
        "names",
        &[(
            "shell-transfer",
            &says("Shell Transfer", 10, "send"),
            NOTHING,
        )],
    );

    let library = Library::load(std::slice::from_ref(&root));
    let entry = library.find("shell-transfer").expect("it is there");
    assert_eq!(entry.id, "shell-transfer");
    assert_eq!(entry.name(), "Shell Transfer");
    assert_eq!(entry.directory, root.join("shell-transfer"));
    assert_eq!(
        entry.manifest_path(),
        root.join("shell-transfer/Manifest.yaml")
    );
    assert_eq!(entry.path, root.join("shell-transfer/init.lua"));

    std::fs::remove_dir_all(&root).expect("taken away");
}

#[test]
fn the_directory_searched_first_wins_and_says_what_it_stands_in_for() {
    let first = root("first", &[("same", &says("Mine", 10, "send"), NOTHING)]);
    let second = root(
        "second",
        &[("same", &says("Theirs", 90, "receive"), NOTHING)],
    );

    let library = Library::load(&[first.clone(), second.clone()]);
    assert_eq!(library.entries().len(), 1);

    let entry = library.find("same").expect("it is there");
    assert_eq!(entry.root, first);
    assert_eq!(entry.name(), "Mine");
    assert!(entry.manifest.offers(Direction::Send));
    assert_eq!(entry.shadowed, vec![second.join("same")]);

    std::fs::remove_dir_all(&first).expect("taken away");
    std::fs::remove_dir_all(&second).expect("taken away");
}

#[test]
fn a_directory_that_cannot_be_started_is_named_as_a_problem_and_the_rest_load() {
    let root = root(
        "broken",
        &[
            ("fine", &says("Fine", 10, "send"), NOTHING),
            ("nameless", "order: 10\nsend:\n  target: none\n", NOTHING),
            ("quiet", &says("Quiet", 10, "nothing"), NOTHING),
            (
                "gone",
                "name: Gone\nentry: elsewhere.lua\nsend:\n  target: none\n",
                NOTHING,
            ),
            ("noise", "this: [is not\n  yaml\n", NOTHING),
        ],
    );

    let library = Library::load(std::slice::from_ref(&root));
    let found: Vec<&str> = library
        .entries()
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(found, ["fine"]);

    let said: Vec<String> = library
        .problems()
        .iter()
        .map(|problem| {
            format!(
                "{}: {}",
                problem
                    .path
                    .file_name()
                    .expect("it has a name")
                    .to_string_lossy(),
                problem.said
            )
        })
        .collect();
    assert_eq!(said.len(), 4, "{said:?}");
    assert!(
        said.iter().any(|one| one.starts_with("nameless")),
        "{said:?}"
    );
    assert!(
        said.iter()
            .any(|one| one.starts_with("quiet") && one.contains("neither direction")),
        "{said:?}"
    );
    assert!(
        said.iter()
            .any(|one| one.starts_with("gone") && one.contains("elsewhere.lua")),
        "{said:?}"
    );
    assert!(said.iter().any(|one| one.starts_with("noise")), "{said:?}");

    std::fs::remove_dir_all(&root).expect("taken away");
}

#[test]
fn listing_what_is_installed_runs_nothing_of_a_script() {
    let root = root(
        "listing",
        &[(
            "writes",
            &says("Writes as it loads", 10, "send"),
            "error(\"this must not be read\", 0)\n",
        )],
    );

    let library = Library::load(std::slice::from_ref(&root));
    assert_eq!(library.entries().len(), 1, "it is listed all the same");
    assert!(
        library.problems().is_empty(),
        "a manifest is a file, and nothing of the script was read: {:?}",
        library.problems()
    );

    let error = zyt_script::check(
        library.find("writes").expect("it is listed"),
        library.roots(),
    )
    .expect_err("asking whether it could run is what loads it");
    assert!(matches!(error, ScriptError::Load { .. }), "{error:?}");

    std::fs::remove_dir_all(&root).expect("taken away");
}

#[test]
fn a_name_nothing_carries_is_refused_by_name() {
    let library = Library::load(&[scripts()]);
    let error = library
        .find("nothing of the sort")
        .expect_err("it is refused");
    assert!(
        matches!(&error, ScriptError::NotFound { name } if name == "nothing of the sort"),
        "{error:?}"
    );
}

#[test]
fn only_the_scripts_offering_a_direction_are_offered_for_it() {
    let root = root(
        "directions",
        &[
            ("sends", &says("Sends", 10, "send"), NOTHING),
            ("takes", &says("Takes", 10, "receive"), NOTHING),
        ],
    );

    let library = Library::load(std::slice::from_ref(&root));
    let sending: Vec<&str> = library
        .offering(Direction::Send)
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    let taking: Vec<&str> = library
        .offering(Direction::Receive)
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(sending, ["sends"]);
    assert_eq!(taking, ["takes"]);

    std::fs::remove_dir_all(&root).expect("taken away");
}

#[test]
fn a_directory_without_a_manifest_is_not_a_script() {
    let root = root("library", &[("one", &says("One", 10, "send"), NOTHING)]);
    std::fs::create_dir_all(root.join("lib/fish")).expect("the tree is made");
    std::fs::write(root.join("lib/fish/wire.lua"), NOTHING).expect("written");

    let library = Library::load(std::slice::from_ref(&root));
    let found: Vec<&str> = library
        .entries()
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(found, ["one"], "lib/ is what require reads");
    assert!(library.problems().is_empty());

    let _ = Path::new("");
    std::fs::remove_dir_all(&root).expect("taken away");
}
