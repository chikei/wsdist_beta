use crate::math::{BlmElement, BlmTier, NinjutsuTier};
use crate::model::{CalcError, SpellId, SpellType};

#[derive(Clone, Copy, Debug)]
pub(super) enum SpellKernel {
    EnSpell,
    Ninjutsu(BlmElement, NinjutsuTier),
    Elemental(BlmElement, BlmTier, f64),
    Kaustra,
    Impact,
    QuickDraw,
    Ranged,
    Divine,
    None,
}

#[derive(Clone, Copy, Debug)]
pub struct SpellSpec {
    pub name: &'static str,
    pub(super) kernel: SpellKernel,
    pub(super) first_word_element: Option<BlmElement>,
}

impl SpellSpec {
    const fn new(name: &'static str, kernel: SpellKernel) -> Self {
        let bytes = name.as_bytes();
        let first_word_element = if first_word_is(bytes, b"Earth") { Some(BlmElement::Earth) }
            else if first_word_is(bytes, b"Water") { Some(BlmElement::Water) }
            else if first_word_is(bytes, b"Wind") { Some(BlmElement::Wind) }
            else if first_word_is(bytes, b"Fire") { Some(BlmElement::Fire) }
            else if first_word_is(bytes, b"Ice") { Some(BlmElement::Ice) }
            else if first_word_is(bytes, b"Thunder") { Some(BlmElement::Thunder) }
            else if first_word_is(bytes, b"Light") { Some(BlmElement::Light) }
            else if first_word_is(bytes, b"Dark") { Some(BlmElement::Dark) }
            else { None };
        Self { name, kernel, first_word_element }
    }
}

const fn first_word_is(name: &[u8], word: &[u8]) -> bool {
    if name.len() < word.len() { return false; }
    let mut index = 0;
    while index < word.len() {
        if name[index] != word[index] { return false; }
        index += 1;
    }
    name.len() == word.len() || name[word.len()] == b' '
}

macro_rules! spells {
    ($($name:literal => $kernel:expr),+ $(,)?) => {
        pub static SPELL_REGISTRY: &[SpellSpec] = &[
            $(SpellSpec::new($name, $kernel)),+
        ];
    };
}

use BlmElement::{Dark, Earth, Fire, Ice, Light, Thunder, Water, Wind};
use BlmTier::{Helix, I, II, III, IV, Ja, V, VI};
use NinjutsuTier::{Ichi, Ni, San};
use SpellKernel::{Divine, Elemental, EnSpell, Impact, Kaustra, Ninjutsu, QuickDraw, Ranged};

// actions.py:974–981,1062–1085 and app_state.py:69–114. The dispatch is
// deliberately not part of a specification: the two callers disagree.
spells! {
    "Doton: Ichi" => Ninjutsu(Earth, Ichi),
    "Doton: Ni" => Ninjutsu(Earth, Ni),
    "Doton: San" => Ninjutsu(Earth, San),
    "Suiton: Ichi" => Ninjutsu(Water, Ichi),
    "Suiton: Ni" => Ninjutsu(Water, Ni),
    "Suiton: San" => Ninjutsu(Water, San),
    "Huton: Ichi" => Ninjutsu(Wind, Ichi),
    "Huton: Ni" => Ninjutsu(Wind, Ni),
    "Huton: San" => Ninjutsu(Wind, San),
    "Katon: Ichi" => Ninjutsu(Fire, Ichi),
    "Katon: Ni" => Ninjutsu(Fire, Ni),
    "Katon: San" => Ninjutsu(Fire, San),
    "Hyoton: Ichi" => Ninjutsu(Ice, Ichi),
    "Hyoton: Ni" => Ninjutsu(Ice, Ni),
    "Hyoton: San" => Ninjutsu(Ice, San),
    "Raiton: Ichi" => Ninjutsu(Thunder, Ichi),
    "Raiton: Ni" => Ninjutsu(Thunder, Ni),
    "Raiton: San" => Ninjutsu(Thunder, San),
    "Stone" => Elemental(Earth, I, 4.0),
    "Stone II" => Elemental(Earth, II, 16.0),
    "Stone III" => Elemental(Earth, III, 40.0),
    "Stone IV" => Elemental(Earth, IV, 88.0),
    "Stone V" => Elemental(Earth, V, 156.0),
    "Stone VI" => Elemental(Earth, VI, 237.0),
    "Stoneja" => Elemental(Earth, Ja, 298.0),
    "Water" => Elemental(Water, I, 5.0),
    "Water II" => Elemental(Water, II, 19.0),
    "Water III" => Elemental(Water, III, 46.0),
    "Water IV" => Elemental(Water, IV, 99.0),
    "Water V" => Elemental(Water, V, 175.0),
    "Water VI" => Elemental(Water, VI, 266.0),
    "Waterja" => Elemental(Water, Ja, 318.0),
    "Aero" => Elemental(Wind, I, 6.0),
    "Aero II" => Elemental(Wind, II, 22.0),
    "Aero III" => Elemental(Wind, III, 54.0),
    "Aero IV" => Elemental(Wind, IV, 115.0),
    "Aero V" => Elemental(Wind, V, 198.0),
    "Aero VI" => Elemental(Wind, VI, 299.0),
    "Aeroja" => Elemental(Wind, Ja, 338.0),
    "Fire" => Elemental(Fire, I, 7.0),
    "Fire II" => Elemental(Fire, II, 26.0),
    "Fire III" => Elemental(Fire, III, 63.0),
    "Fire IV" => Elemental(Fire, IV, 135.0),
    "Fire V" => Elemental(Fire, V, 288.0),
    "Fire VI" => Elemental(Fire, VI, 339.0),
    "Firaja" => Elemental(Fire, Ja, 358.0),
    "Blizzard" => Elemental(Ice, I, 8.0),
    "Blizzard II" => Elemental(Ice, II, 31.0),
    "Blizzard III" => Elemental(Ice, III, 75.0),
    "Blizzard IV" => Elemental(Ice, IV, 162.0),
    "Blizzard V" => Elemental(Ice, V, 267.0),
    "Blizzard VI" => Elemental(Ice, VI, 386.0),
    "Blizzaja" => Elemental(Ice, Ja, 378.0),
    "Thunder" => Elemental(Thunder, I, 9.0),
    "Thunder II" => Elemental(Thunder, II, 37.0),
    "Thunder III" => Elemental(Thunder, III, 91.0),
    "Thunder IV" => Elemental(Thunder, IV, 195.0),
    "Thunder V" => Elemental(Thunder, V, 306.0),
    "Thunder VI" => Elemental(Thunder, VI, 437.0),
    "Thundaja" => Elemental(Thunder, Ja, 398.0),
    "Geohelix II" => Elemental(Earth, Helix, 78.0),
    "Hydrohelix II" => Elemental(Water, Helix, 78.0),
    "Anemohelix II" => Elemental(Wind, Helix, 78.0),
    "Pyrohelix II" => Elemental(Fire, Helix, 78.0),
    "Cryohelix II" => Elemental(Ice, Helix, 78.0),
    "Ionohelix II" => Elemental(Thunder, Helix, 78.0),
    "Luminohelix II" => Elemental(Light, Helix, 78.0),
    "Noctohelix II" => Elemental(Dark, Helix, 78.0),
    "Kaustra" => Kaustra,
    "Impact" => Impact,
    "EnSpell" => EnSpell,
    "Earth Shot" => QuickDraw,
    "Water Shot" => QuickDraw,
    "Wind Shot" => QuickDraw,
    "Fire Shot" => QuickDraw,
    "Ice Shot" => QuickDraw,
    "Thunder Shot" => QuickDraw,
    "Ranged Attack" => Ranged,
    "Barrage" => Ranged,
    "Banish" => Divine,
    "Banish II" => Divine,
    "Holy" => Divine,
    "Holy II" => Divine,
    "None" => SpellKernel::None,
}

impl SpellId {
    pub const fn index(self) -> usize { self.0 as usize }
    pub fn name(self) -> &'static str { SPELL_REGISTRY[self.index()].name }
}

pub fn resolve_spell_name(name: &str) -> Result<SpellId, CalcError> {
    SPELL_REGISTRY.iter().position(|spec| spec.name == name)
        .map(|index| SpellId(index as u16))
        .ok_or_else(|| CalcError::Input {
            operation: "cast_spell", field: "spell_name", message: format!("unsupported spell {name:?}"),
        })
}

// tabs/simulate_tab.py:249–258, after the caller's separate None assertion.
// Case folding only compares the ASCII pattern; it never allocates a string.
pub fn gui_spell_type(name: &str) -> Result<SpellType, CalcError> {
    if name.as_bytes().windows(5).any(|part| part.eq_ignore_ascii_case(b"ton: ")) {
        Ok(SpellType::Ninjutsu)
    } else {
        let last = name.split_whitespace().next_back().ok_or_else(|| {
            CalcError::legacy("gui_spell_type", "spell_name", "IndexError", "list index out of range")
        })?;
        if last.eq_ignore_ascii_case("shot") { Ok(SpellType::QuickDraw) }
        else if name.contains("Banish") || name.contains("Holy") { Ok(SpellType::DivineMagic) }
        else if matches!(name, "Ranged Attack" | "Barrage") { Ok(SpellType::RangedAttack) }
        else { Ok(SpellType::ElementalMagic) }
    }
}

// wsdist.py:866–873. Barrage and divine names intentionally fall through.
pub fn optimizer_spell_type(name: &str) -> SpellType {
    if name.contains(" Shot") { SpellType::QuickDraw }
    else if name == "Ranged Attack" { SpellType::RangedAttack }
    else if name.contains(": Ichi") || name.contains(": Ni") || name.contains(": San") {
        SpellType::Ninjutsu
    } else { SpellType::ElementalMagic }
}
