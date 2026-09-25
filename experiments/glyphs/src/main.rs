//! Every glyph the fonts of egui carry, as a list.
//!
//! The toolkit is asked, not the font files: a code point that stands in the
//! charmap of a face the toolkit has bound to a family is a code point the
//! toolkit draws, and that is the question an icon of the interface asks. No
//! window is opened for it — `egui::Context` is run for one frame with no
//! widgets in it, which is enough for the fonts to exist — so the tool runs
//! where there is no display at all.
//!
//! It says first which fonts the toolkit holds and which family each of them
//! answers, because that is the shape of the answer: a glyph belongs to a face,
//! and a face is reached through a family. The glyphs follow.
//!
//! Without an argument it lists what egui ships with, family by family. With a
//! path to a font file it lists that file instead, bound to both families and
//! with nothing behind it, so what comes out is what that one font carries.

mod error;

use clap::Parser;
use egui::{FontData, FontDefinitions, FontFamily};
use error::{GlyphsError, Result};
use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Every glyph the fonts of egui carry, printed and written to a file.
#[derive(Debug, Parser)]
#[command(name = "glyphs", version, about, long_about = None)]
struct Arguments {
    /// Font file to list instead of the fonts egui ships with.
    ///
    /// It is bound to both families with nothing behind it, so the list is
    /// that font and not that font over the fallbacks of the toolkit.
    font: Option<PathBuf>,

    /// File the list is written to.
    #[arg(short, long, default_value = "glyphs.txt", value_name = "FILE")]
    out: PathBuf,
}

/// One family and what it carries: every code point, with the faces that have
/// it, in the order of the code points.
struct Coverage {
    /// Name of the family, as the toolkit spells it.
    family: String,
    /// Faces bound to it, in the order they are asked.
    faces: Vec<String>,
    /// Every glyph of it, and which faces carry it.
    glyphs: BTreeMap<char, Vec<String>>,
}

/// One font the toolkit holds, and the families it answers.
struct Loaded {
    /// Name the toolkit knows it by, which is the key of its font data.
    name: String,
    /// Families it is bound to, in the order of their names.
    families: Vec<String>,
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            report(&error);
            std::process::ExitCode::FAILURE
        }
    }
}

/// Reads the arguments, asks the toolkit and writes what it answered.
fn run() -> Result<()> {
    let arguments = Arguments::parse();
    let asked = arguments.font.as_deref();
    let definitions = match asked {
        Some(path) => Some(one_font(path)?),
        None => None,
    };

    let context = fonts(definitions);
    let preamble = preamble(&loaded(&context), asked);
    print(&preamble);

    let mut coverage = coverage(&context);
    if let Some(path) = asked {
        coverage = the_font_alone(coverage, &face_name(path));
    }
    if coverage.iter().all(|one| one.glyphs.is_empty()) {
        return Err(GlyphsError::NoGlyphs);
    }

    let listing = listing(&coverage);
    write(&arguments.out, &format!("{preamble}{listing}"))?;
    print(&listing);
    Ok(())
}

/// Prints the list, and gives up on it quietly when nobody is reading.
///
/// The list is written before it is printed, so a reader that stopped after the
/// first lines — the usual `| head` — leaves the file behind it all the same.
fn print(text: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.flush();
}

/// Prints a failure and what it was about, one line each.
fn report(error: &GlyphsError) {
    eprintln!("glyphs: {error}");
    let mut source: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(error);
    while let Some(next) = source {
        eprintln!("  {next}");
        source = next.source();
    }
}

/// The one block a named font file gets.
///
/// The file is bound to both families of the toolkit, so both answer the same
/// glyphs; a list that said them twice would say nothing twice. The block is
/// named after the file instead of after a family, because a family is what the
/// toolkit chose here and the file is what was asked about.
fn the_font_alone(coverage: Vec<Coverage>, name: &str) -> Vec<Coverage> {
    coverage
        .into_iter()
        .take(1)
        .map(|one| Coverage {
            family: name.to_string(),
            ..one
        })
        .collect()
}

/// What a font file is called in the list: its file name.
fn face_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

/// The fonts of one file, bound to both families and nothing else.
///
/// Nothing stands behind it: the point of naming a file is to see what that
/// file has, and a fallback chain would answer for the code points it has not.
fn one_font(path: &Path) -> Result<FontDefinitions> {
    let bytes = std::fs::read(path).map_err(|source| GlyphsError::Font {
        path: path.to_path_buf(),
        source,
    })?;
    let name = face_name(path);

    let mut definitions = FontDefinitions::empty();
    definitions
        .font_data
        .insert(name.clone(), Arc::new(FontData::from_owned(bytes)));
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        definitions.families.insert(family, vec![name.clone()]);
    }
    Ok(definitions)
}

/// A toolkit whose fonts exist, and nothing else.
///
/// One frame is run before the fonts are asked for, because the toolkit builds
/// them on the first frame and not before it. Nothing is drawn in that frame
/// and nothing is shown: there is no window, and the texture the frame produced
/// is dropped with it. A font of one's own is set between two frames, because
/// what `set_fonts` says is built on the frame after it.
fn fonts(definitions: Option<FontDefinitions>) -> egui::Context {
    let context = egui::Context::default();
    frame(&context);
    if let Some(definitions) = definitions {
        context.set_fonts(definitions);
        frame(&context);
    }
    context
}

/// Which fonts the toolkit holds, and which families each of them answers.
///
/// A font it holds and no family asks for is in the list as well, with no
/// family beside it: it is loaded, and nothing of it is ever drawn.
fn loaded(context: &egui::Context) -> Vec<Loaded> {
    context.fonts_mut(|fonts| {
        let definitions = fonts.definitions();
        definitions
            .font_data
            .keys()
            .map(|name| {
                let mut families: Vec<String> = definitions
                    .families
                    .iter()
                    .filter(|(_, faces)| faces.contains(name))
                    .map(|(family, _)| family.to_string())
                    .collect();
                families.sort_unstable();
                Loaded {
                    name: name.clone(),
                    families,
                }
            })
            .collect()
    })
}

/// What every family of the toolkit carries, in the order of their names.
fn coverage(context: &egui::Context) -> Vec<Coverage> {
    let mut families = context.fonts_mut(|fonts| fonts.families());
    families.sort_by_key(|family| family.to_string());

    families
        .into_iter()
        .map(|family| {
            let (faces, glyphs) = context.fonts_mut(|fonts| {
                let faces = fonts
                    .definitions()
                    .families
                    .get(&family)
                    .cloned()
                    .unwrap_or_default();
                let glyphs = fonts.fonts.font(&family).characters().clone();
                (faces, glyphs)
            });
            Coverage {
                family: family.to_string(),
                faces,
                glyphs,
            }
        })
        .collect()
}

/// Runs one frame with nothing in it.
fn frame(context: &egui::Context) {
    let mut output = context.run_ui(egui::RawInput::default(), |_| {});
    output.textures_delta.clear();
}

/// What stands at the top: where the fonts came from, and which of them the
/// toolkit holds with the families they answer.
///
/// It is printed before a single glyph is looked at, because it is the answer to
/// the first question — what is loaded at all — and the list of glyphs is only
/// worth reading once that is known.
fn preamble(loaded: &[Loaded], font: Option<&Path>) -> String {
    let mut text = match font {
        Some(path) => format!("# font: {}\n", path.display()),
        None => "# fonts: the ones egui ships with\n".to_string(),
    };
    text.push_str(&format!("# loaded: {}\n", loaded.len()));

    for one in loaded {
        text.push_str(&format!("#   {}\t{}\n", one.name, families(&one.families)));
    }

    text
}

/// The families one font answers, or a word for a font no family asks for.
fn families(families: &[String]) -> String {
    match families.is_empty() {
        true => "no family".to_string(),
        false => families.join(", "),
    }
}

/// The whole list as text: a block per family, a line per glyph.
///
/// A line is the code point, the glyph itself and the faces that carry it,
/// separated by tabs, so the file is read by an eye and cut by a script alike.
/// A glyph that would move the cursor instead of standing under it — a control
/// character, a line break — is left out of the column of its own; the code
/// point still says which one it is.
fn listing(coverage: &[Coverage]) -> String {
    let mut text = String::new();

    for one in coverage {
        text.push_str(&format!(
            "\n# {} \u{2014} {} glyphs, faces: {}\n",
            one.family,
            one.glyphs.len(),
            faces(&one.faces)
        ));
        for (glyph, carried_by) in &one.glyphs {
            text.push_str(&format!(
                "U+{:04X}\t{}\t{}\n",
                u32::from(*glyph),
                shown(*glyph),
                carried_by.join(", ")
            ));
        }
    }

    text
}

/// The faces of a family as one line, or a word when it has none.
fn faces(faces: &[String]) -> String {
    match faces.is_empty() {
        true => "none".to_string(),
        false => faces.join(", "),
    }
}

/// The glyph as it goes into the column, which is nothing for one that would
/// move the cursor rather than stand in it.
fn shown(glyph: char) -> String {
    match glyph.is_control() {
        true => String::new(),
        false => glyph.to_string(),
    }
}

/// Writes the list where it was asked for.
fn write(path: &Path, text: &str) -> Result<()> {
    std::fs::write(path, text).map_err(|source| GlyphsError::Write {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A font file to name on the command line, written from a font the toolkit
    /// carries in itself.
    ///
    /// Nothing of the machine is asked for: a font of the platform may be
    /// anywhere or nowhere, and the question here is what the tool does with a
    /// file it was handed, not which files exist.
    fn font_file() -> tempfile::TempPath {
        let definitions = FontDefinitions::default();
        let (_, data) = definitions
            .font_data
            .iter()
            .next()
            .expect("the toolkit ships with a font");
        let mut file = tempfile::Builder::new()
            .suffix(".ttf")
            .tempfile()
            .expect("a temporary file is made");
        std::io::Write::write_all(&mut file, &data.font).expect("the font is written");
        file.into_temp_path()
    }

    /// The fonts egui ships with carry the letters of the alphabet, so a list
    /// of them that has no `A` in it is a list that asked the wrong question.
    #[test]
    fn the_fonts_of_the_toolkit_carry_their_letters() {
        let coverage = coverage(&fonts(None));

        assert!(!coverage.is_empty(), "the toolkit has families");
        for one in &coverage {
            assert!(
                one.glyphs.contains_key(&'A'),
                "{} carries the capital A",
                one.family
            );
            assert!(!one.faces.is_empty(), "{} is bound to a face", one.family);
        }
    }

    /// A line says the code point, the glyph and the face, and a control
    /// character keeps its column empty instead of reaching the terminal.
    #[test]
    fn a_line_says_the_code_point_the_glyph_and_the_face() {
        let coverage = vec![Coverage {
            family: "Monospace".to_string(),
            faces: vec!["Hack".to_string()],
            glyphs: BTreeMap::from([
                ('\u{9}', vec!["Hack".to_string()]),
                ('A', vec!["Hack".to_string()]),
            ]),
        }];

        let text = listing(&coverage);

        assert!(text.contains("# Monospace \u{2014} 2 glyphs, faces: Hack\n"));
        assert!(text.contains("U+0041\tA\tHack\n"));
        assert!(text.contains("U+0009\t\tHack\n"));
    }

    /// The fonts the toolkit ships with are said before any glyph is, each with
    /// the families it answers: `Ubuntu-Light` is the face of the proportional
    /// family the interface is drawn in, so it is in the list and says so.
    #[test]
    fn the_fonts_of_the_toolkit_are_said_first() {
        let loaded = loaded(&fonts(None));

        let text = preamble(&loaded, None);
        assert!(text.starts_with("# fonts: the ones egui ships with\n"));
        assert!(text.contains(&format!("# loaded: {}\n", loaded.len())));

        let ubuntu = loaded
            .iter()
            .find(|one| one.name.contains("Ubuntu"))
            .expect("the toolkit ships with Ubuntu-Light");
        assert!(
            ubuntu
                .families
                .iter()
                .any(|family| family == "Proportional"),
            "it answers the proportional family"
        );
        assert!(text.contains(&format!("#   {}\t", ubuntu.name)));
    }

    /// A font of one's own is the only one loaded, and it answers both families
    /// of the toolkit, because nothing is left behind it to answer for it.
    #[test]
    fn a_font_of_ones_own_is_the_only_one_loaded() {
        let path = font_file();
        let definitions = one_font(&path).expect("the font file is there");

        let loaded = loaded(&fonts(Some(definitions)));

        assert_eq!(loaded.len(), 1, "one font and nothing else");
        assert_eq!(loaded[0].families, ["Monospace", "Proportional"]);
    }

    /// A font nobody has is a failure that names the path, not a list of
    /// nothing.
    #[test]
    fn a_font_that_is_nowhere_is_a_failure_of_its_own() {
        let error = one_font(Path::new("no-such-font.ttf")).expect_err("there is no such file");

        assert!(matches!(error, GlyphsError::Font { .. }));
    }
}
