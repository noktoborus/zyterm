use super::*;

fn notice(line: &[u8]) -> Notice<'_> {
    match parse(line) {
        Line::Notice(notice) => notice,
        other => panic!("{other:?}"),
    }
}

#[test]
fn every_notification_reads() {
    assert_eq!(
        notice(b"%layout-change @0 b25d,80x24,0,0,0 b25d,80x24,0,0,0 *"),
        Notice::Layout {
            window: WindowId(0),
            layout: b"b25d,80x24,0,0,0",
            visible: Some(b"b25d,80x24,0,0,0"),
            zoomed: false,
        }
    );
    assert_eq!(
        notice(b"%layout-change @1 a,80x24,0,0{40x24,0,0,1,39x24,41,0,2} b,80x24,0,0,2 *Z"),
        Notice::Layout {
            window: WindowId(1),
            layout: b"a,80x24,0,0{40x24,0,0,1,39x24,41,0,2}",
            visible: Some(b"b,80x24,0,0,2"),
            zoomed: true,
        }
    );
    assert_eq!(
        notice(b"%layout-change @2 b25d,80x24,0,0,0"),
        Notice::Layout {
            window: WindowId(2),
            layout: b"b25d,80x24,0,0,0",
            visible: None,
            zoomed: false,
        }
    );
    let cases: &[(&[u8], Notice<'_>)] = &[
        (b"%window-add @1", Notice::WindowAdded(WindowId(1))),
        (b"%window-close @1", Notice::WindowClosed(WindowId(1))),
        (
            "%window-renamed @0 ќ".as_bytes(),
            Notice::WindowRenamed(WindowId(0), "ќ"),
        ),
        (
            b"%window-renamed @0 a b ",
            Notice::WindowRenamed(WindowId(0), "a b "),
        ),
        (
            b"%window-renamed @0",
            Notice::WindowRenamed(WindowId(0), ""),
        ),
        (
            b"%session-window-changed $0 @1",
            Notice::SessionWindowChanged(SessionId(0), WindowId(1)),
        ),
        (
            b"%window-pane-changed @1 %3",
            Notice::PaneChanged(WindowId(1), PaneId(3)),
        ),
        (
            b"%session-changed $0 zyt",
            Notice::SessionChanged(SessionId(0), "zyt"),
        ),
        (b"%sessions-changed", Notice::SessionsChanged),
        (b"%sessions-changed ", Notice::SessionsChanged),
        (
            b"%session-renamed $2 main",
            Notice::SessionRenamed(SessionId(2), "main"),
        ),
        (
            b"%unlinked-window-add @3",
            Notice::UnlinkedWindowAdded(WindowId(3)),
        ),
        (
            b"%unlinked-window-close @3",
            Notice::UnlinkedWindowClosed(WindowId(3)),
        ),
        (
            b"%unlinked-window-renamed @2 sh",
            Notice::UnlinkedWindowRenamed(WindowId(2), "sh"),
        ),
        (
            b"%subscription-changed zyterm-bells $0 @1 1 - : 1",
            Notice::SubscriptionChanged {
                name: "zyterm-bells",
                window: WindowId(1),
                value: "1",
            },
        ),
        (
            b"%subscription-changed name $0 - - - : a value",
            Notice::Unknown(b"%subscription-changed"),
        ),
        (b"%pause %0", Notice::Paused(PaneId(0))),
        (b"%continue %0 ", Notice::Continued(PaneId(0))),
        (b"%exit", Notice::Exit(None)),
        (b"%exit detached", Notice::Exit(Some("detached"))),
        (
            b"%exit too far behind",
            Notice::Exit(Some("too far behind")),
        ),
        (
            b"%client-detached /dev/pts/3",
            Notice::Unknown(b"%client-detached"),
        ),
        (
            b"%paste-buffer-changed buffer0",
            Notice::Unknown(b"%paste-buffer-changed"),
        ),
    ];
    for (line, expected) in cases {
        assert_eq!(&notice(line), expected, "{}", String::from_utf8_lossy(line));
    }
}

#[test]
fn output_and_extended_output_point_at_the_same_bytes() {
    let plain = b"%output %0 ls\\015\\012";
    let Line::Output { pane, age, start } = parse(plain) else {
        panic!()
    };
    assert_eq!(
        (pane, age, &plain[start..]),
        (PaneId(0), None, &b"ls\\015\\012"[..])
    );
    let extended = b"%extended-output %0 15 : ls\\015\\012";
    let Line::Output { pane, age, start } = parse(extended) else {
        panic!()
    };
    assert_eq!(
        (pane, age, &extended[start..]),
        (PaneId(0), Some(15), &b"ls\\015\\012"[..])
    );
    let spaces = b"%output %3   two spaces before  ";
    let Line::Output { start, .. } = parse(spaces) else {
        panic!()
    };
    assert_eq!(&spaces[start..], b"  two spaces before  ");
    let empty = b"%output %3";
    let Line::Output { start, .. } = parse(empty) else {
        panic!()
    };
    assert_eq!(&empty[start..], b"");
    let colon = b"%extended-output %1 0 : a : b";
    let Line::Output { start, .. } = parse(colon) else {
        panic!()
    };
    assert_eq!(&colon[start..], b"a : b");
}

#[test]
fn heads_of_replies_read() {
    let guard = Guard {
        stamp: (1792047849, 266),
        flags: 1,
    };
    assert!(matches!(parse(b"%begin 1792047849 266 1"), Line::Begin(read) if read == guard));
    assert!(matches!(parse(b"%end 1792047849 266 1"), Line::End(read) if read == guard));
    assert!(matches!(parse(b"%error 1792047849 266 1"), Line::Error(read) if read == guard));
}

#[test]
fn bad_lines_are_malformed_or_text() {
    for line in [
        &b"%output x"[..],
        b"%output %",
        b"%output %99999999999 x",
        b"%extended-output %0 x : y",
        b"%extended-output %0 1 no colon",
        b"%begin 1 2",
        b"%begin 1 2 3 4",
        b"%window-add 1",
        b"%window-add @1 @2",
        b"%window-pane-changed @1",
        b"%session-changed @1 x",
        b"%window-renamed @1 \xff",
    ] {
        assert!(
            matches!(parse(line), Line::Malformed),
            "{}",
            String::from_utf8_lossy(line)
        );
    }
    assert!(matches!(parse(b"$0 zyt"), Line::Text));
    assert!(matches!(parse(b""), Line::Text));
}

#[test]
fn a_notification_is_known_by_its_whole_head() {
    assert!(parse(b"%output %3 x").is_notification());
    assert!(parse(b"%layout-change @0 b25d,80x24,0,0,0").is_notification());
    assert!(parse(b"%begin 1 2 1").is_notification());
    assert!(!parse(b"%output of a screen").is_notification());
    assert!(!parse(b"%something else").is_notification());
    assert!(!parse(b"%end 1 2 1").is_notification());
    assert!(!parse(b"plain text").is_notification());
}
