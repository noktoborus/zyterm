//! A payload of known content, for feeding a terminal something it can be held
//! to.
//!
//! The pattern is built by hand rather than by the serializer of the crate,
//! because what it is for is reading the crate against something: a payload
//! both written and read by the same code says nothing about either.

/// The pixels one sixel character carries, one column six rows tall.
pub const BAND: usize = 6;

/// A test pattern of that size: six colour bars, a one pixel frame, and a
/// diagonal.
///
/// The frame says where the picture ends, which is what a terminal placing it
/// into cells is read against — a picture one pixel short of a row of cells
/// leaves part of a row empty, and the frame is how that is seen. The diagonal
/// says which way up it was drawn and whether anything was scaled unevenly.
pub fn test_pattern(width: usize, height: usize) -> String {
    let width = width.max(8);
    let height = height.max(BAND);
    let bands = height.div_ceil(BAND);

    let mut payload = String::from("\u{1b}Pq");
    payload.push_str(&format!("\"1;1;{width};{height}"));
    for (register, (red, green, blue)) in COLORS.iter().enumerate() {
        payload.push_str(&format!("#{register};2;{red};{green};{blue}"));
    }

    for band in 0..bands {
        let bar = band * COLORS.len() / bands.max(1);
        for register in 0..COLORS.len() {
            let mut row = String::new();
            for column in 0..width {
                let mut sixels = 0_u8;
                for pixel in 0..BAND {
                    let y = band * BAND + pixel;
                    if y >= height {
                        continue;
                    }
                    if paints(register, bar, column, y, width, height) {
                        sixels |= 1 << pixel;
                    }
                }
                row.push((b'?' + sixels) as char);
            }
            if row.bytes().any(|byte| byte != b'?') {
                payload.push_str(&format!("#{register}{row}$"));
            }
        }
        payload.push('-');
    }

    payload.push_str("\u{1b}\\");
    payload
}

/// Whether this register paints that pixel.
///
/// Three things are drawn, and the register of each is what keeps them apart:
/// the frame and the diagonal are the last colour, and everything inside is
/// the bar of its band.
fn paints(
    register: usize,
    bar: usize,
    column: usize,
    row: usize,
    width: usize,
    height: usize,
) -> bool {
    let edge = column == 0 || row == 0 || column + 1 == width || row + 1 == height;
    let diagonal = column * height == row * width;
    let last = COLORS.len() - 1;

    match register {
        marker if marker == last => edge || diagonal,
        other => other == bar && !edge && !diagonal,
    }
}

/// The colours of the bars, in hundredths as sixel says them; the last one is
/// what the frame and the diagonal are drawn in.
const COLORS: [(u8, u8, u8); 7] = [
    (100, 0, 0),
    (100, 65, 0),
    (100, 100, 0),
    (0, 100, 0),
    (0, 50, 100),
    (50, 0, 100),
    (100, 100, 100),
];
