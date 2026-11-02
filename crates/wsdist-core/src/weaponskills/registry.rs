// Canonical weaponskill_data.py:304–518, retaining insertion order and effective constants.
use super::{CritScaling, DStatFormula, TpScaling, WeaponSkillId, WeaponSkillSpec, WsElement, WsHook, python_sum};
use crate::model::{Player, SkillType};
use crate::stats::Stat;

// Macro expansion produces one direct numerical branch per named WSC formula.
// `terms()` is registry metadata, not an expression interpreter used by evaluation.
macro_rules! wsc_formulas {
    ($($variant:ident => [$(($coefficient:literal, $stat:ident)),+]),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum WscFormula { $($variant),+ }
        impl WscFormula {
            pub const fn terms(self) -> &'static [(f64, Stat)] {
                match self { $(Self::$variant => &[$(($coefficient, Stat::$stat)),+]),+ }
            }
            pub(super) fn evaluate(self, player: &Player) -> f64 {
                match self { $(Self::$variant => python_sum([$($coefficient * player.stats.get(Stat::$stat)),+]),)+ }
            }
        }
    };
}

wsc_formulas! {
    Str40Dex40 => [(0.4, Str), (0.4, Dex)],
    Str40Int40 => [(0.4, Str), (0.4, Int)],
    Str40Mnd40 => [(0.4, Str), (0.4, Mnd)],
    Str100 => [(1.0, Str)],
    Str50Mnd50 => [(0.5, Str), (0.5, Mnd)],
    Str30Mnd50 => [(0.3, Str), (0.5, Mnd)],
    Mnd85 => [(0.85, Mnd)],
    Dex80 => [(0.8, Dex)],
    Str30Dex20Int30 => [(0.3, Str), (0.2, Dex), (0.3, Int)],
    Dex70Mnd70 => [(0.7, Dex), (0.7, Mnd)],
    Str20Dex60 => [(0.2, Str), (0.6, Dex)],
    Str30Int30 => [(0.3, Str), (0.3, Int)],
    Str30Dex30 => [(0.3, Str), (0.3, Dex)],
    Dex40Int40 => [(0.4, Dex), (0.4, Int)],
    Str60Int60 => [(0.6, Str), (0.6, Int)],
    Dex85 => [(0.85, Dex)],
    Agi80 => [(0.8, Agi)],
    Dex25Agi25 => [(0.25, Dex), (0.25, Agi)],
    Dex100 => [(1.0, Dex)],
    Dex40Chr40 => [(0.4, Dex), (0.4, Chr)],
    Dex40Agi40 => [(0.4, Dex), (0.4, Agi)],
    Dex50 => [(0.5, Dex)],
    Agi85 => [(0.85, Agi)],
    Str80 => [(0.8, Str)],
    Dex60 => [(0.6, Dex)],
    Dex30Chr70 => [(0.3, Dex), (0.7, Chr)],
    Str20Dex20 => [(0.2, Str), (0.2, Dex)],
    Str85 => [(0.85, Str)],
    Str60Vit60 => [(0.6, Str), (0.6, Vit)],
    Str50 => [(0.5, Str)],
    Str55Vit55 => [(0.55, Str), (0.55, Vit)],
    Str60 => [(0.6, Str)],
    Str75 => [(0.75, Str)],
    Str50Mnd30 => [(0.5, Str), (0.3, Mnd)],
    Str30 => [(0.3, Str)],
    Str40Chr60 => [(0.4, Str), (0.6, Chr)],
    Str50Dex50 => [(0.5, Str), (0.5, Dex)],
    Str60Mnd60 => [(0.6, Str), (0.6, Mnd)],
    Str50Int50 => [(0.5, Str), (0.5, Int)],
    Str30Int70 => [(0.3, Str), (0.7, Int)],
    Int85 => [(0.85, Int)],
    Str20Int20 => [(0.2, Str), (0.2, Int)],
    Str40Agi40 => [(0.4, Str), (0.4, Agi)],
    Str30Mnd30 => [(0.3, Str), (0.3, Mnd)],
    Vit80 => [(0.8, Vit)],
    Str40Vit40 => [(0.4, Str), (0.4, Vit)],
    Str30Mnd70 => [(0.3, Str), (0.7, Mnd)],
    Int50Mnd50 => [(0.5, Int), (0.5, Mnd)],
    Dex0 => [(0.0, Dex)],
    Str50Vit50 => [(0.5, Str), (0.5, Vit)],
    Vit85 => [(0.85, Vit)],
    Dex30Chr60 => [(0.3, Dex), (0.6, Chr)],
    Str32Dex32 => [(0.32, Str), (0.32, Dex)],
    Str20Agi50 => [(0.2, Str), (0.5, Agi)],
    Str65Agi65 => [(0.65, Str), (0.65, Agi)],
    Agi70 => [(0.7, Agi)],
    Agi60 => [(0.6, Agi)],
    Agi100 => [(1.0, Agi)],
    Dex70Agi70 => [(0.7, Dex), (0.7, Agi)],
    Int80 => [(0.8, Int)],
    Mnd80 => [(0.8, Mnd)],
    Int45Mnd45 => [(0.45, Int), (0.45, Mnd)],
    Vit100 => [(1.0, Vit)],
    Str20Vit50 => [(0.2, Str), (0.5, Vit)],
    Str15Vit15 => [(0.15, Str), (0.15, Vit)],
    Str32Vit32 => [(0.32, Str), (0.32, Vit)],
}

pub static WS_TABLE: [WeaponSkillSpec; 184] = [
    // Sword
    WeaponSkillSpec::new("Fast Blade", SkillType::Sword, 306, 2, WscFormula::Str40Dex40, TpScaling::Interpolated([1.0, 1.5, 2.0])),
    WeaponSkillSpec::new("Burning Blade", SkillType::Sword, 307, 1, WscFormula::Str40Int40, TpScaling::Interpolated([1.0, 2.09765625, 3.3984375]))
        .magical(WsElement::Fire, DStatFormula::IntCapped),
    WeaponSkillSpec::new("Red Lotus Blade", SkillType::Sword, 308, 1, WscFormula::Str40Int40, TpScaling::Interpolated([1.0, 2.3828125, 3.75]))
        .magical(WsElement::Fire, DStatFormula::IntCapped),
    WeaponSkillSpec::new("Shining Blade", SkillType::Sword, 309, 1, WscFormula::Str40Mnd40, TpScaling::Interpolated([1.125, 2.22265625, 3.5234375]))
        .magical(WsElement::Light, DStatFormula::Zero),
    WeaponSkillSpec::new("Seraph Blade", SkillType::Sword, 310, 1, WscFormula::Str40Mnd40, TpScaling::Interpolated([1.125, 2.625, 4.125]))
        .magical(WsElement::Light, DStatFormula::Zero),
    WeaponSkillSpec::new("Circle Blade", SkillType::Sword, 311, 1, WscFormula::Str100, TpScaling::Fixed(1.0)),
    WeaponSkillSpec::new("Swift Blade", SkillType::Sword, 312, 3, WscFormula::Str50Mnd50, TpScaling::Fixed(1.5))
        .replicated()
        .hooks(&[WsHook::Accuracy { points: [0.0, 20.0, 40.0], both_hands: true }]),
    WeaponSkillSpec::new("Savage Blade", SkillType::Sword, 313, 2, WscFormula::Str50Mnd50, TpScaling::Interpolated([4.0, 10.25, 13.75])),
    WeaponSkillSpec::new("Sanguine Blade", SkillType::Sword, 314, 1, WscFormula::Str30Mnd50, TpScaling::Fixed(2.75))
        .magical(WsElement::Dark, DStatFormula::IntDouble),
    WeaponSkillSpec::new("Requiescat", SkillType::Sword, 315, 5, WscFormula::Mnd85, TpScaling::Fixed(1.0))
        .replicated()
        .hooks(&[WsHook::InterpolatedAttack { points: [0.8, 0.9, 1.0], both_hands: true }]),
    WeaponSkillSpec::new("Knights of Round", SkillType::Sword, 316, 1, WscFormula::Str40Mnd40, TpScaling::Fixed(5.0)),
    WeaponSkillSpec::new("Chant du Cygne", SkillType::Sword, 317, 3, WscFormula::Dex80, TpScaling::Fixed(1.6328125))
        .replicated()
        .critical(CritScaling::Melee([0.15, 0.25, 0.4])),
    WeaponSkillSpec::new("Death Blossom", SkillType::Sword, 318, 3, WscFormula::Str30Mnd50, TpScaling::Fixed(4.0)),
    WeaponSkillSpec::new("Expiacion", SkillType::Sword, 319, 2, WscFormula::Str30Dex20Int30, TpScaling::Interpolated([3.796875, 9.390625, 12.1875])),
    WeaponSkillSpec::new("Fast Blade II", SkillType::Sword, 320, 2, WscFormula::Dex80, TpScaling::Interpolated([1.8, 3.5, 5.0]))
        .replicated(),
    WeaponSkillSpec::new("Imperator", SkillType::Sword, 321, 1, WscFormula::Dex70Mnd70, TpScaling::Interpolated([3.75, 7.5, 11.75])),
    // Katana
    WeaponSkillSpec::new("Blade: Retsu", SkillType::Katana, 324, 2, WscFormula::Str20Dex60, TpScaling::Fixed(1.0)),
    WeaponSkillSpec::new("Blade: Teki", SkillType::Katana, 325, 1, WscFormula::Str30Int30, TpScaling::Fixed(1.0))
        .hybrid(WsElement::Water, [0.5, 1.375, 2.25]),
    WeaponSkillSpec::new("Blade: To", SkillType::Katana, 326, 1, WscFormula::Str40Int40, TpScaling::Fixed(1.0))
        .hybrid(WsElement::Ice, [0.5, 1.5, 2.5]),
    WeaponSkillSpec::new("Blade: Chi", SkillType::Katana, 327, 2, WscFormula::Str30Int30, TpScaling::Fixed(1.0))
        .hybrid(WsElement::Earth, [0.5, 1.375, 2.25]),
    WeaponSkillSpec::new("Blade: Ei", SkillType::Katana, 328, 1, WscFormula::Str40Int40, TpScaling::Interpolated([1.0, 3.0, 5.0]))
        .magical(WsElement::Dark, DStatFormula::IntCapped),
    WeaponSkillSpec::new("Blade: Jin", SkillType::Katana, 329, 3, WscFormula::Str30Dex30, TpScaling::Fixed(1.375))
        .replicated()
        .critical(CritScaling::Melee([0.1, 0.25, 0.5])),
    WeaponSkillSpec::new("Blade: Ten", SkillType::Katana, 330, 1, WscFormula::Str30Dex30, TpScaling::Interpolated([4.5, 11.5, 15.5])),
    WeaponSkillSpec::new("Blade: Ku", SkillType::Katana, 331, 5, WscFormula::Str30Dex30, TpScaling::Fixed(1.25))
        .replicated()
        .hooks(&[WsHook::Accuracy { points: [0.0, 20.0, 40.0], both_hands: true }]),
    WeaponSkillSpec::new("Blade: Yu", SkillType::Katana, 332, 1, WscFormula::Dex40Int40, TpScaling::Fixed(3.0))
        .magical(WsElement::Water, DStatFormula::Zero),
    WeaponSkillSpec::new("Blade: Kamu", SkillType::Katana, 333, 1, WscFormula::Str60Int60, TpScaling::Fixed(1.0))
        .hooks(&[WsHook::Attack { modifier: 1.25, both_hands: true }, WsHook::DefenseDown { points: [0.25, 0.25, 0.25] }]),
    WeaponSkillSpec::new("Blade: Shun", SkillType::Katana, 334, 5, WscFormula::Dex85, TpScaling::Fixed(1.0))
        .replicated()
        .hooks(&[WsHook::InterpolatedAttack { points: [1.0, 2.0, 3.0], both_hands: true }]),
    WeaponSkillSpec::new("Blade: Metsu", SkillType::Katana, 335, 1, WscFormula::Dex80, TpScaling::Fixed(5.0)),
    WeaponSkillSpec::new("Blade: Hi", SkillType::Katana, 336, 1, WscFormula::Agi80, TpScaling::Fixed(5.0))
        .critical(CritScaling::Melee([0.15, 0.2, 0.25])),
    WeaponSkillSpec::new("Zesho Meppo", SkillType::Katana, 337, 4, WscFormula::Dex25Agi25, TpScaling::Interpolated([4.0, 10.0, 18.715])),
    // Dagger
    WeaponSkillSpec::new("Viper Bite", SkillType::Dagger, 340, 2, WscFormula::Dex100, TpScaling::Fixed(1.0))
        .hooks(&[WsHook::Attack { modifier: 1.0, both_hands: true }]),
    WeaponSkillSpec::new("Dancing Edge", SkillType::Dagger, 341, 5, WscFormula::Dex40Chr40, TpScaling::Fixed(1.1875))
        .hooks(&[WsHook::Accuracy { points: [0.0, 20.0, 40.0], both_hands: true }]),
    WeaponSkillSpec::new("Shark Bite", SkillType::Dagger, 342, 2, WscFormula::Dex40Agi40, TpScaling::Interpolated([4.5, 6.8, 8.5])),
    WeaponSkillSpec::new("Evisceration", SkillType::Dagger, 343, 5, WscFormula::Dex50, TpScaling::Fixed(1.25))
        .replicated()
        .critical(CritScaling::Melee([0.1, 0.25, 0.5])),
    WeaponSkillSpec::new("Aeolian Edge", SkillType::Dagger, 344, 1, WscFormula::Dex40Int40, TpScaling::Interpolated([2.0, 3.0, 4.5]))
        .magical(WsElement::Wind, DStatFormula::IntCapped),
    WeaponSkillSpec::new("Exenterator", SkillType::Dagger, 345, 4, WscFormula::Agi85, TpScaling::Fixed(1.0))
        .replicated(),
    WeaponSkillSpec::new("Mercy Stroke", SkillType::Dagger, 346, 1, WscFormula::Str80, TpScaling::Fixed(5.0)),
    WeaponSkillSpec::new("Rudra's Storm", SkillType::Dagger, 347, 1, WscFormula::Dex80, TpScaling::Interpolated([5.0, 10.19, 13.0])),
    WeaponSkillSpec::new("Mandalic Stab", SkillType::Dagger, 348, 1, WscFormula::Dex60, TpScaling::Interpolated([4.0, 6.09, 8.5]))
        .hooks(&[WsHook::Attack { modifier: 0.75, both_hands: true }]),
    WeaponSkillSpec::new("Mordant Rime", SkillType::Dagger, 349, 2, WscFormula::Dex30Chr70, TpScaling::Fixed(5.0))
        .hooks(&[WsHook::Accuracy { points: [0.0, 20.0, 40.0], both_hands: true }]),
    WeaponSkillSpec::new("Pyrrhic Kleos", SkillType::Dagger, 350, 4, WscFormula::Str40Dex40, TpScaling::Fixed(1.75))
        .replicated(),
    WeaponSkillSpec::new("Ruthless Stroke", SkillType::Dagger, 351, 4, WscFormula::Dex25Agi25, TpScaling::Interpolated([5.375, 14.0, 23.0])),
    // Polearm
    WeaponSkillSpec::new("Double Thrust", SkillType::Polearm, 354, 2, WscFormula::Str30Dex30, TpScaling::Interpolated([1.0, 1.5, 2.0])),
    WeaponSkillSpec::new("Thunder Thrust", SkillType::Polearm, 355, 1, WscFormula::Str40Int40, TpScaling::Interpolated([1.5, 2.0, 2.5]))
        .magical(WsElement::Thunder, DStatFormula::IntCapped),
    WeaponSkillSpec::new("Raiden Thrust", SkillType::Polearm, 356, 1, WscFormula::Str40Int40, TpScaling::Interpolated([1.0, 2.0, 3.0]))
        .magical(WsElement::Thunder, DStatFormula::IntCapped),
    WeaponSkillSpec::new("Penta Thrust", SkillType::Polearm, 357, 5, WscFormula::Str20Dex20, TpScaling::Fixed(1.0))
        .hooks(&[WsHook::Accuracy { points: [0.0, 20.0, 40.0], both_hands: false }, WsHook::Attack { modifier: -0.125, both_hands: false }]),
    WeaponSkillSpec::new("Wheeling Thrust", SkillType::Polearm, 358, 1, WscFormula::Str80, TpScaling::Fixed(1.75))
        .hooks(&[WsHook::DefenseDown { points: [0.5, 0.625, 0.75] }]),
    WeaponSkillSpec::new("Impulse Drive", SkillType::Polearm, 359, 2, WscFormula::Str100, TpScaling::Interpolated([1.0, 3.0, 5.5])),
    WeaponSkillSpec::new("Sonic Thrust", SkillType::Polearm, 360, 1, WscFormula::Str40Dex40, TpScaling::Interpolated([3.0, 3.7, 4.5])),
    WeaponSkillSpec::new("Stardiver", SkillType::Polearm, 361, 4, WscFormula::Str85, TpScaling::Interpolated([0.75, 1.25, 1.75]))
        .replicated(),
    WeaponSkillSpec::new("Geirskogul", SkillType::Polearm, 362, 1, WscFormula::Dex80, TpScaling::Fixed(3.0)),
    WeaponSkillSpec::new("Camlann's Torment", SkillType::Polearm, 363, 1, WscFormula::Str60Vit60, TpScaling::Fixed(3.0))
        .hooks(&[WsHook::DefenseDown { points: [0.125, 0.375, 0.625] }]),
    WeaponSkillSpec::new("Drakesbane", SkillType::Polearm, 364, 4, WscFormula::Str50, TpScaling::Fixed(1.0))
        .critical(CritScaling::Melee([0.1, 0.25, 0.4]))
        .hooks(&[WsHook::Attack { modifier: -0.1875, both_hands: false }]),
    WeaponSkillSpec::new("Diarmuid", SkillType::Polearm, 365, 2, WscFormula::Str55Vit55, TpScaling::Interpolated([2.17, 5.36, 8.55])),
    // Great Katana
    WeaponSkillSpec::new("Tachi: Enpi", SkillType::GreatKatana, 368, 2, WscFormula::Str60, TpScaling::Interpolated([1.0, 1.5, 2.0])),
    WeaponSkillSpec::new("Tachi: Goten", SkillType::GreatKatana, 369, 1, WscFormula::Str60, TpScaling::Fixed(1.0))
        .hybrid(WsElement::Thunder, [0.5, 1.5, 2.5]),
    WeaponSkillSpec::new("Tachi: Kagero", SkillType::GreatKatana, 370, 1, WscFormula::Str75, TpScaling::Fixed(1.0))
        .hybrid(WsElement::Fire, [0.5, 1.5, 2.5]),
    WeaponSkillSpec::new("Tachi: Koki", SkillType::GreatKatana, 371, 1, WscFormula::Str50Mnd30, TpScaling::Fixed(1.0))
        .hybrid(WsElement::Light, [0.5, 1.5, 2.5]),
    WeaponSkillSpec::new("Tachi: Jinpu", SkillType::GreatKatana, 372, 2, WscFormula::Str30, TpScaling::Fixed(1.0))
        .hybrid(WsElement::Wind, [0.5, 1.5, 2.5]),
    WeaponSkillSpec::new("Tachi: Yukikaze", SkillType::GreatKatana, 373, 1, WscFormula::Str75, TpScaling::Interpolated([1.5625, 2.6875, 4.125]))
        .hooks(&[WsHook::Attack { modifier: 0.5, both_hands: false }]),
    WeaponSkillSpec::new("Tachi: Gekko", SkillType::GreatKatana, 374, 1, WscFormula::Str75, TpScaling::Interpolated([1.5625, 2.6875, 4.125]))
        .hooks(&[WsHook::Attack { modifier: 1.0, both_hands: false }]),
    WeaponSkillSpec::new("Tachi: Kasha", SkillType::GreatKatana, 375, 1, WscFormula::Str75, TpScaling::Interpolated([1.5625, 2.6875, 4.125]))
        .hooks(&[WsHook::Attack { modifier: 0.65, both_hands: false }]),
    WeaponSkillSpec::new("Tachi: Ageha", SkillType::GreatKatana, 376, 1, WscFormula::Str40Chr60, TpScaling::Fixed(2.625)),
    WeaponSkillSpec::new("Tachi: Shoha", SkillType::GreatKatana, 377, 2, WscFormula::Str85, TpScaling::Interpolated([1.375, 2.1875, 2.6875]))
        .hooks(&[WsHook::Attack { modifier: 1.375, both_hands: false }]),
    WeaponSkillSpec::new("Tachi: Kaiten", SkillType::GreatKatana, 378, 1, WscFormula::Str80, TpScaling::Fixed(3.0)),
    WeaponSkillSpec::new("Tachi: Fudo", SkillType::GreatKatana, 379, 1, WscFormula::Str80, TpScaling::Interpolated([3.75, 5.75, 8.0])),
    WeaponSkillSpec::new("Tachi: Rana", SkillType::GreatKatana, 380, 3, WscFormula::Str50, TpScaling::Fixed(1.0))
        .hooks(&[WsHook::Accuracy { points: [0.0, 20.0, 40.0], both_hands: false }]),
    WeaponSkillSpec::new("Tachi: Mumei", SkillType::GreatKatana, 381, 1, WscFormula::Str50Dex50, TpScaling::Interpolated([3.66, 7.33, 11.0])),
    // Scythe
    WeaponSkillSpec::new("Slice", SkillType::Scythe, 384, 1, WscFormula::Str100, TpScaling::Interpolated([1.5, 1.75, 2.0])),
    WeaponSkillSpec::new("Dark Harvest", SkillType::Scythe, 385, 1, WscFormula::Str40Int40, TpScaling::Interpolated([1.0, 2.0, 2.5]))
        .magical(WsElement::Dark, DStatFormula::IntCapped),
    WeaponSkillSpec::new("Shadow of Death", SkillType::Scythe, 386, 1, WscFormula::Str40Int40, TpScaling::Interpolated([1.0, 4.17, 8.6]))
        .magical(WsElement::Dark, DStatFormula::IntCapped),
    WeaponSkillSpec::new("Nightmare Scythe", SkillType::Scythe, 387, 1, WscFormula::Str60Mnd60, TpScaling::Fixed(1.0)),
    WeaponSkillSpec::new("Spinning Scythe", SkillType::Scythe, 388, 1, WscFormula::Str100, TpScaling::Fixed(1.0)),
    WeaponSkillSpec::new("Cross Reaper", SkillType::Scythe, 389, 2, WscFormula::Str60Mnd60, TpScaling::Interpolated([2.0, 4.0, 7.0])),
    WeaponSkillSpec::new("Guillotine", SkillType::Scythe, 390, 4, WscFormula::Str30Mnd50, TpScaling::Fixed(0.875)),
    WeaponSkillSpec::new("Spiral Hell", SkillType::Scythe, 391, 1, WscFormula::Str50Int50, TpScaling::Interpolated([1.375, 2.75, 4.75])),
    WeaponSkillSpec::new("Infernal Scythe", SkillType::Scythe, 392, 1, WscFormula::Str30Int70, TpScaling::Fixed(3.5))
        .magical(WsElement::Dark, DStatFormula::Zero),
    WeaponSkillSpec::new("Entropy", SkillType::Scythe, 393, 4, WscFormula::Int85, TpScaling::Interpolated([0.75, 1.25, 2.0]))
        .replicated(),
    WeaponSkillSpec::new("Catastrophe", SkillType::Scythe, 394, 1, WscFormula::Str40Int40, TpScaling::Fixed(2.75)),
    WeaponSkillSpec::new("Quietus", SkillType::Scythe, 395, 1, WscFormula::Str60Mnd60, TpScaling::Fixed(3.0))
        .hooks(&[WsHook::DefenseDown { points: [0.1, 0.3, 0.5] }]),
    WeaponSkillSpec::new("Insurgency", SkillType::Scythe, 396, 4, WscFormula::Str20Int20, TpScaling::Interpolated([0.5, 3.25, 6.0])),
    WeaponSkillSpec::new("Origin", SkillType::Scythe, 397, 1, WscFormula::Str60Int60, TpScaling::Interpolated([3.0, 6.25, 9.5])),
    // Great Sword
    WeaponSkillSpec::new("Hard Slash", SkillType::GreatSword, 400, 2, WscFormula::Str40Agi40, TpScaling::Interpolated([1.5, 1.75, 2.0])),
    WeaponSkillSpec::new("Freezebite", SkillType::GreatSword, 401, 1, WscFormula::Str40Int40, TpScaling::Interpolated([1.5, 3.5, 6.0]))
        .magical(WsElement::Ice, DStatFormula::IntCapped),
    WeaponSkillSpec::new("Shockwave", SkillType::GreatSword, 402, 1, WscFormula::Str30Mnd30, TpScaling::Fixed(1.0)),
    WeaponSkillSpec::new("Sickle Moon", SkillType::GreatSword, 403, 1, WscFormula::Str40Agi40, TpScaling::Interpolated([1.5, 2.0, 2.75])),
    WeaponSkillSpec::new("Spinning Slash", SkillType::GreatSword, 404, 1, WscFormula::Str30Int30, TpScaling::Interpolated([2.5, 3.0, 3.5]))
        .hooks(&[WsHook::Attack { modifier: 0.5, both_hands: false }]),
    WeaponSkillSpec::new("Ground Strike", SkillType::GreatSword, 405, 1, WscFormula::Str50Int50, TpScaling::Interpolated([1.5, 1.75, 3.0]))
        .hooks(&[WsHook::Attack { modifier: 0.75, both_hands: false }]),
    WeaponSkillSpec::new("Herculean Slash", SkillType::GreatSword, 406, 1, WscFormula::Vit80, TpScaling::Fixed(3.5))
        .magical(WsElement::Ice, DStatFormula::Zero),
    WeaponSkillSpec::new("Resolution", SkillType::GreatSword, 407, 5, WscFormula::Str85, TpScaling::Interpolated([0.71875, 1.5, 2.25]))
        .replicated()
        .hooks(&[WsHook::Attack { modifier: -0.15, both_hands: false }]),
    WeaponSkillSpec::new("Scourge", SkillType::GreatSword, 408, 1, WscFormula::Str40Vit40, TpScaling::Fixed(3.0)),
    WeaponSkillSpec::new("Torcleaver", SkillType::GreatSword, 409, 1, WscFormula::Vit80, TpScaling::Interpolated([4.75, 7.5, 9.765625])),
    WeaponSkillSpec::new("Dimidiation", SkillType::GreatSword, 410, 2, WscFormula::Dex80, TpScaling::Interpolated([2.25, 4.5, 6.75]))
        .hooks(&[WsHook::Attack { modifier: 0.25, both_hands: false }]),
    WeaponSkillSpec::new("Fimbulvetr", SkillType::GreatSword, 411, 1, WscFormula::Str60Vit60, TpScaling::Interpolated([3.3, 6.6, 9.9])),
    // Club
    WeaponSkillSpec::new("Shining Strike", SkillType::Club, 414, 1, WscFormula::Str40Mnd40, TpScaling::Interpolated([1.625, 3.0, 4.625]))
        .magical(WsElement::Light, DStatFormula::Zero),
    WeaponSkillSpec::new("Seraph Strike", SkillType::Club, 415, 1, WscFormula::Str40Mnd40, TpScaling::Interpolated([2.125, 3.675, 6.125]))
        .magical(WsElement::Light, DStatFormula::Zero),
    WeaponSkillSpec::new("Skullbreaker", SkillType::Club, 416, 1, WscFormula::Str100, TpScaling::Fixed(1.0)),
    WeaponSkillSpec::new("True Strike", SkillType::Club, 417, 1, WscFormula::Str100, TpScaling::Fixed(1.0))
        .critical(CritScaling::Fixed(1.0))
        .hooks(&[WsHook::Accuracy { points: [-60.0, -30.0, 0.0], both_hands: true }, WsHook::Attack { modifier: 1.0, both_hands: true }]),
    WeaponSkillSpec::new("Judgment", SkillType::Club, 418, 1, WscFormula::Str50Mnd50, TpScaling::Interpolated([3.5, 8.75, 12.0])),
    WeaponSkillSpec::new("Hexa Strike", SkillType::Club, 419, 6, WscFormula::Str30Mnd30, TpScaling::Fixed(1.125))
        .replicated()
        .critical(CritScaling::Melee([0.1, 0.175, 0.25])),
    WeaponSkillSpec::new("Black Halo", SkillType::Club, 420, 2, WscFormula::Str30Mnd70, TpScaling::Interpolated([3.0, 7.25, 9.75])),
    WeaponSkillSpec::new("Realmrazer", SkillType::Club, 421, 7, WscFormula::Mnd85, TpScaling::Fixed(0.9))
        .replicated()
        .hooks(&[WsHook::Accuracy { points: [0.0, 20.0, 40.0], both_hands: true }]),
    WeaponSkillSpec::new("Randgrith", SkillType::Club, 422, 1, WscFormula::Str40Mnd40, TpScaling::Fixed(4.25)),
    WeaponSkillSpec::new("Mystic Boon", SkillType::Club, 423, 1, WscFormula::Str30Mnd70, TpScaling::Interpolated([2.5, 4.0, 7.0])),
    WeaponSkillSpec::new("Exudation", SkillType::Club, 424, 1, WscFormula::Int50Mnd50, TpScaling::Fixed(2.8))
        .hooks(&[WsHook::InterpolatedAttack { points: [1.5, 3.625, 4.75], both_hands: true }]),
    WeaponSkillSpec::new("Dagda", SkillType::Club, 425, 2, WscFormula::Dex0, TpScaling::Interpolated([1.0, 2.0, 3.0])),
    // Great Axe
    WeaponSkillSpec::new("Shield Break", SkillType::GreatAxe, 428, 1, WscFormula::Str60Vit60, TpScaling::Fixed(1.0)),
    WeaponSkillSpec::new("Iron Tempest", SkillType::GreatAxe, 429, 1, WscFormula::Str60, TpScaling::Fixed(1.0))
        .hooks(&[WsHook::InterpolatedAttack { points: [1.0, 1.2, 1.5], both_hands: false }]),
    WeaponSkillSpec::new("Armor Break", SkillType::GreatAxe, 430, 1, WscFormula::Str60Vit60, TpScaling::Fixed(1.0)),
    WeaponSkillSpec::new("Weapon Break", SkillType::GreatAxe, 431, 1, WscFormula::Str60Vit60, TpScaling::Fixed(1.0)),
    WeaponSkillSpec::new("Raging Rush", SkillType::GreatAxe, 432, 3, WscFormula::Str50, TpScaling::Fixed(1.0))
        .critical(CritScaling::Melee([0.15, 0.3, 0.5])),
    WeaponSkillSpec::new("Full Break", SkillType::GreatAxe, 433, 1, WscFormula::Str50Vit50, TpScaling::Fixed(1.0)),
    WeaponSkillSpec::new("Steel Cyclone", SkillType::GreatAxe, 434, 1, WscFormula::Str60Vit60, TpScaling::Interpolated([1.5, 2.5, 4.0]))
        .hooks(&[WsHook::Attack { modifier: 0.5, both_hands: false }]),
    WeaponSkillSpec::new("Fell Cleave", SkillType::GreatAxe, 435, 1, WscFormula::Str60, TpScaling::Fixed(2.75)),
    WeaponSkillSpec::new("Upheaval", SkillType::GreatAxe, 436, 4, WscFormula::Vit85, TpScaling::Interpolated([1.0, 3.5, 6.5])),
    WeaponSkillSpec::new("Metatron Torment", SkillType::GreatAxe, 437, 1, WscFormula::Str80, TpScaling::Fixed(2.75)),
    WeaponSkillSpec::new("Ukko's Fury", SkillType::GreatAxe, 438, 2, WscFormula::Str80, TpScaling::Fixed(2.0))
        .critical(CritScaling::Melee([0.2, 0.35, 0.55])),
    WeaponSkillSpec::new("King's Justice", SkillType::GreatAxe, 439, 3, WscFormula::Str50, TpScaling::Interpolated([1.0, 3.0, 5.0])),
    WeaponSkillSpec::new("Disaster", SkillType::GreatAxe, 440, 1, WscFormula::Str60Vit60, TpScaling::Interpolated([3.05, 6.1, 9.15])),
    // Axe
    WeaponSkillSpec::new("Raging Axe", SkillType::Axe, 443, 2, WscFormula::Str60, TpScaling::Interpolated([1.0, 1.5, 2.0])),
    WeaponSkillSpec::new("Spinning Axe", SkillType::Axe, 444, 2, WscFormula::Str60, TpScaling::Interpolated([2.0, 2.5, 3.0])),
    WeaponSkillSpec::new("Rampage", SkillType::Axe, 445, 5, WscFormula::Str50, TpScaling::Fixed(1.0))
        .replicated()
        .critical(CritScaling::Melee([0.0, 0.2, 0.4])),
    WeaponSkillSpec::new("Calamity", SkillType::Axe, 446, 1, WscFormula::Str50Vit50, TpScaling::Interpolated([2.5, 6.5, 10.375])),
    WeaponSkillSpec::new("Mistral Axe", SkillType::Axe, 447, 1, WscFormula::Str50, TpScaling::Interpolated([4.0, 10.5, 13.625])),
    WeaponSkillSpec::new("Decimation", SkillType::Axe, 448, 3, WscFormula::Str50, TpScaling::Fixed(1.75))
        .replicated()
        .hooks(&[WsHook::Accuracy { points: [0.0, 20.0, 40.0], both_hands: true }]),
    WeaponSkillSpec::new("Bora Axe", SkillType::Axe, 449, 1, WscFormula::Dex100, TpScaling::Fixed(4.5))
        .hooks(&[WsHook::Accuracy { points: [0.0, 20.0, 40.0], both_hands: true }]),
    WeaponSkillSpec::new("Ruinator", SkillType::Axe, 450, 4, WscFormula::Str85, TpScaling::Fixed(1.0))
        .replicated()
        .hooks(&[WsHook::Accuracy { points: [0.0, 20.0, 40.0], both_hands: true }, WsHook::Attack { modifier: 0.1, both_hands: true }]),
    WeaponSkillSpec::new("Onslaught", SkillType::Axe, 451, 1, WscFormula::Dex80, TpScaling::Fixed(2.75)),
    WeaponSkillSpec::new("Cloudsplitter", SkillType::Axe, 452, 1, WscFormula::Str40Mnd40, TpScaling::Interpolated([3.75, 6.69921875, 8.5]))
        .magical(WsElement::Thunder, DStatFormula::Zero),
    WeaponSkillSpec::new("Primal Rend", SkillType::Axe, 453, 1, WscFormula::Dex30Chr60, TpScaling::Interpolated([3.0625, 5.8359375, 7.5625]))
        .magical(WsElement::Light, DStatFormula::PrimalRend),
    WeaponSkillSpec::new("Blitz", SkillType::Axe, 454, 5, WscFormula::Str32Dex32, TpScaling::Interpolated([1.5, 7.0, 12.5])),
    // Archery
    WeaponSkillSpec::new("Flaming Arrow", SkillType::Archery, 457, 1, WscFormula::Str20Agi50, TpScaling::Fixed(1.0))
        .hybrid(WsElement::Fire, [0.5, 1.55, 2.1]),
    WeaponSkillSpec::new("Piercing Arrow", SkillType::Archery, 458, 1, WscFormula::Str20Agi50, TpScaling::Fixed(1.0))
        .replicated()
        .hooks(&[WsHook::DefenseDown { points: [0.0, 0.35, 0.5] }]),
    WeaponSkillSpec::new("Dulling Arrow", SkillType::Archery, 459, 1, WscFormula::Str20Agi50, TpScaling::Fixed(1.0))
        .critical(CritScaling::Ranged([0.1, 0.2, 0.25])),
    WeaponSkillSpec::new("Sidewinder", SkillType::Archery, 460, 1, WscFormula::Str20Agi50, TpScaling::Fixed(5.0))
        .hooks(&[WsHook::RangedAccuracy { points: [-50.0, -20.0, 0.0] }]),
    WeaponSkillSpec::new("Blast Arrow", SkillType::Archery, 461, 1, WscFormula::Str20Agi50, TpScaling::Fixed(2.0))
        .hooks(&[WsHook::RangedAccuracy { points: [0.0, 20.0, 40.0] }]),
    WeaponSkillSpec::new("Empyreal Arrow", SkillType::Archery, 462, 1, WscFormula::Str20Agi50, TpScaling::Interpolated([1.5, 2.5, 5.0]))
        .hooks(&[WsHook::RangedAttack { modifier: 1.0 }]),
    WeaponSkillSpec::new("Refulgent Arrow", SkillType::Archery, 463, 1, WscFormula::Str60, TpScaling::Interpolated([3.0, 4.25, 7.0])),
    WeaponSkillSpec::new("Apex Arrow", SkillType::Archery, 464, 1, WscFormula::Agi85, TpScaling::Fixed(3.0))
        .hooks(&[WsHook::DefenseDown { points: [0.15, 0.3, 0.45] }]),
    WeaponSkillSpec::new("Namas Arrow", SkillType::Archery, 465, 1, WscFormula::Str40Agi40, TpScaling::Fixed(2.75)),
    WeaponSkillSpec::new("Jishnu's Radiance", SkillType::Archery, 466, 3, WscFormula::Dex80, TpScaling::Fixed(1.75))
        .replicated()
        .critical(CritScaling::Ranged([0.15, 0.2, 0.25])),
    WeaponSkillSpec::new("Sarv", SkillType::Archery, 467, 1, WscFormula::Str65Agi65, TpScaling::Interpolated([2.75, 5.5, 8.25])),
    // Marksmanship
    WeaponSkillSpec::new("Hot Shot", SkillType::Marksmanship, 470, 1, WscFormula::Agi70, TpScaling::Fixed(1.0))
        .hybrid(WsElement::Fire, [0.5, 1.55, 2.1]),
    WeaponSkillSpec::new("Split Shot", SkillType::Marksmanship, 471, 1, WscFormula::Agi70, TpScaling::Fixed(1.0))
        .hooks(&[WsHook::DefenseDown { points: [0.0, 0.35, 0.5] }]),
    WeaponSkillSpec::new("Sniper Shot", SkillType::Marksmanship, 472, 1, WscFormula::Agi70, TpScaling::Fixed(1.0))
        .critical(CritScaling::Ranged([0.1, 0.2, 0.25])),
    WeaponSkillSpec::new("Slug Shot", SkillType::Marksmanship, 473, 1, WscFormula::Agi70, TpScaling::Fixed(5.0))
        .hooks(&[WsHook::RangedAccuracy { points: [-50.0, -20.0, 0.0] }]),
    WeaponSkillSpec::new("Blast Shot", SkillType::Marksmanship, 474, 1, WscFormula::Agi70, TpScaling::Fixed(2.0))
        .hooks(&[WsHook::RangedAccuracy { points: [0.0, 20.0, 40.0] }]),
    WeaponSkillSpec::new("Detonator", SkillType::Marksmanship, 475, 1, WscFormula::Agi70, TpScaling::Interpolated([1.5, 2.5, 5.0]))
        .hooks(&[WsHook::RangedAttack { modifier: 1.0 }]),
    WeaponSkillSpec::new("Last Stand", SkillType::Marksmanship, 476, 2, WscFormula::Agi85, TpScaling::Interpolated([2.0, 3.0, 4.0]))
        .replicated(),
    WeaponSkillSpec::new("Coronach", SkillType::Marksmanship, 477, 1, WscFormula::Dex40Agi40, TpScaling::Fixed(3.0)),
    WeaponSkillSpec::new("Wildfire", SkillType::Marksmanship, 478, 1, WscFormula::Agi60, TpScaling::Fixed(5.5))
        .magical(WsElement::Fire, DStatFormula::Wildfire),
    WeaponSkillSpec::new("Trueflight", SkillType::Marksmanship, 479, 1, WscFormula::Agi100, TpScaling::Interpolated([3.890625, 6.4921875, 9.671875]))
        .magical(WsElement::Light, DStatFormula::AgiDouble),
    WeaponSkillSpec::new("Leaden Salute", SkillType::Marksmanship, 480, 1, WscFormula::Agi100, TpScaling::Interpolated([4.0, 6.7, 10.0]))
        .magical(WsElement::Dark, DStatFormula::AgiDouble),
    WeaponSkillSpec::new("Terminus", SkillType::Marksmanship, 481, 1, WscFormula::Dex70Agi70, TpScaling::Interpolated([2.5, 5.0, 7.5])),
    // Staff
    WeaponSkillSpec::new("Heavy Swing", SkillType::Staff, 484, 1, WscFormula::Str100, TpScaling::Interpolated([1.0, 1.25, 2.25])),
    WeaponSkillSpec::new("Rock Crusher", SkillType::Staff, 485, 1, WscFormula::Str40Int40, TpScaling::Interpolated([1.0, 2.0, 2.5]))
        .magical(WsElement::Earth, DStatFormula::IntCapped),
    WeaponSkillSpec::new("Earth Crusher", SkillType::Staff, 486, 1, WscFormula::Str40Int40, TpScaling::Interpolated([1.0, 2.3125, 3.625]))
        .magical(WsElement::Earth, DStatFormula::IntCapped),
    WeaponSkillSpec::new("Starburst", SkillType::Staff, 487, 1, WscFormula::Str40Mnd40, TpScaling::Interpolated([1.0, 2.0, 2.5]))
        .magical(WsElement::Light, DStatFormula::IntCapped),
    WeaponSkillSpec::new("Sunburst", SkillType::Staff, 488, 1, WscFormula::Str40Mnd40, TpScaling::Interpolated([1.0, 2.5, 4.0]))
        .magical(WsElement::Light, DStatFormula::IntCapped),
    WeaponSkillSpec::new("Shell Crusher", SkillType::Staff, 489, 1, WscFormula::Str100, TpScaling::Fixed(1.0)),
    WeaponSkillSpec::new("Full Swing", SkillType::Staff, 490, 1, WscFormula::Str50, TpScaling::Interpolated([1.0, 3.0, 5.0])),
    WeaponSkillSpec::new("Retribution", SkillType::Staff, 491, 1, WscFormula::Str30Mnd50, TpScaling::Interpolated([2.0, 2.5, 3.0]))
        .hooks(&[WsHook::Attack { modifier: 0.5, both_hands: false }]),
    WeaponSkillSpec::new("Cataclysm", SkillType::Staff, 492, 1, WscFormula::Str30Int30, TpScaling::Interpolated([2.75, 4.0, 5.0]))
        .magical(WsElement::Dark, DStatFormula::IntCapped),
    WeaponSkillSpec::new("Shattersoul", SkillType::Staff, 493, 3, WscFormula::Int85, TpScaling::Fixed(1.375)),
    WeaponSkillSpec::new("Vidohunir", SkillType::Staff, 494, 1, WscFormula::Int80, TpScaling::Fixed(1.75))
        .magical(WsElement::Dark, DStatFormula::IntDouble),
    WeaponSkillSpec::new("Omniscience", SkillType::Staff, 495, 1, WscFormula::Mnd80, TpScaling::Fixed(2.0))
        .magical(WsElement::Dark, DStatFormula::MndDouble),
    WeaponSkillSpec::new("Garland of Bliss", SkillType::Staff, 496, 1, WscFormula::Str30Mnd70, TpScaling::Fixed(2.25))
        .magical(WsElement::Light, DStatFormula::MndDouble),
    WeaponSkillSpec::new("Gate of Tartarus", SkillType::Staff, 497, 1, WscFormula::Int80, TpScaling::Fixed(3.0)),
    WeaponSkillSpec::new("Oshala", SkillType::Staff, 498, 1, WscFormula::Int45Mnd45, TpScaling::Interpolated([3.945, 7.894, 11.839])),
    // Hand-to-Hand
    WeaponSkillSpec::new("Combo", SkillType::HandToHand, 503, 2, WscFormula::Str30Dex30, TpScaling::Interpolated([1.0, 2.4, 3.4]))
        .replicated(),
    WeaponSkillSpec::new("One Inch Punch", SkillType::HandToHand, 504, 1, WscFormula::Vit100, TpScaling::Fixed(1.0))
        .replicated()
        .hooks(&[WsHook::DefenseDown { points: [0.0, 0.25, 0.5] }]),
    WeaponSkillSpec::new("Raging Fists", SkillType::HandToHand, 505, 4, WscFormula::Str30Dex30, TpScaling::Interpolated([1.0, 2.1875, 3.75]))
        .replicated(),
    WeaponSkillSpec::new("Spinning Attack", SkillType::HandToHand, 506, 1, WscFormula::Str100, TpScaling::Fixed(1.0))
        .replicated(),
    WeaponSkillSpec::new("Howling Fist", SkillType::HandToHand, 507, 1, WscFormula::Str20Vit50, TpScaling::Interpolated([2.05, 3.55, 5.75]))
        .replicated()
        .hooks(&[WsHook::AttackUnguarded { modifier: 0.5 }]),
    WeaponSkillSpec::new("Dragon Kick", SkillType::HandToHand, 508, 1, WscFormula::Str50Vit50, TpScaling::Interpolated([1.7, 3.0, 5.0]))
        .replicated()
        .hooks(&[WsHook::KickAttack]),
    WeaponSkillSpec::new("Asuran Fists", SkillType::HandToHand, 509, 7, WscFormula::Str15Vit15, TpScaling::Fixed(1.25))
        .replicated()
        .hooks(&[WsHook::Accuracy { points: [0.0, 20.0, 40.0], both_hands: true }]),
    WeaponSkillSpec::new("Tornado Kick", SkillType::HandToHand, 510, 2, WscFormula::Str40Vit40, TpScaling::Interpolated([1.7, 2.8, 4.5]))
        .replicated()
        .hooks(&[WsHook::KickAttack]),
    WeaponSkillSpec::new("Shijin Spiral", SkillType::HandToHand, 511, 4, WscFormula::Dex85, TpScaling::Fixed(1.5))
        .replicated()
        .hooks(&[WsHook::Accuracy { points: [0.0, 20.0, 40.0], both_hands: true }, WsHook::AttackUnguarded { modifier: 0.05 }]),
    WeaponSkillSpec::new("Final Heaven", SkillType::HandToHand, 512, 1, WscFormula::Vit80, TpScaling::Fixed(3.0)),
    WeaponSkillSpec::new("Victory Smite", SkillType::HandToHand, 513, 3, WscFormula::Str80, TpScaling::Fixed(1.5))
        .replicated()
        .critical(CritScaling::Melee([0.1, 0.25, 0.45])),
    WeaponSkillSpec::new("Ascetic's Fury", SkillType::HandToHand, 514, 1, WscFormula::Str50Vit50, TpScaling::Fixed(1.0))
        .replicated()
        .critical(CritScaling::Melee([0.2, 0.3, 0.5]))
        .hooks(&[WsHook::AttackUnguarded { modifier: 1.0 }]),
    WeaponSkillSpec::new("Stringing Pummel", SkillType::HandToHand, 515, 5, WscFormula::Str32Vit32, TpScaling::Fixed(1.0))
        .replicated()
        .critical(CritScaling::Melee([0.15, 0.3, 0.45])),
    WeaponSkillSpec::new("Maru Kala", SkillType::HandToHand, 516, 2, WscFormula::Str40Dex40, TpScaling::Interpolated([3.0, 7.0, 11.5])),
    WeaponSkillSpec::new("Dragon Blow", SkillType::HandToHand, 517, 2, WscFormula::Dex85, TpScaling::Interpolated([3.675, 7.0, 10.4375]))
        .hooks(&[WsHook::AttackUnguarded { modifier: 1.5 }]),
];

pub(super) fn resolve(name: &str) -> Option<WeaponSkillId> {
    match name {
        "Fast Blade" => Some(WeaponSkillId(0)),
        "Burning Blade" => Some(WeaponSkillId(1)),
        "Red Lotus Blade" => Some(WeaponSkillId(2)),
        "Shining Blade" => Some(WeaponSkillId(3)),
        "Seraph Blade" => Some(WeaponSkillId(4)),
        "Circle Blade" => Some(WeaponSkillId(5)),
        "Swift Blade" => Some(WeaponSkillId(6)),
        "Savage Blade" => Some(WeaponSkillId(7)),
        "Sanguine Blade" => Some(WeaponSkillId(8)),
        "Requiescat" => Some(WeaponSkillId(9)),
        "Knights of Round" => Some(WeaponSkillId(10)),
        "Chant du Cygne" => Some(WeaponSkillId(11)),
        "Death Blossom" => Some(WeaponSkillId(12)),
        "Expiacion" => Some(WeaponSkillId(13)),
        "Fast Blade II" => Some(WeaponSkillId(14)),
        "Imperator" => Some(WeaponSkillId(15)),
        "Blade: Retsu" => Some(WeaponSkillId(16)),
        "Blade: Teki" => Some(WeaponSkillId(17)),
        "Blade: To" => Some(WeaponSkillId(18)),
        "Blade: Chi" => Some(WeaponSkillId(19)),
        "Blade: Ei" => Some(WeaponSkillId(20)),
        "Blade: Jin" => Some(WeaponSkillId(21)),
        "Blade: Ten" => Some(WeaponSkillId(22)),
        "Blade: Ku" => Some(WeaponSkillId(23)),
        "Blade: Yu" => Some(WeaponSkillId(24)),
        "Blade: Kamu" => Some(WeaponSkillId(25)),
        "Blade: Shun" => Some(WeaponSkillId(26)),
        "Blade: Metsu" => Some(WeaponSkillId(27)),
        "Blade: Hi" => Some(WeaponSkillId(28)),
        "Zesho Meppo" => Some(WeaponSkillId(29)),
        "Viper Bite" => Some(WeaponSkillId(30)),
        "Dancing Edge" => Some(WeaponSkillId(31)),
        "Shark Bite" => Some(WeaponSkillId(32)),
        "Evisceration" => Some(WeaponSkillId(33)),
        "Aeolian Edge" => Some(WeaponSkillId(34)),
        "Exenterator" => Some(WeaponSkillId(35)),
        "Mercy Stroke" => Some(WeaponSkillId(36)),
        "Rudra's Storm" => Some(WeaponSkillId(37)),
        "Mandalic Stab" => Some(WeaponSkillId(38)),
        "Mordant Rime" => Some(WeaponSkillId(39)),
        "Pyrrhic Kleos" => Some(WeaponSkillId(40)),
        "Ruthless Stroke" => Some(WeaponSkillId(41)),
        "Double Thrust" => Some(WeaponSkillId(42)),
        "Thunder Thrust" => Some(WeaponSkillId(43)),
        "Raiden Thrust" => Some(WeaponSkillId(44)),
        "Penta Thrust" => Some(WeaponSkillId(45)),
        "Wheeling Thrust" => Some(WeaponSkillId(46)),
        "Impulse Drive" => Some(WeaponSkillId(47)),
        "Sonic Thrust" => Some(WeaponSkillId(48)),
        "Stardiver" => Some(WeaponSkillId(49)),
        "Geirskogul" => Some(WeaponSkillId(50)),
        "Camlann's Torment" => Some(WeaponSkillId(51)),
        "Drakesbane" => Some(WeaponSkillId(52)),
        "Diarmuid" => Some(WeaponSkillId(53)),
        "Tachi: Enpi" => Some(WeaponSkillId(54)),
        "Tachi: Goten" => Some(WeaponSkillId(55)),
        "Tachi: Kagero" => Some(WeaponSkillId(56)),
        "Tachi: Koki" => Some(WeaponSkillId(57)),
        "Tachi: Jinpu" => Some(WeaponSkillId(58)),
        "Tachi: Yukikaze" => Some(WeaponSkillId(59)),
        "Tachi: Gekko" => Some(WeaponSkillId(60)),
        "Tachi: Kasha" => Some(WeaponSkillId(61)),
        "Tachi: Ageha" => Some(WeaponSkillId(62)),
        "Tachi: Shoha" => Some(WeaponSkillId(63)),
        "Tachi: Kaiten" => Some(WeaponSkillId(64)),
        "Tachi: Fudo" => Some(WeaponSkillId(65)),
        "Tachi: Rana" => Some(WeaponSkillId(66)),
        "Tachi: Mumei" => Some(WeaponSkillId(67)),
        "Slice" => Some(WeaponSkillId(68)),
        "Dark Harvest" => Some(WeaponSkillId(69)),
        "Shadow of Death" => Some(WeaponSkillId(70)),
        "Nightmare Scythe" => Some(WeaponSkillId(71)),
        "Spinning Scythe" => Some(WeaponSkillId(72)),
        "Cross Reaper" => Some(WeaponSkillId(73)),
        "Guillotine" => Some(WeaponSkillId(74)),
        "Spiral Hell" => Some(WeaponSkillId(75)),
        "Infernal Scythe" => Some(WeaponSkillId(76)),
        "Entropy" => Some(WeaponSkillId(77)),
        "Catastrophe" => Some(WeaponSkillId(78)),
        "Quietus" => Some(WeaponSkillId(79)),
        "Insurgency" => Some(WeaponSkillId(80)),
        "Origin" => Some(WeaponSkillId(81)),
        "Hard Slash" => Some(WeaponSkillId(82)),
        "Freezebite" => Some(WeaponSkillId(83)),
        "Shockwave" => Some(WeaponSkillId(84)),
        "Sickle Moon" => Some(WeaponSkillId(85)),
        "Spinning Slash" => Some(WeaponSkillId(86)),
        "Ground Strike" => Some(WeaponSkillId(87)),
        "Herculean Slash" => Some(WeaponSkillId(88)),
        "Resolution" => Some(WeaponSkillId(89)),
        "Scourge" => Some(WeaponSkillId(90)),
        "Torcleaver" => Some(WeaponSkillId(91)),
        "Dimidiation" => Some(WeaponSkillId(92)),
        "Fimbulvetr" => Some(WeaponSkillId(93)),
        "Shining Strike" => Some(WeaponSkillId(94)),
        "Seraph Strike" => Some(WeaponSkillId(95)),
        "Skullbreaker" => Some(WeaponSkillId(96)),
        "True Strike" => Some(WeaponSkillId(97)),
        "Judgment" => Some(WeaponSkillId(98)),
        "Hexa Strike" => Some(WeaponSkillId(99)),
        "Black Halo" => Some(WeaponSkillId(100)),
        "Realmrazer" => Some(WeaponSkillId(101)),
        "Randgrith" => Some(WeaponSkillId(102)),
        "Mystic Boon" => Some(WeaponSkillId(103)),
        "Exudation" => Some(WeaponSkillId(104)),
        "Dagda" => Some(WeaponSkillId(105)),
        "Shield Break" => Some(WeaponSkillId(106)),
        "Iron Tempest" => Some(WeaponSkillId(107)),
        "Armor Break" => Some(WeaponSkillId(108)),
        "Weapon Break" => Some(WeaponSkillId(109)),
        "Raging Rush" => Some(WeaponSkillId(110)),
        "Full Break" => Some(WeaponSkillId(111)),
        "Steel Cyclone" => Some(WeaponSkillId(112)),
        "Fell Cleave" => Some(WeaponSkillId(113)),
        "Upheaval" => Some(WeaponSkillId(114)),
        "Metatron Torment" => Some(WeaponSkillId(115)),
        "Ukko's Fury" => Some(WeaponSkillId(116)),
        "King's Justice" => Some(WeaponSkillId(117)),
        "Disaster" => Some(WeaponSkillId(118)),
        "Raging Axe" => Some(WeaponSkillId(119)),
        "Spinning Axe" => Some(WeaponSkillId(120)),
        "Rampage" => Some(WeaponSkillId(121)),
        "Calamity" => Some(WeaponSkillId(122)),
        "Mistral Axe" => Some(WeaponSkillId(123)),
        "Decimation" => Some(WeaponSkillId(124)),
        "Bora Axe" => Some(WeaponSkillId(125)),
        "Ruinator" => Some(WeaponSkillId(126)),
        "Onslaught" => Some(WeaponSkillId(127)),
        "Cloudsplitter" => Some(WeaponSkillId(128)),
        "Primal Rend" => Some(WeaponSkillId(129)),
        "Blitz" => Some(WeaponSkillId(130)),
        "Flaming Arrow" => Some(WeaponSkillId(131)),
        "Piercing Arrow" => Some(WeaponSkillId(132)),
        "Dulling Arrow" => Some(WeaponSkillId(133)),
        "Sidewinder" => Some(WeaponSkillId(134)),
        "Blast Arrow" => Some(WeaponSkillId(135)),
        "Empyreal Arrow" => Some(WeaponSkillId(136)),
        "Refulgent Arrow" => Some(WeaponSkillId(137)),
        "Apex Arrow" => Some(WeaponSkillId(138)),
        "Namas Arrow" => Some(WeaponSkillId(139)),
        "Jishnu's Radiance" => Some(WeaponSkillId(140)),
        "Sarv" => Some(WeaponSkillId(141)),
        "Hot Shot" => Some(WeaponSkillId(142)),
        "Split Shot" => Some(WeaponSkillId(143)),
        "Sniper Shot" => Some(WeaponSkillId(144)),
        "Slug Shot" => Some(WeaponSkillId(145)),
        "Blast Shot" => Some(WeaponSkillId(146)),
        "Detonator" => Some(WeaponSkillId(147)),
        "Last Stand" => Some(WeaponSkillId(148)),
        "Coronach" => Some(WeaponSkillId(149)),
        "Wildfire" => Some(WeaponSkillId(150)),
        "Trueflight" => Some(WeaponSkillId(151)),
        "Leaden Salute" => Some(WeaponSkillId(152)),
        "Terminus" => Some(WeaponSkillId(153)),
        "Heavy Swing" => Some(WeaponSkillId(154)),
        "Rock Crusher" => Some(WeaponSkillId(155)),
        "Earth Crusher" => Some(WeaponSkillId(156)),
        "Starburst" => Some(WeaponSkillId(157)),
        "Sunburst" => Some(WeaponSkillId(158)),
        "Shell Crusher" => Some(WeaponSkillId(159)),
        "Full Swing" => Some(WeaponSkillId(160)),
        "Retribution" => Some(WeaponSkillId(161)),
        "Cataclysm" => Some(WeaponSkillId(162)),
        "Shattersoul" => Some(WeaponSkillId(163)),
        "Vidohunir" => Some(WeaponSkillId(164)),
        "Omniscience" => Some(WeaponSkillId(165)),
        "Garland of Bliss" => Some(WeaponSkillId(166)),
        "Gate of Tartarus" => Some(WeaponSkillId(167)),
        "Oshala" => Some(WeaponSkillId(168)),
        "Combo" => Some(WeaponSkillId(169)),
        "One Inch Punch" => Some(WeaponSkillId(170)),
        "Raging Fists" => Some(WeaponSkillId(171)),
        "Spinning Attack" => Some(WeaponSkillId(172)),
        "Howling Fist" => Some(WeaponSkillId(173)),
        "Dragon Kick" => Some(WeaponSkillId(174)),
        "Asuran Fists" => Some(WeaponSkillId(175)),
        "Tornado Kick" => Some(WeaponSkillId(176)),
        "Shijin Spiral" => Some(WeaponSkillId(177)),
        "Final Heaven" => Some(WeaponSkillId(178)),
        "Victory Smite" => Some(WeaponSkillId(179)),
        "Ascetic's Fury" => Some(WeaponSkillId(180)),
        "Stringing Pummel" => Some(WeaponSkillId(181)),
        "Maru Kala" => Some(WeaponSkillId(182)),
        "Dragon Blow" => Some(WeaponSkillId(183)),
        _ => None,
    }
}

// Literal weaponskill_info.py:75 exclusions, resolved to registry IDs once.
pub(super) fn shining_one_excluded(skill: WeaponSkillId) -> bool {
    matches!(skill.0,
        131 // Flaming Arrow
        | 139 // Namas Arrow
        | 138 // Apex Arrow
        | 137 // Refulgent Arrow
        | 136 // Empyreal Arrow
        | 134 // Sidewinder
        | 132 // Piercing Arrow
        | 140 // Jishnu's Radiance
        | 135 // Blast Arrow
        | 142 // Hot Shot
        | 149 // Coronach
        | 148 // Last Stand
        | 147 // Detonator
        | 146 // Blast Shot
        | 145 // Slug Shot
        | 143 // Split Shot
    )
}
