//! A minimal but genuinely valid TrueType font, built in code, for tests.
//!
//! # Why this exists rather than a `.ttf` in a fixtures directory
//!
//! Every test of the text path needs a font, and the two obvious ways to get one are both bad here.
//! Committing a real typeface means picking one, licensing it, and carrying a binary blob nobody can
//! diff. Reading the *system's* fonts means the test suite passes or fails depending on what is
//! installed, which is the exact failure `FontCache` is built to prevent.
//!
//! So the font is generated, which is `games/vault`'s `pix` and `games/atrium`'s `tone` applied once
//! more: the source is readable code, the asset is derived, and it is byte-identical every time.
//!
//! # What it contains
//!
//! One square glyph, mapped from the letter `A`, plus the tables a parser insists on. It is not
//! *pretty* — it is a filled box — and that is deliberate: a test that asserts on glyph positions
//! wants predictable advances, not typography.
//!
//! # The format, briefly
//!
//! A TrueType file is a directory of tables. Each entry is a four-byte tag, a checksum, an offset
//! and a length; every table is padded to a four-byte boundary. All numbers are **big-endian**,
//! which is the single most common way to get this wrong.

/// Units per em. 1000 is a round number and makes advances easy to reason about.
const UNITS_PER_EM: u16 = 1000;

/// How wide the one glyph is, in font units. Half an em.
const ADVANCE: u16 = 500;

/// How many glyphs the font has: `.notdef` plus the box.
const GLYPH_COUNT: u16 = 2;

/// Builds the font.
#[must_use]
pub fn single_glyph_font() -> Vec<u8> {
    // Order does not matter to a parser, but keeping it stable keeps the output byte-identical.
    let tables: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"OS/2", os2()),
        (b"cmap", cmap()),
        (b"glyf", glyf()),
        (b"head", head()),
        (b"hhea", hhea()),
        (b"hmtx", hmtx()),
        (b"loca", loca()),
        (b"maxp", maxp()),
        (b"name", name()),
        (b"post", post()),
    ];

    assemble(&tables)
}

/// Writes the table directory and the tables after it.
fn assemble(tables: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let count = tables.len() as u16;
    // The directory's three derived fields. A parser can work them out itself, and some check them.
    let entry_selector = (15 - count.leading_zeros()) as u16;
    let search_range = (1u16 << entry_selector) * 16;
    let range_shift = count * 16 - search_range;

    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // TrueType outlines
    out.extend_from_slice(&count.to_be_bytes());
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&range_shift.to_be_bytes());

    // Where the first table body goes: after the header and the whole directory.
    let mut offset = 12 + tables.len() as u32 * 16;
    let mut bodies = Vec::new();

    for (tag, data) in tables {
        let padded = pad_to_four(data);
        out.extend_from_slice(*tag);
        out.extend_from_slice(&checksum(&padded).to_be_bytes());
        out.extend_from_slice(&offset.to_be_bytes());
        // The *unpadded* length, which is what the specification asks for even though the table
        // occupies the padded amount.
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());

        offset += padded.len() as u32;
        bodies.extend_from_slice(&padded);
    }

    out.extend_from_slice(&bodies);
    out
}

/// A table's checksum: the sum of its big-endian 32-bit words, wrapping.
fn checksum(padded: &[u8]) -> u32 {
    padded.chunks_exact(4).fold(0u32, |total, word| {
        total.wrapping_add(u32::from_be_bytes([word[0], word[1], word[2], word[3]]))
    })
}

/// Zero-pads to a four-byte boundary, which every table must sit on.
fn pad_to_four(data: &[u8]) -> Vec<u8> {
    let mut padded = data.to_vec();
    padded.resize(data.len().next_multiple_of(4), 0);
    padded
}

/// `head` — the font's global metrics.
fn head() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // version
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // fontRevision
    out.extend_from_slice(&0u32.to_be_bytes()); // checkSumAdjustment; nothing here verifies it
    out.extend_from_slice(&0x5F0F_3CF5u32.to_be_bytes()); // magicNumber, and it is checked
    out.extend_from_slice(&0u16.to_be_bytes()); // flags
    out.extend_from_slice(&UNITS_PER_EM.to_be_bytes());
    out.extend_from_slice(&0u64.to_be_bytes()); // created
    out.extend_from_slice(&0u64.to_be_bytes()); // modified
    out.extend_from_slice(&0i16.to_be_bytes()); // xMin
    out.extend_from_slice(&0i16.to_be_bytes()); // yMin
    out.extend_from_slice(&(ADVANCE as i16).to_be_bytes()); // xMax
    out.extend_from_slice(&700i16.to_be_bytes()); // yMax
    out.extend_from_slice(&0u16.to_be_bytes()); // macStyle
    out.extend_from_slice(&8u16.to_be_bytes()); // lowestRecPPEM
    out.extend_from_slice(&2i16.to_be_bytes()); // fontDirectionHint
    // **Short `loca` offsets**, which is what makes the `loca` table below two bytes per entry.
    // Getting this out of step with `loca` is the classic way to build a font that parses and then
    // reads garbage outlines.
    out.extend_from_slice(&0i16.to_be_bytes()); // indexToLocFormat: 0 = short
    out.extend_from_slice(&0i16.to_be_bytes()); // glyphDataFormat
    out
}

/// `hhea` — horizontal metrics header.
fn hhea() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // version
    out.extend_from_slice(&800i16.to_be_bytes()); // ascender
    out.extend_from_slice(&(-200i16).to_be_bytes()); // descender
    out.extend_from_slice(&0i16.to_be_bytes()); // lineGap
    out.extend_from_slice(&ADVANCE.to_be_bytes()); // advanceWidthMax
    out.extend_from_slice(&0i16.to_be_bytes()); // minLeftSideBearing
    out.extend_from_slice(&0i16.to_be_bytes()); // minRightSideBearing
    out.extend_from_slice(&(ADVANCE as i16).to_be_bytes()); // xMaxExtent
    out.extend_from_slice(&1i16.to_be_bytes()); // caretSlopeRise
    out.extend_from_slice(&0i16.to_be_bytes()); // caretSlopeRun
    out.extend_from_slice(&0i16.to_be_bytes()); // caretOffset
    for _ in 0..4 {
        out.extend_from_slice(&0i16.to_be_bytes()); // four reserved
    }
    out.extend_from_slice(&0i16.to_be_bytes()); // metricDataFormat
    out.extend_from_slice(&GLYPH_COUNT.to_be_bytes()); // numberOfHMetrics
    out
}

/// `maxp` — version 1.0, which is the one `glyf` outlines require.
fn maxp() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    out.extend_from_slice(&GLYPH_COUNT.to_be_bytes());
    out.extend_from_slice(&4u16.to_be_bytes()); // maxPoints
    out.extend_from_slice(&1u16.to_be_bytes()); // maxContours
    // The remaining eleven fields are all zero for a font with no composites and no hinting.
    for _ in 0..11 {
        out.extend_from_slice(&0u16.to_be_bytes());
    }
    out
}

/// `hmtx` — an advance and a bearing per glyph.
fn hmtx() -> Vec<u8> {
    let mut out = Vec::new();
    for _ in 0..GLYPH_COUNT {
        out.extend_from_slice(&ADVANCE.to_be_bytes());
        out.extend_from_slice(&0i16.to_be_bytes());
    }
    out
}

/// `loca` — where each glyph's outline starts, in short format (halved offsets).
fn loca() -> Vec<u8> {
    let mut out = Vec::new();
    // `.notdef` is empty, so it and the box start at the same place; the box's outline follows.
    out.extend_from_slice(&0u16.to_be_bytes()); // glyph 0 starts at 0
    out.extend_from_slice(&0u16.to_be_bytes()); // glyph 1 starts at 0 too: glyph 0 is empty
    out.extend_from_slice(&((box_outline().len() / 2) as u16).to_be_bytes()); // end of glyph 1
    out
}

/// `glyf` — one filled square.
fn glyf() -> Vec<u8> {
    box_outline()
}

/// A single closed contour: a square from (50, 0) to (450, 700).
fn box_outline() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&1i16.to_be_bytes()); // numberOfContours; positive means simple
    out.extend_from_slice(&50i16.to_be_bytes()); // xMin
    out.extend_from_slice(&0i16.to_be_bytes()); // yMin
    out.extend_from_slice(&450i16.to_be_bytes()); // xMax
    out.extend_from_slice(&700i16.to_be_bytes()); // yMax

    out.extend_from_slice(&3u16.to_be_bytes()); // endPtsOfContours: last point index
    out.extend_from_slice(&0u16.to_be_bytes()); // instructionLength

    // Four on-curve points. Flag 0x01 is "on curve"; no repeats and no short forms, which makes the
    // coordinate arrays below plain 16-bit deltas.
    out.extend_from_slice(&[0x01; 4]);

    // X deltas, then Y deltas — the format stores them in separate arrays, which surprises people.
    for delta in [50i16, 400, 0, -400] {
        out.extend_from_slice(&delta.to_be_bytes());
    }
    for delta in [0i16, 0, 700, 0] {
        out.extend_from_slice(&delta.to_be_bytes());
    }

    // Padded here rather than by the caller, because `loca` stores *halved* offsets and an odd
    // length would not survive the division.
    pad_to_four(&out)
}

/// `cmap` — format 4, mapping `A` to glyph 1.
fn cmap() -> Vec<u8> {
    let subtable = cmap_format_4();

    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_be_bytes()); // version
    out.extend_from_slice(&1u16.to_be_bytes()); // one encoding record
    out.extend_from_slice(&3u16.to_be_bytes()); // platformID 3, Windows
    out.extend_from_slice(&1u16.to_be_bytes()); // encodingID 1, Unicode BMP
    out.extend_from_slice(&12u32.to_be_bytes()); // offset to the subtable
    out.extend_from_slice(&subtable);
    out
}

/// The format 4 subtable itself: one real segment plus the required terminator.
fn cmap_format_4() -> Vec<u8> {
    // Segment one covers `A` alone; segment two is the mandatory 0xFFFF sentinel.
    let seg_count = 2u16;
    let a = u16::from(b'A');

    let mut out = Vec::new();
    out.extend_from_slice(&4u16.to_be_bytes()); // format
    out.extend_from_slice(&32u16.to_be_bytes()); // length, filled in as a constant below
    out.extend_from_slice(&0u16.to_be_bytes()); // language
    out.extend_from_slice(&(seg_count * 2).to_be_bytes()); // segCountX2
    out.extend_from_slice(&4u16.to_be_bytes()); // searchRange
    out.extend_from_slice(&1u16.to_be_bytes()); // entrySelector
    out.extend_from_slice(&0u16.to_be_bytes()); // rangeShift

    out.extend_from_slice(&a.to_be_bytes()); // endCode[0]
    out.extend_from_slice(&0xFFFFu16.to_be_bytes()); // endCode[1]
    out.extend_from_slice(&0u16.to_be_bytes()); // reservedPad
    out.extend_from_slice(&a.to_be_bytes()); // startCode[0]
    out.extend_from_slice(&0xFFFFu16.to_be_bytes()); // startCode[1]

    // idDelta maps a character to a glyph by addition, modulo 65536. `A` must land on glyph 1.
    out.extend_from_slice(&(1i16.wrapping_sub(a as i16)).to_be_bytes());
    out.extend_from_slice(&1i16.to_be_bytes()); // the sentinel maps 0xFFFF to 0

    out.extend_from_slice(&0u16.to_be_bytes()); // idRangeOffset[0]
    out.extend_from_slice(&0u16.to_be_bytes()); // idRangeOffset[1]

    // The length field written above has to match what was actually produced.
    let length = out.len() as u16;
    out[2..4].copy_from_slice(&length.to_be_bytes());
    out
}

/// `name` — the family name, which is how a shaper is asked for this font.
///
/// **Not optional in practice.** `fontdb` reads the family from here, and a font with no name is a
/// font nothing can ask for by name — which is exactly how `FontCache::shape` refers to it.
fn name() -> Vec<u8> {
    // **Three records, and the third is not optional.** `fontdb` needs a family name *and* a
    // PostScript name (id 6) and returns "unnamed font" without both — which surfaces as a font that
    // simply does not load, with no indication of why. Written with only 1 and 2 first time round.
    //
    // A PostScript name has no spaces, by convention and by several tools' insistence.
    let strings: [(u16, &str); 3] = [
        (1, "Amadeo Test"),        // family
        (2, "Regular"),            // subfamily
        (6, "AmadeoTest-Regular"), // PostScript
    ];

    let record_count = strings.len() as u16;
    let storage_offset = 6 + record_count * 12;

    let mut records = Vec::new();
    let mut storage: Vec<u8> = Vec::new();

    for (name_id, text) in strings {
        // UTF-16BE, because platform 3 encoding 1 says so.
        let encoded: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();

        records.extend_from_slice(&3u16.to_be_bytes()); // platformID, Windows
        records.extend_from_slice(&1u16.to_be_bytes()); // encodingID, Unicode BMP
        records.extend_from_slice(&0x0409u16.to_be_bytes()); // languageID, en-US
        records.extend_from_slice(&name_id.to_be_bytes());
        records.extend_from_slice(&(encoded.len() as u16).to_be_bytes());
        // Offsets are from the start of storage, not from the start of the table.
        records.extend_from_slice(&(storage.len() as u16).to_be_bytes());

        storage.extend_from_slice(&encoded);
    }

    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_be_bytes()); // format 0
    out.extend_from_slice(&record_count.to_be_bytes());
    out.extend_from_slice(&storage_offset.to_be_bytes());
    out.extend_from_slice(&records);
    out.extend_from_slice(&storage);
    out
}

/// `post` — version 3.0, which declares "no glyph names here" and is 32 bytes of mostly nothing.
fn post() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0x0003_0000u32.to_be_bytes()); // version 3.0
    out.extend_from_slice(&0i32.to_be_bytes()); // italicAngle
    out.extend_from_slice(&(-100i16).to_be_bytes()); // underlinePosition
    out.extend_from_slice(&50i16.to_be_bytes()); // underlineThickness
    out.extend_from_slice(&0u32.to_be_bytes()); // isFixedPitch
    for _ in 0..4 {
        out.extend_from_slice(&0u32.to_be_bytes()); // the four memory hints, all unused
    }
    out
}

/// `OS/2` — version 4. `fontdb` reads weight and style from here to describe the face.
fn os2() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&4u16.to_be_bytes()); // version
    out.extend_from_slice(&(ADVANCE as i16).to_be_bytes()); // xAvgCharWidth
    out.extend_from_slice(&400u16.to_be_bytes()); // usWeightClass: regular
    out.extend_from_slice(&5u16.to_be_bytes()); // usWidthClass: medium
    out.extend_from_slice(&0u16.to_be_bytes()); // fsType
    // **Four each, not five.** Written as five first time round, which made the table 100 bytes
    // instead of 96 and shifted every field after it — and the whole font then failed to parse with
    // no indication of where. `the_generated_font_parses` is what found it.
    for _ in 0..4 {
        out.extend_from_slice(&0i16.to_be_bytes()); // ySubscript X/Y size, X/Y offset
    }
    for _ in 0..4 {
        out.extend_from_slice(&0i16.to_be_bytes()); // ySuperscript X/Y size, X/Y offset
    }
    out.extend_from_slice(&50i16.to_be_bytes()); // yStrikeoutSize
    out.extend_from_slice(&250i16.to_be_bytes()); // yStrikeoutPosition
    out.extend_from_slice(&0i16.to_be_bytes()); // sFamilyClass
    out.extend_from_slice(&[0u8; 10]); // panose
    for _ in 0..4 {
        out.extend_from_slice(&0u32.to_be_bytes()); // ulUnicodeRange 1..4
    }
    out.extend_from_slice(b"AMDO"); // achVendID
    out.extend_from_slice(&0u16.to_be_bytes()); // fsSelection
    out.extend_from_slice(&u16::from(b'A').to_be_bytes()); // usFirstCharIndex
    out.extend_from_slice(&u16::from(b'A').to_be_bytes()); // usLastCharIndex
    out.extend_from_slice(&800i16.to_be_bytes()); // sTypoAscender
    out.extend_from_slice(&(-200i16).to_be_bytes()); // sTypoDescender
    out.extend_from_slice(&0i16.to_be_bytes()); // sTypoLineGap
    out.extend_from_slice(&800u16.to_be_bytes()); // usWinAscent
    out.extend_from_slice(&200u16.to_be_bytes()); // usWinDescent
    out.extend_from_slice(&0u32.to_be_bytes()); // ulCodePageRange1
    out.extend_from_slice(&0u32.to_be_bytes()); // ulCodePageRange2
    out.extend_from_slice(&500i16.to_be_bytes()); // sxHeight
    out.extend_from_slice(&700i16.to_be_bytes()); // sCapHeight
    out.extend_from_slice(&0u16.to_be_bytes()); // usDefaultChar
    out.extend_from_slice(&0u16.to_be_bytes()); // usBreakChar
    out.extend_from_slice(&1u16.to_be_bytes()); // usMaxContext

    // Version 4 is exactly this long. Asserted rather than trusted, because a field miscounted
    // anywhere above shifts everything after it and the only symptom is a font that will not parse.
    debug_assert_eq!(out.len(), 96, "OS/2 version 4 is 96 bytes");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generated_font_parses() {
        // **The test that makes every other text test meaningful.** If this font is malformed, the
        // shaping tests all pass vacuously by producing no glyphs — which is exactly what a missing
        // font produces, and indistinguishable from it.
        let mut db = cosmic_text::fontdb::Database::new();
        let ids = db.load_font_source(cosmic_text::fontdb::Source::Binary(std::sync::Arc::new(
            single_glyph_font(),
        )));

        assert_eq!(ids.len(), 1, "one face");
        let face = db.face(ids[0]).expect("the face is in the database");
        assert!(
            face.families
                .iter()
                .any(|(family, _)| family == "Amadeo Test"),
            "the family name has to survive, because that is how a shaper asks for it: {:?}",
            face.families
        );
    }

    #[test]
    fn it_is_byte_identical_every_time() {
        // The same property `pix` and `tone` have. A generator whose output varies is not a source
        // of truth for anything.
        assert_eq!(single_glyph_font(), single_glyph_font());
    }
}
