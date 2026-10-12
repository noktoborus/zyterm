//! Sixel payloads, picked up beside the emulation.
//!
//! The backend knows nothing of sixel: a device control string reaches no cell
//! and is written to the log and nowhere else. So a payload is picked up the
//! way the operating system commands are, out of the same bytes and by the same
//! parser, through [`Perform::hook`], [`Perform::put`] and
//! [`Perform::unhook`].
//!
//! What is read while it arrives is what the payload says about itself — the
//! size its raster attribute declares, and how many bytes it came to — and the
//! payload itself, kept as it arrived. Nothing is decoded here: the bytes are
//! what a caller saves, hands to another program or puts on the clipboard,
//! whether anything ever draws them or not.
//!
//! [`Perform::hook`]: alacritty_terminal::vte::Perform::hook
//! [`Perform::put`]: alacritty_terminal::vte::Perform::put
//! [`Perform::unhook`]: alacritty_terminal::vte::Perform::unhook

use sixel_image::{SixelColor, SixelDeserializer};
use sixel_tokenizer::{Parser, SixelEvent};

/// The final character of the device control string that carries a picture.
const SIXEL: char = 'q';

/// What one sixel payload said about itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SixelReport {
    /// Bytes of the payload: what stood between the introducer and the
    /// terminator, neither of them counted.
    pub bytes: usize,
    /// Pixels the raster attribute declared, when the payload declared both.
    ///
    /// Nothing where the payload named no raster attribute or left a side of it
    /// out, which the standard allows: the size is then the extent of the data
    /// and is not known until the data has been read.
    pub declared: Option<(usize, usize)>,
    /// Whether the introducer asked for the background to be left alone.
    ///
    /// It is the second parameter of the device control string. A picture that
    /// asked for it paints the pixels it drew and nothing else; one that did
    /// not has its whole rectangle painted, the untouched pixels in the colour
    /// of the register in hand. It is kept because the payload alone does not
    /// say it and the pixels differ by it.
    pub transparent: bool,
    /// Why nothing of the picture was kept, where the terminal refused it.
    ///
    /// It is settled after the payload has been read and decoded, so a report
    /// fresh from the payload says nothing; the line of the picture is asked
    /// for once it is known. A payload the decoder could not read is not a
    /// refusal and says nothing either: there was no picture to refuse.
    pub refused: Option<SixelRefusal>,
}

/// Why a terminal kept nothing of a picture a program sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SixelRefusal {
    /// Pictures are not read at all ([`PictureAccess::Refused`]).
    Ignored,
    /// The picture weighed more than one picture may, its payload and its
    /// pixels together.
    TooLarge,
}

/// What a picture the terminal does not draw leaves on the page in its place.
///
/// The crate holds no text of its own, so the words are the caller's: it is
/// handed what the payload said about itself and answers with the line to put
/// where the picture would have stood.
pub type SixelPlaceholder = std::sync::Arc<dyn Fn(&SixelReport) -> String + Send + Sync + 'static>;

/// The most one picture may weigh, its payload and its pixels together, while
/// the caller has said nothing (`Terminal::set_picture_weight`).
///
/// A payload is held whole while it arrives, so it is also how long a payload
/// may grow before the rest of it is thrown away. A picture heavier than this
/// is kept not at all, and its line says it was too large.
pub const DEFAULT_PICTURE_WEIGHT: usize = 16 * 1024 * 1024;

/// The largest picture a terminal keeps while the caller has said nothing
/// (`Terminal::set_picture_limit`).
///
/// A picture past it is shrunk to fit it, its proportions kept; one within it
/// is kept as it was drawn.
///
/// It is also the size a program is told about when it asks what this terminal
/// takes, so a program that believes the answer sends what we keep and nothing
/// has to be shrunk.
pub const DEFAULT_PICTURE_LIMIT: (usize, usize) = (1920, 1080);

/// The largest canvas a payload is read into before it is shrunk to the limit,
/// unless the limit is larger still.
///
/// A payload is short and a canvas is not: a raster attribute of four lines
/// asks for however many pixels it likes, and the bytes of them are four times
/// that again. So a picture past this is cut to it, the top left corner kept,
/// and only then shrunk.
pub const MAX_CANVAS: (usize, usize) = (4096, 4096);

/// What a picture is shrunk to fit, and what a program asking is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PictureLimit {
    /// Widest picture in pixels.
    pub width: usize,
    /// Tallest picture in pixels.
    pub height: usize,
}

impl Default for PictureLimit {
    fn default() -> Self {
        Self {
            width: DEFAULT_PICTURE_LIMIT.0,
            height: DEFAULT_PICTURE_LIMIT.1,
        }
    }
}

/// What a terminal does with the pictures a program sends.
///
/// The three are not degrees of one thing: the middle one keeps a picture
/// without putting it on the screen, which is what a caller that does not
/// trust the far side with the look of its window still wants — the picture can
/// be saved, handed to another program or copied, and the page says what
/// arrived.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PictureAccess {
    /// Read, kept and drawn where it stands.
    #[default]
    Drawn,
    /// Read and kept, and not drawn: the line stands in its place.
    Kept,
    /// Not read at all. The line is text and names no picture, so there is
    /// nothing to do with it and nothing was spent on it.
    Refused,
}

/// How many colour registers a program is told this terminal has.
///
/// The decoder has no table of its own — a register is a number a payload
/// names — so this is what a program is answered rather than what is kept
/// anywhere. It is the number every sixel of the last forty years was written
/// against, and a program that reads a smaller one draws worse pictures for
/// nothing.
pub const PALETTE_REGISTERS: usize = 256;

/// What a program asked about the graphics of this terminal.
///
/// It is `XTSMGRAPHICS`, which a program sends before a picture to find out
/// what it may send. The backend drops it, so it is read beside it like
/// everything else here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphicsQuery {
    /// What is being asked about: colour registers, or the size of a picture.
    pub item: u16,
    /// What is being asked of it: read it, reset it, set it, read the largest.
    pub action: u16,
}

/// What this terminal answers such a question, or nothing where the question
/// is about something it has nothing to say about.
///
/// A program may only read. Setting the size of a picture is refused — the
/// limit is this program's setting and not the far side's to raise — and a
/// refusal is answered rather than ignored, because a program waiting for an
/// answer waits for ever.
pub fn graphics_answer(query: GraphicsQuery, limit: PictureLimit) -> Option<String> {
    const READ: u16 = 1;
    const RESET: u16 = 2;
    const SET: u16 = 3;
    const LARGEST: u16 = 4;

    let (registers, geometry) = (1, 2);
    if query.item != registers && query.item != geometry {
        return Some(format!("\u{1b}[?{};1S", query.item));
    }
    let failed = format!("\u{1b}[?{};3S", query.item);

    match query.action {
        READ | RESET | LARGEST => {}
        SET => return Some(failed),
        _ => return Some(format!("\u{1b}[?{};2S", query.item)),
    }

    Some(match query.item {
        item if item == registers => format!("\u{1b}[?{item};0;{PALETTE_REGISTERS}S"),
        item => format!("\u{1b}[?{item};0;{};{}S", limit.width, limit.height),
    })
}

/// One payload as it arrives, byte by byte.
///
/// The head of the payload is read with the tokenizer of the decoder, so the
/// size that is reported is the size the decoder would have seen. The head is
/// where the raster attribute stands, so that reading stops at the first byte
/// of data: everything after it is pixels, and nothing here looks at pixels.
pub(crate) struct SixelCapture {
    reading: Option<Reading>,
    weight: usize,
}

impl Default for SixelCapture {
    fn default() -> Self {
        Self {
            reading: None,
            weight: DEFAULT_PICTURE_WEIGHT,
        }
    }
}

/// One payload being read.
struct Reading {
    bytes: usize,
    declared: Option<(usize, usize)>,
    transparent: bool,
    /// The payload so far, up to the weight of a picture.
    payload: Vec<u8>,
    /// The tokenizer, while the head is still being read.
    head: Option<Parser>,
}

impl SixelCapture {
    /// Changes the most one picture may weigh, which is also how long a
    /// payload may grow.
    pub fn set_weight(&mut self, weight: usize) {
        self.weight = weight;
    }

    /// Begins a payload, when this device control string is one.
    ///
    /// The introducer is handed to the tokenizer again, parameters and all,
    /// because the tokenizer reads a whole sequence and the parser of the
    /// emulation has already taken that part of it.
    pub fn hook(&mut self, params: &[u16], action: char) {
        self.reading = None;
        if action != SIXEL {
            return;
        }

        let mut head = Parser::new();
        let mut introducer = String::from("\u{1b}P");
        for (index, param) in params.iter().enumerate() {
            if index > 0 {
                introducer.push(';');
            }
            introducer.push_str(&param.to_string());
        }
        introducer.push(SIXEL);
        for byte in introducer.as_bytes() {
            head.advance(byte, |_| {});
        }

        self.reading = Some(Reading {
            bytes: 0,
            declared: None,
            transparent: params.get(1) == Some(&1),
            payload: Vec::new(),
            head: Some(head),
        });
    }

    /// One byte of the payload.
    pub fn put(&mut self, byte: u8) {
        let Some(reading) = self.reading.as_mut() else {
            return;
        };
        reading.bytes += 1;
        if reading.payload.len() < self.weight {
            reading.payload.push(byte);
        }

        let Some(head) = reading.head.as_mut() else {
            return;
        };
        let mut declared = None;
        let mut done = false;
        head.advance(&byte, |event| match event {
            SixelEvent::RasterAttribute {
                ph: Some(width),
                pv: Some(height),
                ..
            } => {
                declared = Some((width, height));
                done = true;
            }
            SixelEvent::RasterAttribute { .. }
            | SixelEvent::Data { .. }
            | SixelEvent::Repeat { .. }
            | SixelEvent::GotoNextLine
            | SixelEvent::GotoBeginningOfLine
            | SixelEvent::End => done = true,
            SixelEvent::Dcs { .. }
            | SixelEvent::ColorIntroducer { .. }
            | SixelEvent::UnknownSequence(_) => {}
        });

        reading.declared = reading.declared.or(declared);
        if done {
            reading.head = None;
        }
    }

    /// The payload is whole; what arrived, or nothing when this device control
    /// string carried no picture.
    ///
    /// A payload that ran past the weight of a picture answers with the bytes
    /// it came to and no payload at all: half a picture is not a picture, and
    /// nothing says which half it is.
    pub fn unhook(&mut self) -> Option<(SixelReport, Option<Vec<u8>>)> {
        let reading = self.reading.take()?;
        let report = SixelReport {
            bytes: reading.bytes,
            declared: reading.declared,
            transparent: reading.transparent,
            refused: None,
        };
        let payload = (reading.bytes == reading.payload.len()).then_some(reading.payload);
        Some((report, payload))
    }
}

/// The pixels of a picture, as a renderer and a clipboard want them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pixels {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// Four bytes per pixel, row by row: red, green, blue and alpha.
    ///
    /// A pixel the payload never painted is transparent, because that is what
    /// the standard says of one: a sixel picture is drawn over what is already
    /// there.
    pub rgba: Vec<u8>,
}

/// Reads the pixels of a payload.
///
/// The payload is handed to the tokenizer with its introducer rebuilt, because
/// the tokenizer reads a whole sequence and what is kept here is only what
/// stood inside one. The parameter that is rebuilt with it is the one the
/// pixels differ by: a payload that did not ask for a transparent background
/// has the whole of its rectangle painted.
///
/// The picture is cut to what the raster attribute declared, in both
/// directions. The decoder works in whole bands of six pixels, so a payload
/// that declared a height between two of them decodes to the band above it, and
/// a row of cells would be taken by pixels nobody sent; the width is cut for
/// the same reason — a program that laid its page out against the size it asked
/// for left room for that size, and a picture one column wider than it stands
/// over what was written beside it. What a payload declares is what a program
/// is laying out against, so it is what the picture is. Nothing is cut where
/// the payload declared no size: the extent of the data is then all there is to
/// go by.
///
/// Rows of a picture that declared no size, counted without the empty ones at
/// the bottom.
///
/// A payload is written in bands of six rows, so one that stopped in the middle
/// of a band carries up to five rows nobody painted. Where the raster attribute
/// said how tall the picture is, that is what it is; where it said nothing, a
/// band of nothing would be a row of cells taken from the page — and a program
/// that writes under the picture would be writing under a row of air. So the
/// empty rows at the bottom are not part of it, which is what `foot` does with
/// them.
///
/// Only the rows at the bottom, and only the empty ones: a picture with a gap
/// across it is a picture with a gap across it.
fn painted_height(image: &sixel_image::SixelImage, height: usize) -> usize {
    image
        .pixels
        .iter()
        .take(height)
        .rposition(|row| row.iter().any(|pixel| pixel.on))
        .map_or(0, |last| last + 1)
}

/// A picture past the limit is shrunk to fit it, its proportions kept.
/// Refusing it would leave the line alone where a picture was sent, with
/// nothing to draw and no menu to save it from, and cutting it would keep a
/// corner of it; the limit is a share of the window by default, so an ordinary
/// photograph is past it. The reading itself is held to [`MAX_CANVAS`], or to
/// the limit where that is larger, because what a payload declares is not what
/// it costs; what is past that is cut before it is shrunk.
pub fn decode(report: &SixelReport, payload: &[u8], limit: PictureLimit) -> crate::Result<Pixels> {
    let canvas = PictureLimit {
        width: limit.width.max(MAX_CANVAS.0),
        height: limit.height.max(MAX_CANVAS.1),
    };
    let declared = report
        .declared
        .filter(|(width, height)| *width > 0 && *height > 0);
    let rows = match declared {
        Some((_, height)) => height.min(canvas.height),
        None => canvas.height,
    };
    let mut parser = Parser::new();
    let mut deserializer = SixelDeserializer::new().max_height(rows);
    let mut failed = None;

    let background = match report.transparent {
        true => "1",
        false => "0",
    };
    let introducer = format!("\u{1b}P0;{background};0{SIXEL}");
    let mut whole = Vec::with_capacity(payload.len() + introducer.len() + 2);
    whole.extend_from_slice(introducer.as_bytes());
    whole.extend_from_slice(payload);
    whole.extend_from_slice(b"\x1b\\");

    let mut clip = Clip::new(canvas);
    for byte in &whole {
        parser.advance(byte, |event| {
            let Some(event) = clip.event(event) else {
                return;
            };
            if let Err(error) = deserializer.handle_event(event) {
                failed = failed.or(Some(error));
            }
        });
    }
    if let Some(reason) = failed {
        return Err(crate::TermError::NotAPicture {
            reason: reason.to_string(),
        });
    }

    let image = deserializer
        .create_image()
        .map_err(|reason| crate::TermError::NotAPicture {
            reason: reason.to_string(),
        })?;

    let (decoded_height, decoded_width) = image.pixel_size();
    let height = match declared {
        Some((_, declared)) => decoded_height.min(declared),
        None => decoded_height,
    }
    .min(canvas.height);
    let width = match declared {
        Some((declared, _)) => decoded_width.min(declared),
        None => decoded_width,
    }
    .min(canvas.width);
    let height = match declared {
        Some(_) => height,
        None => painted_height(&image, height),
    };
    if width == 0 || height == 0 {
        return Err(crate::TermError::NotAPicture {
            reason: "the payload drew no pixel".to_string(),
        });
    }

    let mut rgba = Vec::with_capacity(width * height * 4);
    for row in image.pixels.iter().take(height) {
        for column in 0..width {
            match row
                .get(column)
                .filter(|pixel| pixel.on)
                .and_then(|pixel| image.color_registers.get(&pixel.color))
            {
                Some(color) => {
                    let [red, green, blue] = channels(color);
                    rgba.extend_from_slice(&[red, green, blue, u8::MAX]);
                }
                None => rgba.extend_from_slice(&[0, 0, 0, 0]),
            }
        }
    }

    crate::shrink::to_fit(
        Pixels {
            width,
            height,
            rgba,
        },
        limit,
    )
}

/// Holds the events of a payload to the canvas before the deserializer sees
/// them.
///
/// The deserializer grows its canvas to whatever an event asks for: a raster
/// attribute is a canvas filled at once, and a repeat is that many pixels of
/// every line of the band. So a payload of a few bytes could ask for gigabytes,
/// and cutting the picture after it was read would be too late. Here a raster
/// attribute is cut to the canvas and a column past the right edge is dropped
/// before anything is allocated for it; the height is held by the deserializer
/// itself.
struct Clip {
    width: usize,
    height: usize,
    column: usize,
}

impl Clip {
    fn new(canvas: PictureLimit) -> Self {
        Self {
            width: canvas.width,
            height: canvas.height,
            column: 0,
        }
    }

    /// The event as the deserializer is to see it, or nothing where all of it
    /// falls past the right edge.
    fn event(&mut self, event: SixelEvent) -> Option<SixelEvent> {
        match event {
            SixelEvent::RasterAttribute { pan, pad, ph, pv } => Some(SixelEvent::RasterAttribute {
                pan,
                pad,
                ph: ph.map(|ph| ph.min(self.width)),
                pv: pv.map(|pv| pv.min(self.height)),
            }),
            SixelEvent::Data { .. } => self.advance(1).map(|_| event),
            SixelEvent::Repeat {
                repeat_count,
                byte_to_repeat,
            } => self
                .advance(repeat_count)
                .map(|repeat_count| SixelEvent::Repeat {
                    repeat_count,
                    byte_to_repeat,
                }),
            SixelEvent::GotoBeginningOfLine | SixelEvent::GotoNextLine => {
                self.column = 0;
                Some(event)
            }
            _ => Some(event),
        }
    }

    /// Moves the column on by as many of `count` as there is room for, and
    /// answers how many that was.
    fn advance(&mut self, count: usize) -> Option<usize> {
        let taken = count.min(self.width.saturating_sub(self.column));
        self.column += taken;
        (taken > 0).then_some(taken)
    }
}

/// The three channels of a colour register.
///
/// Sixel says a colour in hundredths of each channel, and may say it in hue,
/// lightness and saturation instead. Both are turned into bytes here, so
/// nothing further on has to know that a palette of a picture is not a palette
/// of a screen.
fn channels(color: &SixelColor) -> [u8; 3] {
    match color {
        SixelColor::Rgb(red, green, blue) => [hundredth(*red), hundredth(*green), hundredth(*blue)],
        SixelColor::Hsl(hue, lightness, saturation) => from_hsl(
            *hue as f32,
            *lightness as f32 / 100.0,
            *saturation as f32 / 100.0,
        ),
    }
}

fn hundredth(value: u8) -> u8 {
    ((value.min(100) as u16 * u8::MAX as u16) / 100) as u8
}

/// Hue in degrees, lightness and saturation as shares of one.
fn from_hsl(hue: f32, lightness: f32, saturation: f32) -> [u8; 3] {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue.rem_euclid(360.0) / 60.0;
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
    let channel = |value: f32| ((value + lift).clamp(0.0, 1.0) * u8::MAX as f32).round() as u8;
    [channel(red), channel(green), channel(blue)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(payload: &str) -> Option<SixelReport> {
        let mut capture = SixelCapture::default();
        capture.hook(&[0, 0, 0], SIXEL);
        for byte in payload.as_bytes() {
            capture.put(*byte);
        }
        let (report, kept) = capture.unhook()?;
        assert_eq!(
            kept.as_deref(),
            Some(payload.as_bytes()),
            "the payload is kept as it arrived"
        );
        Some(report)
    }

    /// The size a payload declares is read off its head, and the bytes are
    /// counted whole.
    #[test]
    fn a_payload_says_its_size_and_its_length() {
        let payload = "\"1;1;64;36#0;2;100;0;0#0~~~~$-";
        let report = read(payload).expect("the payload was read");

        assert_eq!(report.declared, Some((64, 36)));
        assert_eq!(report.bytes, payload.len());
    }

    /// A payload that declares no raster attribute says nothing about its
    /// size: the extent of the data is not known until the data is read, and a
    /// payload nobody draws is not read.
    #[test]
    fn a_payload_with_no_raster_attribute_says_no_size() {
        let report = read("#0;2;100;0;0#0~~~~$-").expect("the payload was read");

        assert_eq!(report.declared, None);
        assert_eq!(report.bytes, 20);
    }

    /// A payload that declared no size is as tall as the rows it painted, and
    /// not as tall as the band it stopped in the middle of.
    #[test]
    fn a_payload_that_declared_no_size_is_as_tall_as_what_it_painted() {
        let payload = "#0;2;100;0;0#0!4@$-";
        let report = read(payload).expect("the payload was read");
        let pixels =
            decode(&report, payload.as_bytes(), PictureLimit::default()).expect("it is a picture");

        assert_eq!(report.declared, None);
        assert_eq!(
            (pixels.width, pixels.height),
            (4, 1),
            "one row of six was painted, so the picture is one row tall"
        );
    }

    /// A payload is read as the pixels it drew, in the colours of its
    /// registers, and cut to the size it declared — in both directions, because
    /// a program laid its page out against that size and left room for it.
    #[test]
    fn a_payload_is_read_as_the_pixels_it_drew() {
        let payload = "\"1;1;2;4#0;2;100;0;0#0!2~$-";
        let report = read(payload).expect("the payload was read");
        let pixels =
            decode(&report, payload.as_bytes(), PictureLimit::default()).expect("it is a picture");

        let wider = "\"1;1;2;4#0;2;100;0;0#0!6~$-";
        let report_of_wider = read(wider).expect("the payload was read");
        let cut = decode(&report_of_wider, wider.as_bytes(), PictureLimit::default())
            .expect("it is a picture");
        assert_eq!(
            (cut.width, cut.height),
            (2, 4),
            "data wider than the raster attribute is cut to it"
        );

        assert_eq!(
            (pixels.width, pixels.height),
            (2, 4),
            "cut to the height the payload declared, not to the band it decoded to"
        );
        assert_eq!(pixels.rgba.len(), 2 * 4 * 4);
        assert_eq!(
            &pixels.rgba[0..4],
            &[255, 0, 0, 255],
            "the first pixel is the red of the register"
        );
    }

    /// A picture that asked for a transparent background paints what it drew
    /// and leaves the rest alone; one that did not has its whole rectangle
    /// painted.
    #[test]
    fn a_transparent_background_is_left_alone() {
        // The first row of the band alone, so five of the six are a background
        // and nothing else.
        let payload = "\"1;1;2;6#0;2;100;0;0#0@@";

        let asked = SixelReport {
            declared: Some((2, 6)),
            transparent: true,
            ..SixelReport::default()
        };
        let pixels =
            decode(&asked, payload.as_bytes(), PictureLimit::default()).expect("it is a picture");
        let last = pixels.rgba.len() - 4;
        assert_eq!(&pixels.rgba[last..], &[0, 0, 0, 0]);

        let silent = SixelReport {
            transparent: false,
            ..asked
        };
        let pixels =
            decode(&silent, payload.as_bytes(), PictureLimit::default()).expect("it is a picture");
        let last = pixels.rgba.len() - 4;
        assert_eq!(&pixels.rgba[last..], &[255, 0, 0, 255]);
    }

    /// A picture past the limit is shrunk to fit it, its proportions kept, and
    /// one within it is taken whole.
    #[test]
    fn a_picture_past_the_limit_is_shrunk_to_it() {
        let payload = "\"1;1;200;12#0;2;100;0;0#0!200~-!200~";
        let report = read(payload).expect("the payload was read");
        let limit = PictureLimit {
            width: 100,
            height: 8,
        };

        let pixels = decode(&report, payload.as_bytes(), limit).expect("it is taken");
        assert_eq!((pixels.width, pixels.height), (100, 6), "held by its width");
        assert_eq!(pixels.rgba.len(), 100 * 6 * 4);
        assert_eq!(&pixels.rgba[..4], &[255, 0, 0, 255]);

        let whole = decode(&report, payload.as_bytes(), PictureLimit::default())
            .expect("and taken whole where it fits");
        assert_eq!((whole.width, whole.height), (200, 12));
    }

    /// A payload that asks for a canvas far past [`MAX_CANVAS`] is read into
    /// no more than that, and then shrunk to the limit.
    #[test]
    fn a_canvas_past_the_most_is_cut_to_it_and_shrunk() {
        let payload = "\"1;1;99999;99999#0;2;100;0;0#0!9999~";
        let report = read(payload).expect("the payload was read");
        let limit = PictureLimit {
            width: 64,
            height: 32,
        };

        let pixels = decode(&report, payload.as_bytes(), limit).expect("it is taken");
        assert_eq!((pixels.width, pixels.height), (32, 32));
    }

    /// A payload that drew no pixel is no picture, and says so.
    ///
    /// It takes an empty one. Every printable character between `?` and `~` is
    /// a byte of sixel data, so a payload of some other text is a picture of
    /// whatever those characters happen to mean.
    #[test]
    fn a_payload_that_drew_nothing_is_no_picture() {
        assert!(decode(&SixelReport::default(), b"", PictureLimit::default()).is_err());
    }

    /// A device control string that is not a picture is not one, whatever it
    /// carries.
    #[test]
    fn another_device_control_string_is_no_picture() {
        let mut capture = SixelCapture::default();
        capture.hook(&[], '|');
        for byte in b"something else entirely" {
            capture.put(*byte);
        }

        assert!(capture.unhook().is_none());
    }
}
