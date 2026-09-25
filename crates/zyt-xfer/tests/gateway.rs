//! Command lines, placeholder resolution and the gateway around a program.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use zyt_xfer::{
    CommandLine, CommandStep, Direction, Target, TargetKind, TransferCommands, TransferEvent,
    TransferJob, XferError, default_profiles,
};

fn wait_for(mut check: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if check() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

#[test]
fn bytes_travel_in_both_directions() {
    let job = TransferJob::start("cat", Direction::Send).expect("cat starts");

    job.feed(b"payload\n");
    let mut output = Vec::new();
    assert!(wait_for(|| {
        let mut chunk = Vec::new();
        job.take_output(&mut chunk);
        output.extend_from_slice(&chunk);
        output == b"payload\n"
    }));
}

#[test]
fn diagnostic_output_becomes_log_events() {
    let job =
        TransferJob::start("echo working >&2; exit 3", Direction::Send).expect("shell starts");

    let mut logs = Vec::new();
    let mut code = None;
    assert!(wait_for(|| {
        while let Some(event) = job.try_event() {
            match event {
                TransferEvent::Log(line) => logs.push(line),
                TransferEvent::Finished { code: exit } => code = Some(exit),
            }
        }
        code.is_some()
    }));
    assert_eq!(logs, vec!["working".to_string()]);
    assert_eq!(code, Some(Some(3)));
}

#[test]
fn shell_syntax_runs() {
    let job =
        TransferJob::start("printf one && printf two", Direction::Send).expect("shell starts");

    let mut output = Vec::new();
    assert!(wait_for(|| {
        let mut chunk = Vec::new();
        job.take_output(&mut chunk);
        output.extend_from_slice(&chunk);
        output == b"onetwo"
    }));
}

#[test]
fn empty_command_is_rejected() {
    let error = TransferJob::start("   ", Direction::Send).expect_err("nothing to run");
    assert!(matches!(error, XferError::EmptyCommand { .. }));

    let error = CommandLine::new("")
        .resolve("test", Target::None, &nothing())
        .unwrap_err();
    assert!(matches!(error, XferError::EmptyCommand { .. }));
}

#[test]
fn cancel_stops_the_program() {
    let job = TransferJob::start("sleep 30", Direction::Receive).expect("shell starts");
    job.cancel().expect("child can be stopped");
    assert!(wait_for(|| job.is_finished()));
}

#[test]
fn name_parts_are_available_to_both_lines() {
    let local = CommandLine::new("cat {>file}");
    let remote = CommandLine::new("cat > {:filename}");
    assert_eq!(local.target_kind(), TargetKind::File);
    assert_eq!(remote.target_kind(), TargetKind::File);

    let file = Path::new("/tmp/image.tar.gz");
    assert_eq!(
        local
            .resolve("cat", Target::File(file), &nothing())
            .unwrap(),
        "cat '/tmp/image.tar.gz'"
    );
    assert_eq!(
        remote
            .resolve("cat", Target::File(file), &nothing())
            .unwrap(),
        "cat > 'image.tar.gz'"
    );
    assert_eq!(
        CommandLine::new("echo {:stem} {:suffix}")
            .resolve("cat", Target::File(file), &nothing())
            .unwrap(),
        "echo 'image.tar' 'gz'"
    );
}

#[test]
fn a_path_is_quoted_as_one_word() {
    let quoted = zyt_xfer::quote_for_shell("/tmp/two words/it's here");

    if cfg!(windows) {
        assert_eq!(quoted, "\"/tmp/two words/it's here\"");
    } else {
        assert_eq!(quoted, r"'/tmp/two words/it'\''s here'");
    }
}

#[test]
fn a_directory_target_fills_the_name_parts_too() {
    let line = CommandLine::new("tar -C {>directory} -cf {:filename}.tar .");
    assert_eq!(line.target_kind(), TargetKind::Directory);
    assert_eq!(
        line.resolve("tar", Target::Directory(Path::new("/srv/data")), &nothing())
            .unwrap(),
        "tar -C '/srv/data' -cf 'data'.tar ."
    );
}

#[test]
fn paths_with_spaces_and_quotes_stay_one_argument() {
    let line = CommandLine::new("sz {>file}");
    assert_eq!(
        line.resolve(
            "zmodem",
            Target::File(Path::new("/tmp/a b'c.bin")),
            &nothing()
        )
        .unwrap(),
        r"sz '/tmp/a b'\''c.bin'"
    );
}

#[test]
fn the_wrong_kind_of_target_is_refused() {
    let line = CommandLine::new("cd {>directory} && rb");
    let error = line
        .resolve("ymodem", Target::File(Path::new("/tmp/a.bin")), &nothing())
        .expect_err("a file is not a directory");
    assert!(matches!(error, XferError::TargetMismatch { .. }));

    let error = CommandLine::new("sz {>file}")
        .resolve("zmodem", Target::None, &nothing())
        .expect_err("a file is needed");
    assert!(matches!(error, XferError::TargetMismatch { .. }));
}

#[test]
fn a_direction_takes_the_stronger_kind_of_both_lines() {
    let commands = TransferCommands::new(
        CommandStep::new(0, "tar -xf -"),
        CommandStep::new(0, "tar -C {>directory} -cf - ."),
    );
    assert_eq!(commands.target_kind(), TargetKind::Directory);
    assert!(commands.is_available());

    let commands = TransferCommands::new(CommandStep::default(), CommandStep::new(0, "rz -y"));
    assert!(!commands.is_available());

    let commands = TransferCommands::new(
        CommandStep::new(500, "cat {>file}"),
        CommandStep::new(0, "cat > {:filename}"),
    );
    assert_eq!(commands.target_kind(), TargetKind::File);
    assert_eq!(commands.local.delay().as_millis(), 500);
    assert_eq!(commands.remote.delay().as_millis(), 0);
}

/// The order of the shipped list is the order they are offered in.
///
/// `Shell Transfer` stands first because it is the only one that needs nothing
/// installed on the device, so it is the one most likely to work on a board
/// nobody has prepared.
#[test]
fn the_shipped_profiles_are_offered_in_the_order_they_are_shipped() {
    let shipped = default_profiles();
    let names: Vec<&str> = shipped
        .iter()
        .map(|profile| profile.name.as_str())
        .collect();

    assert_eq!(
        names,
        [
            "Shell Transfer",
            "ymodem",
            "xmodem",
            "zmodem",
            "SCP to remote PWD",
            "Cat file"
        ]
    );
}

#[test]
fn shipped_profiles_cover_the_simple_cases() {
    let profiles = default_profiles();

    let cat = profiles
        .iter()
        .find(|profile| profile.name == "Cat file")
        .expect("the cat profile exists");
    assert_eq!(
        cat.commands(Direction::Send).local.line.as_str(),
        "cat {>file}"
    );
    assert_eq!(
        cat.commands(Direction::Send).remote.line.as_str(),
        "cat > {:filename}"
    );
    assert!(cat.commands(Direction::Send).is_available());
    assert!(!cat.commands(Direction::Receive).is_available());

    assert!(
        profiles
            .iter()
            .all(|profile| profile.name != "Tar Transfer"),
        "the tar profile is gone"
    );
}

#[test]
fn nothing_is_lost_when_the_program_ends_at_once() {
    for _ in 0..20 {
        let job = TransferJob::start("printf payload", Direction::Send).expect("shell starts");

        let mut output = Vec::new();
        assert!(wait_for(|| {
            let mut chunk = Vec::new();
            job.take_output(&mut chunk);
            output.extend_from_slice(&chunk);
            job.is_finished() && !output.is_empty()
        }));
        assert_eq!(output, b"payload".to_vec());
    }
}

#[test]
fn the_finish_key_has_a_readable_text_form() {
    let named = [
        ("esc", vec![0x1b]),
        ("Escape", vec![0x1b]),
        ("enter", vec![0x0d]),
        ("lf", vec![0x0a]),
        ("tab", vec![0x09]),
        ("ctrl+c", vec![0x03]),
        ("ctrl+d", vec![0x04]),
        ("ctrl+[", vec![0x1b]),
        (r"\x03", vec![0x03]),
        (r"\r\n", vec![0x0d, 0x0a]),
        ("quit", b"quit".to_vec()),
        ("", Vec::new()),
    ];

    for (text, expected) in named {
        let commands = TransferCommands::new(CommandStep::new(0, "cat"), CommandStep::default())
            .finished_by(text);
        assert_eq!(
            commands.finish_bytes("test").expect("the key is valid"),
            expected,
            "{text}"
        );
    }
}

#[test]
fn a_broken_finish_key_is_reported() {
    for text in ["ctrl+", "ctrl+shift", r"\x0", r"\q"] {
        let commands = TransferCommands::new(CommandStep::new(0, "cat"), CommandStep::default())
            .finished_by(text);
        let error = commands
            .finish_bytes("test")
            .expect_err("the key is broken");
        assert!(
            matches!(error, XferError::InvalidFinishKey { .. }),
            "{text}"
        );
    }
}

#[test]
fn the_shipped_profiles_stop_the_program_on_the_device() {
    let name = "Cat file";
    let profile = default_profiles()
        .into_iter()
        .find(|profile| profile.name == name)
        .expect("the profile exists");
    let send = profile.commands(Direction::Send);

    assert_eq!(send.finish_label().as_deref(), Some("ctrl+c"));
    assert_eq!(send.finish_bytes(name).unwrap(), vec![0x03]);
    assert!(
        profile
            .commands(Direction::Receive)
            .finish_label()
            .is_none()
    );
}

#[test]
fn several_files_become_several_quoted_arguments() {
    let line = CommandLine::new("sh-xfer put {>files}");
    assert_eq!(line.target_kind(), TargetKind::Files);

    let paths = vec![
        PathBuf::from("/tmp/one.bin"),
        PathBuf::from("/tmp/two words.bin"),
    ];
    assert_eq!(
        line.resolve("test", Target::Files(&paths), &nothing())
            .expect("resolves"),
        "sh-xfer put '/tmp/one.bin' '/tmp/two words.bin'"
    );
}

#[test]
fn several_directories_become_several_quoted_arguments() {
    let line = CommandLine::new("tar -cf - {>directories}");
    assert_eq!(line.target_kind(), TargetKind::Directories);

    let paths = vec![PathBuf::from("/srv/a"), PathBuf::from("/srv/b")];
    assert_eq!(
        line.resolve("test", Target::Directories(&paths), &nothing())
            .expect("resolves"),
        "tar -cf - '/srv/a' '/srv/b'"
    );
}

#[test]
fn the_name_parts_of_several_files_are_listed_too() {
    let line = CommandLine::new("echo {:filename} && sz {>files}");
    assert_eq!(line.target_kind(), TargetKind::Files);

    let paths = vec![
        PathBuf::from("/tmp/one.tar.gz"),
        PathBuf::from("/tmp/two.bin"),
    ];
    assert_eq!(
        line.resolve("test", Target::Files(&paths), &nothing())
            .expect("resolves"),
        "echo 'one.tar.gz' 'two.bin' && sz '/tmp/one.tar.gz' '/tmp/two.bin'"
    );
}

#[test]
fn a_name_on_its_own_still_means_one_file() {
    let line = CommandLine::new("rx {:filename}");
    assert_eq!(line.target_kind(), TargetKind::File);
}

#[test]
fn one_path_and_many_are_told_apart() {
    let one = CommandLine::new("sz {>file}");
    let many = CommandLine::new("sz {>files}");
    let paths = vec![PathBuf::from("/tmp/one.bin")];

    assert!(matches!(
        one.resolve("test", Target::Files(&paths), &nothing()),
        Err(XferError::TargetMismatch { .. })
    ));
    assert!(matches!(
        many.resolve("test", Target::File(Path::new("/tmp/one.bin")), &nothing()),
        Err(XferError::TargetMismatch { .. })
    ));
}

#[test]
fn a_direction_takes_the_stronger_kind_of_both_lines_with_many() {
    let commands = TransferCommands::new(
        CommandStep::new(0, "sh-xfer put {>files}"),
        CommandStep::default(),
    );
    assert_eq!(commands.target_kind(), TargetKind::Files);
    assert!(commands.is_available());

    assert!(TargetKind::Files.is_multiple());
    assert!(!TargetKind::Files.is_directory());
    assert!(TargetKind::Directories.is_multiple());
    assert!(TargetKind::Directories.is_directory());
    assert!(!TargetKind::File.is_multiple());
}

#[cfg(unix)]
#[test]
fn a_transfer_keeps_its_pipe_and_its_lines() {
    let job = TransferJob::start(
        "if [ -t 2 ]; then echo TTY >&2; else echo PIPE >&2; fi",
        Direction::Send,
    )
    .expect("shell starts");

    let mut said = Vec::new();
    assert!(wait_for(|| {
        while let Some(event) = job.try_event() {
            if let TransferEvent::Log(line) = event {
                said.push(line);
            }
        }
        said.iter().any(|line| line == "PIPE")
    }));
}

#[test]
fn a_direction_without_the_console_key_parses() {
    let plain: TransferCommands =
        serde_yaml_ng::from_str("local: {delay_ms: 0, line: sz}\nremote: {delay_ms: 0, line: ''}")
            .expect("a direction that leaves the key out parses");
    assert_eq!(plain.local.line.as_str(), "sz");

    let with_console: TransferCommands = serde_yaml_ng::from_str(
        "local: {delay_ms: 0, line: rz}\nremote: {delay_ms: 0, line: ''}\nconsole: true",
    )
    .expect("a direction written while the key existed parses");
    assert_eq!(with_console.local.line.as_str(), "rz");
}

#[test]
fn every_shipped_profile_holds_the_line() {
    for profile in default_profiles() {
        assert!(
            profile.pty,
            "{} talks over the console of the device",
            profile.name
        );
    }
}

#[test]
fn a_profile_written_before_the_flag_holds_the_line() {
    let text = "
- name: mine
  send:
    local:
      delay_ms: 0
      line: cat {>file}
    remote:
      delay_ms: 0
      line: cat > {:filename}
    finish: ctrl+c
  receive:
    local:
      delay_ms: 0
      line: ''
    remote:
      delay_ms: 0
      line: ''
    finish: ''
";
    let profiles: Vec<zyt_xfer::TransferProfile> =
        serde_yaml_ng::from_str(text).expect("the file is read");

    assert!(profiles[0].pty, "a file that says nothing means the line");
}

#[test]
fn a_profile_beside_the_line_types_nothing_and_sends_no_key() {
    let mut profile = default_profiles()
        .into_iter()
        .find(|profile| profile.name == "Cat file")
        .expect("the profile exists");

    assert!(profile.remote(Direction::Send).is_some());
    assert_eq!(profile.finish(Direction::Send), "ctrl+c");

    profile.pty = false;

    assert!(profile.remote(Direction::Send).is_none());
    assert_eq!(profile.finish(Direction::Send), zyt_xfer::FINISH_NONE);
}

/// A source that keeps no values, which is what most of these lines ask for.
fn nothing() -> std::collections::BTreeMap<String, String> {
    std::collections::BTreeMap::new()
}

#[test]
fn a_line_names_the_values_of_the_source_it_asks_for() {
    let line = CommandLine::new("scp {>files} {remote_user}@{remote_host}:{} {remote_user}");

    assert_eq!(line.variables(), ["remote_user", "remote_host"]);
    assert_eq!(line.target_kind(), zyt_xfer::TargetKind::Files);
    assert!(CommandLine::new("scp {>files}").variables().is_empty());
}

#[test]
fn a_value_of_the_source_is_put_in_quoted_and_the_braces_of_the_program_are_left_alone() {
    let line = CommandLine::new("sh-xfer pwd-exec scp -- -O {>files} {user}@{host}:{}");
    let mut values = std::collections::BTreeMap::new();
    values.insert("user".to_string(), "root".to_string());
    values.insert("host".to_string(), "192.168.1.1".to_string());
    let paths = vec![std::path::PathBuf::from("/tmp/one bin")];

    let resolved = line
        .resolve("scp", Target::Files(&paths), &values)
        .expect("every value is there");

    assert_eq!(
        resolved,
        "sh-xfer pwd-exec scp -- -O '/tmp/one bin' 'root'@'192.168.1.1':{}"
    );
}

#[test]
fn a_value_the_source_has_not_got_stops_the_transfer() {
    let line = CommandLine::new("scp {>file} {host}:/tmp");
    let file = std::path::Path::new("/tmp/one.bin");

    let mut blank = std::collections::BTreeMap::new();
    blank.insert("host".to_string(), "   ".to_string());

    for values in [nothing(), blank] {
        assert!(matches!(
            line.resolve("scp", Target::File(file), &values),
            Err(zyt_xfer::XferError::UnsetVariable { name, .. }) if name == "host"
        ));
    }
}

#[test]
fn a_profile_names_every_value_it_asks_for() {
    let mut profile = zyt_xfer::TransferProfile {
        name: "scp".to_string(),
        pty: true,
        send: zyt_xfer::TransferCommands::new(
            zyt_xfer::CommandStep::new(0, "scp {>files} {host}:/tmp"),
            zyt_xfer::CommandStep::new(0, "echo {greeting}"),
        ),
        receive: zyt_xfer::TransferCommands::default(),
    };

    assert_eq!(profile.variables(), ["host", "greeting"]);

    profile.pty = false;
    assert_eq!(
        profile.variables(),
        ["host"],
        "a profile beside the line never types the second line"
    );
}

#[test]
fn the_scp_profile_asks_the_device_where_it_stands_and_the_source_who_to_reach() {
    let profile = default_profiles()
        .into_iter()
        .find(|profile| profile.name == zyt_xfer::SCP_TO_REMOTE_PWD)
        .expect("the profile is shipped");

    assert!(profile.pty, "it asks the device on its own console");
    assert_eq!(profile.variables(), ["remote_user", "remote_host"]);
    assert_eq!(
        profile.commands(Direction::Send).target_kind(),
        zyt_xfer::TargetKind::Files
    );
    assert!(!profile.commands(Direction::Receive).is_available());
    assert_eq!(
        profile.commands(Direction::Send).finish_label().as_deref(),
        Some("enter"),
        "sh-xfer leaves the shell of the device a line of its own to read"
    );

    let paths = vec![std::path::PathBuf::from("/tmp/one.bin")];
    let mut values = std::collections::BTreeMap::new();
    values.insert("remote_user".to_string(), "root".to_string());
    values.insert("remote_host".to_string(), "192.168.1.1".to_string());

    let line = profile
        .commands(Direction::Send)
        .local
        .line
        .resolve(&profile.name, Target::Files(&paths), &values)
        .expect("both values are there");

    assert_eq!(
        line,
        "sh-xfer pwd-exec scp -- -v -O '/tmp/one.bin' 'root'@'192.168.1.1':{}"
    );
}

#[test]
fn only_a_plain_name_is_a_value_of_the_source() {
    for name in ["host", "remote_host", "remote-host", "h2", "A"] {
        assert!(zyt_xfer::is_variable_name(name), "{name}");
    }
    for name in [
        "",
        " ",
        "a b",
        "a:b",
        ">file",
        ":filename",
        "a.b",
        "a/b",
        "путь",
    ] {
        assert!(!zyt_xfer::is_variable_name(name), "{name}");
    }

    let line = CommandLine::new("cp {>file} {:filename} {} {a b} {host}");
    assert_eq!(line.variables(), ["host"]);
}
