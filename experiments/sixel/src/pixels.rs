//! The colours of a decoded payload, and what the picture comes to.

use sixel_image::{SixelColor, SixelImage};

/// What one picture holds.
pub struct Counted {
    /// Pixels of the whole rectangle.
    pub total: usize,
    /// Pixels a payload actually drew; the rest are transparent.
    pub on: usize,
    /// Runs of neighbouring pixels of one colour, counted row by row.
    ///
    /// It is the number of coloured rectangles the picture would come to as
    /// plain geometry, which is what drawing it without a texture would cost.
    pub runs: usize,
}

/// Counts the pixels and the runs of one picture.
pub fn count(image: &SixelImage) -> Counted {
    let mut counted = Counted {
        total: 0,
        on: 0,
        runs: 0,
    };

    for row in &image.pixels {
        counted.total += row.len();
        let mut last: Option<u16> = None;
        for pixel in row {
            if !pixel.on {
                last = None;
                continue;
            }
            counted.on += 1;
            if last != Some(pixel.color) {
                counted.runs += 1;
            }
            last = Some(pixel.color);
        }
    }
    counted
}

/// The colour of a register as the screen wants it, or nothing for a register
/// the payload never painted.
///
/// Sixel says its channels in hundredths and this crate hands them over that
/// way, so every one of them is scaled; a palette entry may also be said in
/// hue, lightness and saturation, which is turned into the same three channels
/// here rather than anywhere further on.
pub fn color_of(image: &SixelImage, register: u16) -> Option<[u8; 3]> {
    match image.color_registers.get(&register)? {
        SixelColor::Rgb(red, green, blue) => {
            Some([hundredth(*red), hundredth(*green), hundredth(*blue)])
        }
        SixelColor::Hsl(hue, lightness, saturation) => Some(from_hsl(
            *hue as f32,
            *lightness as f32 / 100.0,
            *saturation as f32 / 100.0,
        )),
    }
}

fn hundredth(value: u8) -> u8 {
    ((value.min(100) as u16 * 255) / 100) as u8
}

/// Hue in degrees, lightness and saturation as shares of one.
fn from_hsl(hue: f32, lightness: f32, saturation: f32) -> [u8; 3] {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = (hue.rem_euclid(360.0)) / 60.0;
    let second = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (red, green, blue) = match sector as u8 {
        0 => (chroma, second, 0.0),
        1 => (second, chroma, 0.0),
        2 => (0.0, chroma, second),
        3 => (0.0, second, chroma),
        4 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let lift = lightness - chroma / 2.0;
    let channel = |value: f32| (((value + lift).clamp(0.0, 1.0)) * 255.0).round() as u8;
    [channel(red), channel(green), channel(blue)]
}
