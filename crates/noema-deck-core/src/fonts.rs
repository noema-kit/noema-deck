//! The typefaces installed on this machine, which the Appearance sheet offers.
//! Deck bundles none of its own.

use std::{collections::BTreeMap, sync::OnceLock};

use fontdb::{Database, Language};
use serde::Serialize;
use ttf_parser::Face;

/// One installed family.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Font {
    pub family: String,
    /// Whether its letters all take one width, as a terminal needs.
    pub mono: bool,
    /// Whether it draws the icons shell prompts use, as a Nerd Font does.
    pub icons: bool,
}

/// Every installed family by name, read once. Families the system keeps for
/// itself, whose names start with a dot, are left out.
pub fn installed() -> &'static [Font] {
    static FONTS: OnceLock<Vec<Font>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut database = Database::new();
        database.load_system_fonts();
        let mut fonts = BTreeMap::new();
        for face in database.faces() {
            let named = face
                .families
                .iter()
                .find(|(_, language)| *language == Language::English_UnitedStates);
            let Some((family, _)) = named.or(face.families.first()) else { continue };
            if family.starts_with('.') || fonts.contains_key(family) {
                continue;
            }
            let (mono, icons) = database
                .with_face_data(face.id, traits)
                .flatten()
                .unwrap_or((face.monospaced, false));
            fonts.insert(family.clone(), Font { family: family.clone(), mono, icons });
        }
        fonts.into_values().collect()
    })
}

/// Whether a face's letters share one width, and whether it draws prompt
/// icons. A Nerd Font's icons may be wider than its letters, so the width is
/// read from letters and not from the font's own monospace flag.
fn traits(data: &[u8], index: u32) -> Option<(bool, bool)> {
    let face = Face::parse(data, index).ok()?;
    let width = |letter| face.glyph_index(letter).and_then(|glyph| face.glyph_hor_advance(glyph));
    let widths = ['i', 'M', 'W', '0'].map(width);
    let mono = widths[0].is_some() && widths.iter().all(|other| *other == widths[0]);
    // A powerline branch and the Apple logo, two icons every Nerd Font has.
    let icons = ['\u{e0a0}', '\u{f179}'].into_iter().all(|icon| face.glyph_index(icon).is_some());
    Some((mono, icons))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_are_listed_once_in_order_without_hidden_ones() {
        let fonts = installed();
        assert!(fonts.windows(2).all(|pair| pair[0].family < pair[1].family));
        assert!(fonts.iter().all(|font| !font.family.starts_with('.')));
    }

    #[test]
    fn a_face_that_cannot_be_read_has_no_traits() {
        assert_eq!(traits(b"not a font", 0), None);
    }
}
