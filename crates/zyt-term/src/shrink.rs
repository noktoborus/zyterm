//! Shrinking a picture to the limit it is kept to.
//!
//! A picture is drawn one pixel of it to one pixel of the screen, so a picture
//! larger than the limit is shrunk to it rather than cut: the whole of it stays
//! in sight, the proportions kept. One within the limit is left as it was
//! drawn, because enlarging adds no detail and costs memory.

use image::imageops::{self, FilterType};

use crate::sixel::{PictureLimit, Pixels};

/// What a sample of the result is made of.
///
/// A triangle widened to the ratio of the shrink averages every source pixel
/// that falls under a target one, which is what a shrink needs, at a fraction of
/// the cost of the wider filters.
const FILTER: FilterType = FilterType::Triangle;

/// The picture shrunk to fit inside the limit, or the picture itself where it
/// already fits.
pub(crate) fn to_fit(pixels: Pixels, limit: PictureLimit) -> crate::Result<Pixels> {
    let (width, height) = fitted((pixels.width, pixels.height), limit);
    if (width, height) == (pixels.width, pixels.height) {
        return Ok(pixels);
    }

    let mismatch = || crate::TermError::PixelsMismatch {
        width: pixels.width,
        height: pixels.height,
        bytes: pixels.rgba.len(),
    };
    let source = image::RgbaImage::from_raw(
        u32::try_from(pixels.width).map_err(|_| mismatch())?,
        u32::try_from(pixels.height).map_err(|_| mismatch())?,
        pixels.rgba.clone(),
    )
    .ok_or_else(mismatch)?;
    let target_width = u32::try_from(width).map_err(|_| mismatch())?;
    let target_height = u32::try_from(height).map_err(|_| mismatch())?;

    let mut rgba = imageops::resize(&source, target_width, target_height, FILTER).into_raw();
    rgba.as_chunks_mut::<4>()
        .0
        .iter_mut()
        .for_each(unpremultiply);

    Ok(Pixels {
        width,
        height,
        rgba,
    })
}

/// The size a picture of `size` comes to inside the limit, its proportions
/// kept and never larger than it was.
///
/// The side that is held is the one past the limit by the most; the other
/// follows it, rounded to the nearest pixel and never less than one.
fn fitted(size: (usize, usize), limit: PictureLimit) -> (usize, usize) {
    let (width, height) = size;
    let (most_wide, most_tall) = (limit.width.max(1), limit.height.max(1));
    if width <= most_wide && height <= most_tall {
        return size;
    }
    match width * most_tall > height * most_wide {
        true => (most_wide, rounded(height * most_wide, width)),
        false => (rounded(width * most_tall, height), most_tall),
    }
}

/// `numerator / denominator` rounded to the nearest whole, never less than one.
fn rounded(numerator: usize, denominator: usize) -> usize {
    ((numerator + denominator / 2) / denominator).max(1)
}

/// Turns a pixel the filter mixed back into a colour that is not weighed by its
/// alpha.
///
/// A sixel pixel is either painted and opaque or not painted and transparent
/// black, so the source already is premultiplied and the filter mixes it the
/// way it should: an edge is not darkened by the black of the pixels beside it.
/// Only the result has to be turned back, since a renderer takes colours that
/// are not weighed.
fn unpremultiply(pixel: &mut [u8; 4]) {
    let alpha = u16::from(pixel[3]);
    if alpha == 0 || alpha == u16::from(u8::MAX) {
        return;
    }
    for channel in &mut pixel[..3] {
        let unweighed = (u16::from(*channel) * u16::from(u8::MAX) + alpha / 2) / alpha;
        *channel = u8::try_from(unweighed).unwrap_or(u8::MAX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limit(width: usize, height: usize) -> PictureLimit {
        PictureLimit { width, height }
    }

    fn red(width: usize, height: usize) -> Pixels {
        Pixels {
            width,
            height,
            rgba: [255, 0, 0, 255].repeat(width * height),
        }
    }

    /// A picture within the limit is kept as it was drawn, not enlarged.
    #[test]
    fn a_picture_within_the_limit_is_left_alone() {
        let pixels = red(30, 20);

        let kept = to_fit(pixels.clone(), limit(800, 600)).expect("it fits");
        assert_eq!(kept, pixels);
    }

    /// A picture past the limit is shrunk to it, its proportions kept, by the
    /// side that is past it the most.
    #[test]
    fn a_picture_past_the_limit_is_shrunk_to_it() {
        assert_eq!(fitted((4000, 3000), limit(1040, 780)), (1040, 780));
        assert_eq!(
            fitted((400, 100), limit(200, 200)),
            (200, 50),
            "held by its width"
        );
        assert_eq!(
            fitted((100, 400), limit(200, 200)),
            (50, 200),
            "held by its height"
        );
        assert_eq!(
            fitted((10_000, 1), limit(100, 100)),
            (100, 1),
            "never less than a pixel"
        );

        let shrunk = to_fit(red(400, 100), limit(200, 200)).expect("it is shrunk");
        assert_eq!((shrunk.width, shrunk.height), (200, 50));
        assert_eq!(shrunk.rgba.len(), 200 * 50 * 4);
        assert_eq!(&shrunk.rgba[..4], &[255, 0, 0, 255], "and keeps its colour");
    }

    /// The edge of a picture beside pixels it never painted keeps its colour
    /// and loses only some of its alpha.
    #[test]
    fn an_edge_beside_transparent_pixels_keeps_its_colour() {
        let mut pixels = red(4, 1);
        pixels.rgba[4..8].copy_from_slice(&[0, 0, 0, 0]);
        pixels.rgba[12..16].copy_from_slice(&[0, 0, 0, 0]);

        let shrunk = to_fit(pixels, limit(2, 2)).expect("it is shrunk");
        assert_eq!((shrunk.width, shrunk.height), (2, 1));
        for pixel in shrunk.rgba.as_chunks::<4>().0 {
            assert!(pixel[3] > 0 && pixel[3] < u8::MAX, "half painted");
            assert_eq!(&pixel[..3], &[255, 0, 0], "and still red");
        }
    }
}
