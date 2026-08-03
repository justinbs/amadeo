//! `pix` — turns the hand-written `.pix` files in `art/` into PNG sprites.
//!
//! ```text
//! cargo run -p vault --bin pix
//! ```
//!
//! # Why this exists
//!
//! Invariant I1 wants a human to be able to author and diff everything, and a PNG is neither
//! authorable nor diffable in a text editor. So the *source* is text — a palette and a grid of
//! characters — and the PNG is derived from it.
//!
//! That is deliberately the same shape as the import pipeline ADR 0026 defers: hand-authorable
//! input, machine-readable output, one command in between. It is a miniature of the real thing, in
//! a game's own directory rather than in the engine, which is where format opinions belong until the
//! engine grows a proper importer.
//!
//! # Why not PPM, which the engine already reads as text
//!
//! **PPM has no alpha.** A sprite drawn over a floor tile needs transparency, and a PPM sprite would
//! be an opaque rectangle. That is the whole reason this converts to PNG rather than leaving the
//! text files as the assets.
//!
//! # Idempotent
//!
//! Running it twice writes byte-identical files, so it can be re-run freely and a diff shows only
//! what actually changed in the art.

use std::path::{Path, PathBuf};

fn main() {
    let art = manifest_dir().join("art");
    let out = manifest_dir().join("assets/textures");

    if let Err(error) = std::fs::create_dir_all(&out) {
        eprintln!("could not create {}: {error}", out.display());
        std::process::exit(1);
    }

    let mut sources: Vec<PathBuf> = match std::fs::read_dir(&art) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "pix"))
            .collect(),
        Err(error) => {
            eprintln!("could not read {}: {error}", art.display());
            std::process::exit(1);
        }
    };
    // Sorted, so the console output is the same every run and a failure is reproducible.
    sources.sort();

    if sources.is_empty() {
        eprintln!("no .pix files in {}", art.display());
        std::process::exit(1);
    }

    let mut failures = 0;
    for source in &sources {
        match convert(source, &out) {
            Ok(report) => println!("  {report}"),
            Err(error) => {
                eprintln!("  {}: {error}", source.display());
                failures += 1;
            }
        }
    }

    if failures > 0 {
        eprintln!("\n{failures} file(s) failed");
        std::process::exit(1);
    }
    println!("\n{} sprite(s) written to {}", sources.len(), out.display());
}

/// This game's directory, from the environment cargo sets at compile time.
///
/// So the tool works from any working directory, which matters because `cargo run` from the
/// workspace root and from the game's own folder are both normal.
fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Reads one `.pix` file and writes the PNG beside it.
fn convert(source: &Path, out: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(source).map_err(|error| error.to_string())?;
    let picture = parse(&text)?;

    let stem = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or("the filename is not usable text")?;
    let destination = out.join(format!("{stem}.png"));

    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, picture.width, picture.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| format!("could not write a PNG header: {error}"))?;
        writer
            .write_image_data(&picture.pixels)
            .map_err(|error| format!("could not write the pixels: {error}"))?;
    }

    // Only written when the bytes actually differ, so re-running does not churn file timestamps and
    // a `git status` after a no-op run is clean.
    let unchanged = std::fs::read(&destination).is_ok_and(|existing| existing == encoded);
    if !unchanged {
        std::fs::write(&destination, &encoded).map_err(|error| error.to_string())?;
    }

    Ok(format!(
        "{stem:<14} {}x{}{}",
        picture.width,
        picture.height,
        if unchanged { "  (unchanged)" } else { "" }
    ))
}

/// A decoded picture, ready to encode.
#[derive(Debug)]
struct Picture {
    width: u32,
    height: u32,
    /// RGBA, row by row from the top.
    pixels: Vec<u8>,
}

/// Reads the `.pix` text format.
///
/// Errors name the line, because these are hand-edited and a message with no line number is a
/// message that costs a search.
fn parse(text: &str) -> Result<Picture, String> {
    let mut palette: Vec<(char, [u8; 4])> = Vec::new();
    let mut rows: Vec<Vec<[u8; 4]>> = Vec::new();

    for (offset, raw) in text.lines().enumerate() {
        let number = offset + 1;
        let line = raw.trim_end();

        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }

        if let Some(entry) = line.trim_start().strip_prefix('!') {
            palette.push(parse_palette_entry(entry, number)?);
            continue;
        }

        let mut row = Vec::with_capacity(line.len());
        for (column, character) in line.chars().enumerate() {
            let colour = palette
                .iter()
                .find(|(key, _)| *key == character)
                .map(|(_, colour)| *colour)
                .ok_or_else(|| {
                    format!(
                        "line {number}, column {}: `{character}` is not in the palette. \
                         Add a line like `! {character} = ff00ffff` above the picture",
                        column + 1
                    )
                })?;
            row.push(colour);
        }
        rows.push(row);
    }

    if rows.is_empty() {
        return Err("there are no pixel rows, only palette and comments".to_string());
    }

    // Every row the same width, checked rather than padded: a ragged picture is a typo, and padding
    // it would silently shift everything after the short line.
    let width = rows[0].len();
    for (index, row) in rows.iter().enumerate() {
        if row.len() != width {
            return Err(format!(
                "row {} is {} pixels wide but row 1 is {width}; every row must match",
                index + 1,
                row.len()
            ));
        }
    }

    let mut pixels = Vec::with_capacity(width * rows.len() * 4);
    for row in &rows {
        for colour in row {
            pixels.extend_from_slice(colour);
        }
    }

    Ok(Picture {
        width: width as u32,
        height: rows.len() as u32,
        pixels,
    })
}

/// Reads `<char> = <rrggbbaa>`, with anything after the colour treated as a note.
fn parse_palette_entry(entry: &str, line: usize) -> Result<(char, [u8; 4]), String> {
    let (key, rest) = entry
        .trim_start()
        .split_once('=')
        .ok_or_else(|| format!("line {line}: a palette entry looks like `! . = 00000000`"))?;

    let key = key.trim();
    let mut characters = key.chars();
    let (Some(character), None) = (characters.next(), characters.next()) else {
        return Err(format!(
            "line {line}: `{key}` should be exactly one character"
        ));
    };

    // Anything past the eight hex digits is a note for the reader, which is how a palette stays
    // self-documenting: `! o = e8964aff  the player's amber`.
    let hex: String = rest
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect();
    if hex.len() != 8 {
        return Err(format!(
            "line {line}: `{}` is not eight hex digits. Alpha is explicit here, so opaque red is \
             `ff0000ff` rather than `ff0000`",
            hex
        ));
    }

    let value = u32::from_str_radix(&hex, 16)
        .map_err(|_| format!("line {line}: `{hex}` is not a hex number"))?;

    Ok((
        character,
        [
            ((value >> 24) & 0xff) as u8,
            ((value >> 16) & 0xff) as u8,
            ((value >> 8) & 0xff) as u8,
            (value & 0xff) as u8,
        ],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_becomes_rgba_rows() {
        let picture = parse("! . = 00000000\n! r = ff0000ff\n.r\nr.\n").expect("valid");

        assert_eq!((picture.width, picture.height), (2, 2));
        assert_eq!(&picture.pixels[0..4], &[0, 0, 0, 0]);
        assert_eq!(&picture.pixels[4..8], &[255, 0, 0, 255]);
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let picture = parse("# a note\n\n! x = 112233ff\n\nx\n").expect("valid");
        assert_eq!((picture.width, picture.height), (1, 1));
    }

    #[test]
    fn a_palette_entry_may_carry_a_note() {
        // What keeps a palette self-documenting.
        let picture = parse("! o = e8964aff  the player's amber\no\n").expect("valid");
        assert_eq!(&picture.pixels[0..4], &[232, 150, 74, 255]);
    }

    #[test]
    fn an_unknown_character_says_how_to_fix_it() {
        let error = parse("! . = 00000000\n.?\n").expect_err("? is not in the palette");
        assert!(error.contains("line 2, column 2"), "{error}");
        assert!(error.contains("Add a line like"), "{error}");
    }

    #[test]
    fn a_ragged_picture_is_refused_rather_than_padded() {
        // Padding would silently shift every row after the short one.
        let error = parse("! x = 000000ff\nxx\nx\n").expect_err("ragged");
        assert!(error.contains("row 2 is 1 pixels wide"), "{error}");
    }

    #[test]
    fn six_hex_digits_are_refused_because_alpha_is_explicit() {
        let error = parse("! x = ff0000\nx\n").expect_err("no alpha");
        assert!(error.contains("eight hex digits"), "{error}");
        assert!(error.contains("ff0000ff"), "{error}");
    }

    #[test]
    fn a_picture_with_no_rows_says_so() {
        let error = parse("# only a comment\n! . = 00000000\n").expect_err("no rows");
        assert!(error.contains("no pixel rows"), "{error}");
    }
}
