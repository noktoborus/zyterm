use super::*;

fn pane(columns: u32, rows: u32, x: u32, y: u32, id: u32) -> Layout {
    Layout {
        columns,
        rows,
        x,
        y,
        content: Content::Pane(PaneId(id)),
    }
}

/// Puts the checksum of tmux before a layout written by hand.
fn summed(body: &str) -> Vec<u8> {
    format!("{:04x},{body}", checksum(body.as_bytes())).into_bytes()
}

#[test]
fn one_pane() {
    assert_eq!(
        Layout::parse(b"b25d,80x24,0,0,0"),
        Ok(pane(80, 24, 0, 0, 0))
    );
    assert_eq!(
        Layout::parse(b"b25e,80x24,0,0,1"),
        Ok(pane(80, 24, 0, 0, 1))
    );
}

#[test]
fn split_left_right_and_top_bottom() {
    assert_eq!(
        Layout::parse(b"8205,80x24,0,0{40x24,0,0,0,39x24,41,0,1}"),
        Ok(Layout {
            columns: 80,
            rows: 24,
            x: 0,
            y: 0,
            content: Content::Columns(vec![pane(40, 24, 0, 0, 0), pane(39, 24, 41, 0, 1)]),
        })
    );
    assert_eq!(
        Layout::parse(b"c1a7,80x24,0,0[80x12,0,0,4,80x11,0,13,5]"),
        Ok(Layout {
            columns: 80,
            rows: 24,
            x: 0,
            y: 0,
            content: Content::Rows(vec![pane(80, 12, 0, 0, 4), pane(80, 11, 0, 13, 5)]),
        })
    );
}

#[test]
fn nested() {
    let layout = Layout::parse(
        b"1558,80x24,0,0{40x24,0,0,0,39x24,41,0[39x12,41,0,1,39x11,41,13{19x11,41,13,2,19x11,61,13,3}]}",
    )
    .unwrap();
    assert_eq!(layout.panes(), [PaneId(0), PaneId(1), PaneId(2), PaneId(3)]);
    assert_eq!(layout.find(PaneId(3)), Some(&pane(19, 11, 61, 13, 3)));
    assert_eq!(layout.find(PaneId(9)), None);
}

#[test]
fn the_visible_layout_of_a_zoomed_window() {
    let line = b"%layout-change @0 1558,80x24,0,0{40x24,0,0,0,39x24,41,0[39x12,41,0,1,39x11,41,13{19x11,41,13,2,19x11,61,13,3}]} b25e,80x24,0,0,1 *Z";
    let crate::line::Line::Notice(crate::line::Notice::Layout {
        layout,
        visible,
        zoomed,
        ..
    }) = crate::line::parse(line)
    else {
        panic!()
    };
    assert!(zoomed);
    assert_eq!(Layout::parse(layout).unwrap().panes().len(), 4);
    assert_eq!(Layout::parse(visible.unwrap()), Ok(pane(80, 24, 0, 0, 1)));
}

#[test]
fn a_bad_checksum() {
    assert_eq!(
        Layout::parse(b"b25d,80x24,0,0,1"),
        Err(Error::LayoutChecksum {
            written: 0xb25d,
            counted: 0xb25e
        })
    );
}

#[test]
fn malformed_layouts() {
    for text in [
        &b""[..],
        b"b25d",
        b"zzzz,80x24,0,0,0",
        b"b25d80x24,0,0,0",
        &summed("80x24,0,0"),
        &summed("80x24,0,0,"),
        &summed("80x24,0,0,0,"),
        &summed("80x24,0,0{40x24,0,0,0"),
        &summed("80x24,0,0{40x24,0,0,0]"),
        &summed("80x24,0,0{}"),
        &summed("80x24,0,0,%1"),
        &summed("80x24,0,0,99999999999"),
        &summed("80y24,0,0,0"),
    ] {
        assert_eq!(
            Layout::parse(text),
            Err(Error::MalformedLayout),
            "{}",
            String::from_utf8_lossy(text)
        );
    }
}

#[test]
fn nesting_is_held_to_its_depth() {
    let nest = |depth: usize| {
        let body = format!("{}1x1,0,0,0{}", "1x1,0,0{".repeat(depth), "}".repeat(depth));
        summed(&body)
    };
    assert!(Layout::parse(&nest(MAX_DEPTH)).is_ok());
    assert_eq!(
        Layout::parse(&nest(MAX_DEPTH + 1)),
        Err(Error::MalformedLayout)
    );
    assert_eq!(Layout::parse(&nest(100_000)), Err(Error::MalformedLayout));
}
