//! The shipped scripts, and the shell they carry.
//!
//! Every template of the protocol is a file of its own, folded into the one
//! line a wire takes before it is sent. What a fold does to a line that was
//! not written for it is quiet and wrong — a construct left open, a comment
//! swallowing the rest — so the shape of every template is asserted here, and
//! every one of them is handed to a shell to parse.

mod fish;

use std::path::{Path, PathBuf};

/// The values every hole is filled with for a syntax check.
fn sample() -> Vec<(String, String)> {
    [
        ("path", "'/tmp/one.bin'"),
        ("program", "\\sha1sum"),
        ("offset", "2965"),
        ("count", "1482"),
        ("heredoc", "SHXFER_EOF"),
    ]
    .iter()
    .map(|(name, value)| (name.to_string(), value.to_string()))
    .collect()
}

/// Every template of the protocol, by the path it stands at.
fn templates() -> Vec<PathBuf> {
    let mut found = Vec::new();
    walk(&fish::shipped().join("lib/fish/sh"), "sh", &mut found);
    assert!(
        found.len() >= 18,
        "every command of the protocol has a file of its own: {found:?}"
    );
    found
}

/// Every file of that extension under that directory.
fn walk(root: &Path, extension: &str, into: &mut Vec<PathBuf>) {
    let Ok(listed) = std::fs::read_dir(root) else {
        return;
    };
    for entry in listed.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, extension, into);
        } else if path.extension().is_some_and(|kind| kind == extension) {
            into.push(path);
        }
    }
    into.sort();
}

/// What a file of a template holds.
fn body(path: &Path) -> String {
    std::fs::read_to_string(path).expect("the template is there")
}

/// The name a complaint calls a template by.
fn named(path: &Path) -> String {
    path.strip_prefix(fish::shipped())
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

#[test]
fn no_template_carries_the_marker_of_a_reply() {
    for path in templates() {
        let folded = zyt_script::fold(&body(&path));
        assert!(
            !folded.contains("###"),
            "{} says the marker plainly, and a console would echo it back as an answer",
            named(&path)
        );
        assert!(
            !folded.contains('\n'),
            "{} would leave the shell of the device waiting",
            named(&path)
        );
    }
}

#[test]
fn every_template_is_written_the_way_it_folds() {
    for path in templates() {
        let text = body(&path);
        assert!(
            !text.contains('\t'),
            "{} is indented with tabs, which a fold turns into nothing",
            named(&path)
        );

        let mut lines = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .peekable();
        while let Some(line) = lines.next() {
            if lines.peek().is_none() {
                break;
            }
            let ends_open = [";", "do", "then", "else", "in", "(", ")"]
                .iter()
                .any(|ending| line.ends_with(ending));
            assert!(
                ends_open,
                "{} has a line the fold would run into the next one:\n{line}",
                named(&path)
            );
        }
    }
}

#[test]
fn every_template_is_shell_a_shell_accepts() {
    for path in templates() {
        let rendered = zyt_script::fill(&zyt_script::fold(&body(&path)), &sample());
        assert!(
            !rendered.contains('{'),
            "{} has a hole nothing answered to:\n{rendered}",
            named(&path)
        );

        let checked = std::process::Command::new("sh")
            .arg("-n")
            .arg("-c")
            .arg(&rendered)
            .output()
            .expect("a shell runs");
        assert!(
            checked.status.success(),
            "{} is not shell a shell accepts:\n{rendered}\n{}",
            named(&path),
            String::from_utf8_lossy(&checked.stderr)
        );
    }
}

#[test]
fn nothing_a_template_sets_outlives_the_command_that_set_it() {
    for path in templates() {
        let rendered = zyt_script::fill(&zyt_script::fold(&body(&path)), &sample());
        assert!(
            !rendered.contains("SHXFER_STTY"),
            "{} keeps the settings of the line in a variable of the device:\n{rendered}",
            named(&path)
        );
        if rendered.contains("stty raw") || rendered.contains("stty -echo") {
            assert!(
                rendered.contains("stty -raw echo"),
                "{} changes the line and does not put it back:\n{rendered}",
                named(&path)
            );
        }
    }
}

#[test]
fn every_shipped_script_is_a_directory_with_a_manifest_and_loads() {
    let roots = vec![fish::shipped()];
    let library = zyt_script::Library::load(&roots);

    assert!(
        library.problems().is_empty(),
        "nothing shipped is broken: {:?}",
        library.problems()
    );
    assert!(library.entries().len() >= 7, "{:?}", library.entries());

    for entry in library.entries() {
        assert!(
            entry.manifest_path().is_file(),
            "{} says what it is in a file of its own",
            entry.id
        );
        assert!(
            entry.path.is_file(),
            "{} names a script that is there",
            entry.id
        );
        assert!(
            !entry.name().is_empty() && entry.name() != entry.id,
            "{} carries a name a person reads: {}",
            entry.id,
            entry.name()
        );
        assert!(
            entry.manifest.offers(zyt_script::Direction::Send)
                || entry.manifest.offers(zyt_script::Direction::Receive),
            "{} offers a direction",
            entry.id
        );

        zyt_script::check(entry, &roots)
            .unwrap_or_else(|error| panic!("{} does not load: {error}", entry.id));
    }
}

#[test]
fn the_names_a_person_reads_are_the_ones_that_were_asked_for() {
    let library = zyt_script::Library::load(&[fish::shipped()]);
    let named: Vec<(&str, &str)> = library
        .entries()
        .iter()
        .map(|entry| (entry.id.as_str(), entry.name()))
        .collect();

    for (id, name) in [
        ("shell-transfer", "Shell Transfer"),
        ("zmodem", "ZModem"),
        ("ymodem", "YModem"),
        ("xmodem", "XModem"),
        ("cat-file", "Cat file"),
        ("shell-driven-scp", "Shell-driven SCP"),
    ] {
        assert!(
            named.contains(&(id, name)),
            "{id} is called {name}: {named:?}"
        );
    }
}

#[test]
fn every_shipped_script_offers_what_the_menus_ask_it_for() {
    let library = zyt_script::Library::load(&[fish::shipped()]);

    /// What one direction of a script is expected to ask for.
    type Offer = (zyt_script::Direction, zyt_script::TargetKind, &'static str);

    let expected: &[(&str, &[Offer])] = &[
        (
            "shell-transfer",
            &[
                (
                    zyt_script::Direction::Send,
                    zyt_script::TargetKind::Files,
                    "enter",
                ),
                (
                    zyt_script::Direction::Receive,
                    zyt_script::TargetKind::Directory,
                    "enter",
                ),
            ],
        ),
        (
            "shell-list",
            &[(
                zyt_script::Direction::Send,
                zyt_script::TargetKind::None,
                "enter",
            )],
        ),
        (
            "zmodem",
            &[
                (
                    zyt_script::Direction::Send,
                    zyt_script::TargetKind::File,
                    "",
                ),
                (
                    zyt_script::Direction::Receive,
                    zyt_script::TargetKind::Directory,
                    "",
                ),
            ],
        ),
        (
            "ymodem",
            &[
                (
                    zyt_script::Direction::Send,
                    zyt_script::TargetKind::File,
                    "",
                ),
                (
                    zyt_script::Direction::Receive,
                    zyt_script::TargetKind::Directory,
                    "",
                ),
            ],
        ),
        (
            "xmodem",
            &[
                (
                    zyt_script::Direction::Send,
                    zyt_script::TargetKind::File,
                    "",
                ),
                (
                    zyt_script::Direction::Receive,
                    zyt_script::TargetKind::File,
                    "",
                ),
            ],
        ),
        (
            "cat-file",
            &[(
                zyt_script::Direction::Send,
                zyt_script::TargetKind::File,
                "ctrl+c",
            )],
        ),
        (
            "shell-driven-scp",
            &[
                (
                    zyt_script::Direction::Send,
                    zyt_script::TargetKind::Files,
                    "enter",
                ),
                (
                    zyt_script::Direction::Receive,
                    zyt_script::TargetKind::Directory,
                    "enter",
                ),
            ],
        ),
    ];

    for (name, directions) in expected {
        let entry = library
            .find(name)
            .unwrap_or_else(|error| panic!("{name} is shipped: {error}"));
        for (way, kind, finish) in *directions {
            let spec = entry
                .manifest
                .direction(*way)
                .unwrap_or_else(|| panic!("{name} offers {}", way.name()));
            assert_eq!(spec.target, *kind, "{name} {}", way.name());
            assert_eq!(spec.finish, *finish, "{name} {}", way.name());
        }
        assert!(entry.manifest.hold_line, "{name} is a transfer on the line");
        for other in [zyt_script::Direction::Send, zyt_script::Direction::Receive] {
            let offered = directions.iter().any(|(way, _, _)| *way == other);
            assert_eq!(
                entry.manifest.offers(other),
                offered,
                "{name} and {}",
                other.name()
            );
        }
    }
}

#[test]
fn the_script_that_asks_the_source_for_a_value_says_which() {
    let library = zyt_script::Library::load(&[fish::shipped()]);
    let entry = library.find("shell-driven-scp").expect("it is shipped");
    assert_eq!(entry.manifest.variables, ["remote_user", "remote_host"]);

    for name in ["shell-transfer", "zmodem", "cat-file"] {
        let entry = library.find(name).expect("it is shipped");
        assert!(
            entry.manifest.variables.is_empty(),
            "{name} asks the source for nothing"
        );
    }
}

#[test]
fn the_library_of_the_scripts_is_not_offered_as_one() {
    let library = zyt_script::Library::load(&[fish::shipped()]);
    let names: Vec<&str> = library
        .entries()
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();

    assert!(names.contains(&"shell-transfer"), "{names:?}");
    assert!(names.contains(&"shell-list"), "{names:?}");
    assert!(
        !names
            .iter()
            .any(|name| *name == "session" || *name == "wire"),
        "the library under lib/ is what require reads, not a script: {names:?}"
    );
    assert!(
        library.problems().is_empty(),
        "nothing shipped is broken: {:?}",
        library.problems()
    );
}
