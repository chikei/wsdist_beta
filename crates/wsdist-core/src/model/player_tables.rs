// Literal ordered reference tables from create_player.py:25–272.
// Repeated Python keys retain their effective last value and first insertion position.
use super::{Job, Slot, Stat};

pub(super) struct ArmorBonus {
    pub name: &'static str,
    pub plus1: bool,
    pub slots: Option<&'static [Slot]>,
    pub max_pieces: usize,
    pub counts_first: bool,
    pub per_piece: f64,
    pub stats: &'static [Stat],
}

pub(super) struct Trait {
    pub stat: Stat,
    pub jobs: &'static [(Job, &'static [(i32, f64)])],
}

pub(super) const PARAMETERS: [Stat; 7] = [Stat::Str, Stat::Dex, Stat::Vit, Stat::Agi, Stat::Int, Stat::Mnd, Stat::Chr];

pub(super) const BASE_PARAMETERS: [[f64; 7]; 22] = [
    [97.0, 93.0, 90.0, 93.0, 84.0, 84.0, 87.0], // war
    [93.0, 95.0, 97.0, 84.0, 81.0, 90.0, 87.0], // mnk
    [90.0, 84.0, 90.0, 87.0, 87.0, 97.0, 93.0], // whm
    [84.0, 93.0, 84.0, 93.0, 97.0, 87.0, 90.0], // blm
    [90.0, 90.0, 87.0, 87.0, 93.0, 93.0, 90.0], // rdm
    [90.0, 97.0, 90.0, 95.0, 93.0, 81.0, 81.0], // thf
    [95.0, 87.0, 97.0, 81.0, 81.0, 93.0, 93.0], // pld
    [97.0, 93.0, 93.0, 90.0, 93.0, 81.0, 81.0], // drk
    [90.0, 97.0, 90.0, 95.0, 93.0, 81.0, 81.0], // bst
    [90.0, 90.0, 90.0, 84.0, 90.0, 90.0, 95.0], // brd
    [87.0, 90.0, 90.0, 97.0, 87.0, 90.0, 87.0], // rng
    [84.0, 87.0, 84.0, 90.0, 95.0, 95.0, 95.0], // smn
    [93.0, 93.0, 93.0, 90.0, 87.0, 87.0, 90.0], // sam
    [93.0, 95.0, 93.0, 95.0, 90.0, 81.0, 84.0], // nin
    [95.0, 90.0, 93.0, 90.0, 84.0, 87.0, 93.0], // drg
    [87.0, 87.0, 87.0, 87.0, 87.0, 87.0, 87.0], // blu
    [87.0, 93.0, 87.0, 95.0, 93.0, 87.0, 87.0], // cor
    [87.0, 95.0, 90.0, 93.0, 87.0, 84.0, 93.0], // pup
    [90.0, 93.0, 87.0, 95.0, 84.0, 84.0, 95.0], // dnc
    [84.0, 90.0, 87.0, 90.0, 95.0, 90.0, 93.0], // sch
    [84.0, 90.0, 90.0, 87.0, 95.0, 95.0, 87.0], // geo
    [93.0, 90.0, 87.0, 95.0, 90.0, 90.0, 84.0], // run
];

pub(super) const SUB_PARAMETERS: [[f64; 7]; 22] = [
    [15.0, 12.0, 10.0, 12.0, 7.0, 7.0, 9.0], // war
    [12.0, 13.0, 15.0, 7.0, 6.0, 10.0, 9.0], // mnk
    [10.0, 7.0, 10.0, 9.0, 9.0, 15.0, 12.0], // whm
    [7.0, 12.0, 7.0, 12.0, 15.0, 9.0, 10.0], // blm
    [10.0, 10.0, 9.0, 9.0, 12.0, 12.0, 10.0], // rdm
    [10.0, 15.0, 10.0, 13.0, 12.0, 6.0, 6.0], // thf
    [13.0, 9.0, 15.0, 6.0, 6.0, 12.0, 12.0], // pld
    [15.0, 12.0, 12.0, 10.0, 12.0, 6.0, 6.0], // drk
    [9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0], // bst
    [9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0], // brd
    [9.0, 10.0, 10.0, 15.0, 9.0, 10.0, 9.0], // rng
    [7.0, 9.0, 7.0, 10.0, 13.0, 13.0, 13.0], // smn
    [12.0, 12.0, 12.0, 10.0, 9.0, 9.0, 10.0], // sam
    [12.0, 13.0, 12.0, 13.0, 10.0, 6.0, 7.0], // nin
    [13.0, 10.0, 12.0, 10.0, 7.0, 9.0, 12.0], // drg
    [9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0], // blu
    [9.0, 12.0, 9.0, 13.0, 12.0, 9.0, 9.0], // cor
    [9.0, 13.0, 10.0, 12.0, 9.0, 7.0, 12.0], // pup
    [10.0, 12.0, 9.0, 13.0, 7.0, 7.0, 13.0], // dnc
    [7.0, 9.0, 8.0, 9.0, 13.0, 9.0, 11.0], // sch
    [7.0, 10.0, 10.0, 8.0, 13.0, 13.0, 9.0], // geo
    [12.0, 10.0, 9.0, 13.0, 10.0, 10.0, 7.0], // run
];

pub(super) const ARMOR_BONUSES: &[ArmorBonus] = &[

    ArmorBonus { name: "Mummu", plus1: false, slots: Some(&[Slot::Head, Slot::Body, Slot::Hands, Slot::Legs, Slot::Feet, Slot::Ring1, Slot::Ring2]), max_pieces: 5, counts_first: false, per_piece: 8.0, stats: &[Stat::Dex, Stat::Agi, Stat::Vit, Stat::Chr] },

    ArmorBonus { name: "Flamma", plus1: false, slots: Some(&[Slot::Head, Slot::Body, Slot::Hands, Slot::Legs, Slot::Feet, Slot::Ring1, Slot::Ring2]), max_pieces: 5, counts_first: false, per_piece: 8.0, stats: &[Stat::Str, Stat::Dex, Stat::Vit] },

    ArmorBonus { name: "Mallquis", plus1: false, slots: Some(&[Slot::Head, Slot::Body, Slot::Hands, Slot::Legs, Slot::Feet, Slot::Ring1, Slot::Ring2]), max_pieces: 5, counts_first: false, per_piece: 8.0, stats: &[Stat::Vit, Stat::Int, Stat::Mnd] },

    ArmorBonus { name: "Ayanmo", plus1: false, slots: Some(&[Slot::Head, Slot::Body, Slot::Hands, Slot::Legs, Slot::Feet, Slot::Ring1, Slot::Ring2]), max_pieces: 5, counts_first: false, per_piece: 8.0, stats: &[Stat::Str, Stat::Vit, Stat::Mnd] },

    ArmorBonus { name: "Adhemar", plus1: true, slots: None, max_pieces: 16, counts_first: true, per_piece: 2.0, stats: &[Stat::CritRate] },

    ArmorBonus { name: "Amalric", plus1: true, slots: None, max_pieces: 16, counts_first: true, per_piece: 10.0, stats: &[Stat::MagicAttack] },

    ArmorBonus { name: "Lustratio", plus1: true, slots: None, max_pieces: 16, counts_first: true, per_piece: 2.0, stats: &[Stat::WeaponSkillDamage] },

    ArmorBonus { name: "Ryuo", plus1: true, slots: None, max_pieces: 16, counts_first: true, per_piece: 10.0, stats: &[Stat::Attack] },

];

pub(super) const AF_PREFIX: [&str; 22] = ["pummeler", "anchorite", "theophany", "spaekona", "atrophy", "pillager", "reverence", "ignominy", "totomic", "brioso", "orion", "convoker", "wakido", "hachiya", "vishap", "assimilator", "laksamana", "foire", "maxixi", "academic", "geomancy", "runeist"];

pub(super) const TRAITS: &[Trait] = &[

    Trait { stat: Stat::Accuracy, jobs: &[

        (Job::Rng, &[(96, 73.0), (86, 60.0), (70, 48.0), (50, 35.0), (30, 22.0), (10, 10.0)]),

        (Job::Drg, &[(76, 35.0), (60, 22.0), (30, 10.0)]),

        (Job::Dnc, &[(76, 35.0), (60, 22.0), (30, 10.0)]),

        (Job::Run, &[(90, 35.0), (70, 22.0), (50, 10.0)]),

    ] },

    Trait { stat: Stat::RangedAccuracy, jobs: &[

        (Job::Rng, &[(96, 73.0), (86, 60.0), (70, 48.0), (50, 35.0), (30, 22.0), (10, 10.0)]),

        (Job::Drg, &[(76, 35.0), (60, 22.0), (30, 10.0)]),

        (Job::Dnc, &[(76, 35.0), (60, 22.0), (30, 10.0)]),

        (Job::Run, &[(90, 35.0), (70, 22.0), (50, 10.0)]),

    ] },

    Trait { stat: Stat::Attack, jobs: &[

        (Job::Drk, &[(99, 96.0), (91, 84.0), (83, 72.0), (76, 60.0), (70, 48.0), (50, 35.0), (30, 22.0), (10, 10.0)]),

        (Job::War, &[(91, 35.0), (65, 22.0), (30, 10.0)]),

        (Job::Drg, &[(91, 22.0), (10, 10.0)]),

    ] },

    Trait { stat: Stat::RangedAttack, jobs: &[

        (Job::Drk, &[(99, 96.0), (91, 84.0), (83, 72.0), (76, 60.0), (70, 48.0), (50, 35.0), (30, 22.0), (10, 10.0)]),

        (Job::War, &[(91, 35.0), (65, 22.0), (30, 10.0)]),

        (Job::Drg, &[(91, 22.0), (10, 10.0)]),

    ] },

    Trait { stat: Stat::Barrage, jobs: &[

        (Job::Rng, &[(90, 6.0), (70, 5.0), (50, 4.0), (30, 3.0)]),

    ] },

    Trait { stat: Stat::ConserveTp, jobs: &[

        (Job::Drg, &[(97, 26.0), (84, 24.0), (71, 21.0), (58, 18.0), (45, 15.0)]),

        (Job::Dnc, &[(97, 21.0), (87, 17.0), (77, 15.0)]),

        (Job::Rng, &[(91, 18.0), (80, 15.0)]),

    ] },

    Trait { stat: Stat::CritDamage, jobs: &[

        (Job::Thf, &[(97, 14.0), (91, 11.0), (84, 8.0), (78, 5.0)]),

        (Job::Dnc, &[(99, 11.0), (88, 8.0), (80, 5.0)]),

        (Job::Drk, &[(95, 8.0), (85, 5.0)]),

        (Job::War, &[(95, 8.0), (85, 5.0)]),

    ] },

    Trait { stat: Stat::Daken, jobs: &[

        (Job::Nin, &[(95, 40.0), (70, 35.0), (55, 30.0), (40, 25.0), (25, 20.0)]),

    ] },

    Trait { stat: Stat::PdlTrait, jobs: &[

        (Job::Drk, &[(80, 50.0), (70, 40.0), (55, 30.0), (40, 20.0), (20, 10.0)]),

        (Job::Mnk, &[(90, 30.0), (60, 20.0), (30, 10.0)]),

        (Job::Rng, &[(90, 30.0), (60, 20.0), (30, 10.0)]),

        (Job::Drg, &[(90, 30.0), (60, 20.0), (30, 10.0)]),

        (Job::War, &[(80, 20.0), (40, 10.0)]),

        (Job::Sam, &[(80, 20.0), (40, 10.0)]),

        (Job::Bst, &[(90, 20.0), (45, 10.0)]),

        (Job::Pup, &[(90, 20.0), (45, 10.0)]),

        (Job::Dnc, &[(90, 20.0), (45, 10.0)]),

        (Job::Thf, &[(50, 10.0)]),

        (Job::Nin, &[(50, 10.0)]),

        (Job::Rdm, &[(60, 10.0)]),

    ] },

    Trait { stat: Stat::RangedCritDamage, jobs: &[

        (Job::Rng, &[(99, 45.0), (90, 40.0), (80, 35.0), (70, 30.0), (60, 20.0), (50, 10.0)]),

    ] },

    Trait { stat: Stat::Da, jobs: &[

        (Job::War, &[(99, 18.0), (85, 16.0), (75, 14.0), (50, 12.0), (25, 10.0)]),

    ] },

    Trait { stat: Stat::DualWield, jobs: &[

        (Job::Nin, &[(85, 35.0), (65, 30.0), (45, 25.0), (25, 15.0), (10, 10.0)]),

        (Job::Dnc, &[(80, 30.0), (60, 25.0), (40, 15.0), (20, 10.0)]),

        (Job::Thf, &[(98, 25.0), (90, 15.0), (83, 10.0)]),

    ] },

    Trait { stat: Stat::Evasion, jobs: &[

        (Job::Thf, &[(88, 72.0), (76, 60.0), (70, 48.0), (50, 35.0), (30, 22.0), (10, 10.0)]),

        (Job::Dnc, &[(86, 48.0), (75, 35.0), (45, 22.0), (15, 10.0)]),

        (Job::Pup, &[(76, 48.0), (60, 35.0), (40, 22.0), (20, 10.0)]),

    ] },

    Trait { stat: Stat::Fencer, jobs: &[

        (Job::War, &[(97, 5.0), (84, 4.0), (71, 3.0), (58, 2.0), (45, 1.0)]),

        (Job::Bst, &[(94, 3.0), (87, 2.0), (80, 1.0)]),

        (Job::Brd, &[(95, 2.0), (85, 1.0)]),

    ] },

    Trait { stat: Stat::KickAttacks, jobs: &[

        (Job::Mnk, &[(76, 14.0), (71, 12.0), (51, 10.0)]),

    ] },

    Trait { stat: Stat::MagicBurstDamageTrait, jobs: &[

        (Job::Blm, &[(97, 13.0), (84, 11.0), (71, 9.0), (58, 7.0), (45, 5.0)]),

        (Job::Sch, &[(99, 7.0), (89, 7.0), (79, 5.0)]),

        (Job::Nin, &[(90, 7.0), (80, 5.0)]),

        (Job::Rdm, &[(95, 7.0), (85, 5.0)]),

    ] },

    Trait { stat: Stat::MagicAttack, jobs: &[

        (Job::Blm, &[(91, 40.0), (81, 36.0), (70, 32.0), (50, 28.0), (30, 24.0), (10, 20.0)]),

        (Job::Rdm, &[(86, 28.0), (40, 24.0), (20, 20.0)]),

    ] },

    Trait { stat: Stat::MagicDefense, jobs: &[

        (Job::Run, &[(99, 22.0), (91, 20.0), (76, 18.0), (70, 16.0), (50, 14.0), (30, 12.0), (10, 10.0)]),

        (Job::Whm, &[(91, 20.0), (81, 18.0), (70, 16.0), (50, 14.0), (30, 12.0), (10, 10.0)]),

        (Job::Rdm, &[(96, 14.0), (45, 12.0), (25, 10.0)]),

    ] },

    Trait { stat: Stat::MartialArts, jobs: &[

        (Job::Mnk, &[(82, 200.0), (75, 180.0), (61, 160.0), (46, 140.0), (31, 120.0), (16, 100.0), (1, 80.0)]),

        (Job::Pup, &[(97, 160.0), (87, 140.0), (75, 120.0), (50, 100.0), (25, 80.0)]),

    ] },

    Trait { stat: Stat::OccultAcumen, jobs: &[

        (Job::Drk, &[(97, 125.0), (84, 100.0), (71, 75.0), (58, 50.0), (45, 25.0)]),

        (Job::Sch, &[(98, 75.0), (88, 50.0), (78, 25.0)]),

        (Job::Blm, &[(95, 50.0), (85, 25.0)]),

    ] },

    Trait { stat: Stat::Recycle, jobs: &[

        (Job::Rng, &[(50, 30.0), (35, 20.0), (20, 10.0)]),

        (Job::Cor, &[(90, 30.0), (65, 20.0), (35, 10.0)]),

    ] },

    Trait { stat: Stat::SkillchainBonus, jobs: &[

        (Job::Dnc, &[(97, 23.0), (84, 20.0), (71, 16.0), (58, 12.0), (45, 8.0)]),

        (Job::Sam, &[(98, 16.0), (88, 12.0), (78, 8.0)]),

        (Job::Nin, &[(95, 12.0), (85, 8.0)]),

        (Job::Mnk, &[(95, 12.0), (85, 8.0)]),

    ] },

    Trait { stat: Stat::Smite, jobs: &[

        (Job::Drk, &[(95, 5.0), (75, 4.0), (55, 3.0), (35, 2.0), (15, 1.0)]),

        (Job::War, &[(95, 3.0), (65, 2.0), (35, 1.0)]),

        (Job::Mnk, &[(80, 2.0), (40, 1.0)]),

        (Job::Drg, &[(80, 2.0), (40, 1.0)]),

        (Job::Pup, &[(40, 1.0)]),

    ] },

    Trait { stat: Stat::StoreTp, jobs: &[

        (Job::Sam, &[(90, 30.0), (70, 25.0), (50, 20.0), (30, 15.0), (10, 10.0)]),

    ] },

    Trait { stat: Stat::SubtleBlow, jobs: &[

        (Job::Mnk, &[(91, 25.0), (65, 20.0), (40, 15.0), (25, 10.0), (5, 5.0)]),

        (Job::Nin, &[(75, 25.0), (60, 20.0), (45, 15.0), (30, 10.0), (15, 5.0)]),

        (Job::Dnc, &[(86, 20.0), (65, 15.0), (45, 10.0), (25, 5.0)]),

    ] },

    Trait { stat: Stat::Ta, jobs: &[

        (Job::Thf, &[(95, 6.0), (55, 5.0)]),

    ] },

    Trait { stat: Stat::TrueShot, jobs: &[

        (Job::Rng, &[(98, 7.0), (88, 5.0), (78, 3.0)]),

        (Job::Cor, &[(95, 5.0), (85, 3.0)]),

    ] },

    Trait { stat: Stat::WeaponSkillDamageTrait, jobs: &[

        (Job::Drg, &[(95, 21.0), (85, 19.0), (75, 17.0), (65, 13.0), (55, 10.0), (45, 7.0)]),

    ] },

    Trait { stat: Stat::Zanshin, jobs: &[

        (Job::Sam, &[(95, 50.0), (75, 45.0), (50, 35.0), (35, 25.0), (20, 15.0)]),

    ] },

];

pub(super) const COMBAT_SKILLS: [&[(Stat, f64)]; 22] = [

    &[(Stat::HandToHandSkill, 334.0), (Stat::DaggerSkill, 388.0), (Stat::SwordSkill, 398.0), (Stat::GreatSwordSkill, 404.0), (Stat::AxeSkill, 417.0), (Stat::GreatAxeSkill, 424.0), (Stat::ScytheSkill, 404.0), (Stat::PolearmSkill, 388.0), (Stat::ClubSkill, 388.0), (Stat::StaffSkill, 398.0), (Stat::ArcherySkill, 334.0), (Stat::MarksmanshipSkill, 334.0), (Stat::ThrowingSkill, 334.0), (Stat::EvasionSkill, 373.0)], // war

    &[(Stat::HandToHandSkill, 424.0), (Stat::StaffSkill, 398.0), (Stat::ClubSkill, 378.0), (Stat::ThrowingSkill, 300.0), (Stat::EvasionSkill, 404.0)], // mnk

    &[(Stat::ClubSkill, 404.0), (Stat::StaffSkill, 378.0), (Stat::ThrowingSkill, 300.0), (Stat::EvasionSkill, 300.0), (Stat::DivineMagicSkill, 417.0)], // whm

    &[(Stat::DaggerSkill, 334.0), (Stat::ScytheSkill, 300.0), (Stat::ClubSkill, 378.0), (Stat::StaffSkill, 388.0), (Stat::ThrowingSkill, 334.0), (Stat::EvasionSkill, 300.0), (Stat::ElementalMagicSkill, 424.0), (Stat::DarkMagicSkill, 417.0)], // blm

    &[(Stat::DaggerSkill, 398.0), (Stat::SwordSkill, 398.0), (Stat::ClubSkill, 334.0), (Stat::ArcherySkill, 334.0), (Stat::ThrowingSkill, 265.0), (Stat::EvasionSkill, 334.0), (Stat::DivineMagicSkill, 300.0), (Stat::ElementalMagicSkill, 378.0), (Stat::DarkMagicSkill, 300.0)], // rdm

    &[(Stat::HandToHandSkill, 300.0), (Stat::DaggerSkill, 424.0), (Stat::SwordSkill, 334.0), (Stat::ClubSkill, 300.0), (Stat::ArcherySkill, 368.0), (Stat::MarksmanshipSkill, 378.0), (Stat::ThrowingSkill, 334.0), (Stat::EvasionSkill, 424.0)], // thf

    &[(Stat::SwordSkill, 424.0), (Stat::ClubSkill, 417.0), (Stat::StaffSkill, 417.0), (Stat::GreatSwordSkill, 398.0), (Stat::DaggerSkill, 368.0), (Stat::PolearmSkill, 300.0), (Stat::EvasionSkill, 373.0), (Stat::DivineMagicSkill, 404.0)], // pld

    &[(Stat::DaggerSkill, 373.0), (Stat::SwordSkill, 388.0), (Stat::GreatSwordSkill, 417.0), (Stat::AxeSkill, 388.0), (Stat::GreatAxeSkill, 388.0), (Stat::ScytheSkill, 424.0), (Stat::ClubSkill, 368.0), (Stat::MarksmanshipSkill, 300.0), (Stat::EvasionSkill, 373.0), (Stat::ElementalMagicSkill, 404.0), (Stat::DarkMagicSkill, 417.0)], // drk

    &[(Stat::DaggerSkill, 378.0), (Stat::SwordSkill, 300.0), (Stat::AxeSkill, 424.0), (Stat::ScytheSkill, 368.0), (Stat::ClubSkill, 334.0), (Stat::EvasionSkill, 373.0)], // bst

    &[(Stat::DaggerSkill, 388.0), (Stat::SwordSkill, 368.0), (Stat::ClubSkill, 334.0), (Stat::StaffSkill, 378.0), (Stat::ThrowingSkill, 300.0), (Stat::EvasionSkill, 334.0)], // brd

    &[(Stat::DaggerSkill, 388.0), (Stat::SwordSkill, 334.0), (Stat::AxeSkill, 388.0), (Stat::ClubSkill, 300.0), (Stat::ArcherySkill, 424.0), (Stat::MarksmanshipSkill, 424.0), (Stat::ThrowingSkill, 368.0), (Stat::EvasionSkill, 300.0)], // rng

    &[(Stat::DaggerSkill, 300.0), (Stat::ClubSkill, 378.0), (Stat::StaffSkill, 398.0), (Stat::EvasionSkill, 300.0), (Stat::SummoningMagicSkill, 417.0)], // smn

    &[(Stat::DaggerSkill, 300.0), (Stat::SwordSkill, 378.0), (Stat::PolearmSkill, 388.0), (Stat::GreatKatanaSkill, 424.0), (Stat::ClubSkill, 300.0), (Stat::ArcherySkill, 378.0), (Stat::ThrowingSkill, 378.0), (Stat::EvasionSkill, 404.0)], // sam

    &[(Stat::HandToHandSkill, 300.0), (Stat::DaggerSkill, 378.0), (Stat::SwordSkill, 373.0), (Stat::KatanaSkill, 424.0), (Stat::GreatKatanaSkill, 368.0), (Stat::ClubSkill, 300.0), (Stat::ArcherySkill, 300.0), (Stat::MarksmanshipSkill, 373.0), (Stat::ThrowingSkill, 424.0), (Stat::EvasionSkill, 417.0), (Stat::NinjutsuSkill, 417.0)], // nin

    &[(Stat::DaggerSkill, 300.0), (Stat::SwordSkill, 368.0), (Stat::PolearmSkill, 424.0), (Stat::ClubSkill, 300.0), (Stat::StaffSkill, 388.0), (Stat::EvasionSkill, 388.0)], // drg

    &[(Stat::SwordSkill, 424.0), (Stat::ClubSkill, 388.0), (Stat::EvasionSkill, 368.0), (Stat::BlueMagciSkill, 424.0)], // blu

    &[(Stat::DaggerSkill, 404.0), (Stat::SwordSkill, 388.0), (Stat::MarksmanshipSkill, 398.0), (Stat::ThrowingSkill, 378.0), (Stat::EvasionSkill, 334.0)], // cor

    &[(Stat::HandToHandSkill, 424.0), (Stat::DaggerSkill, 368.0), (Stat::ClubSkill, 334.0), (Stat::ThrowingSkill, 378.0), (Stat::EvasionSkill, 398.0)], // pup

    &[(Stat::DaggerSkill, 424.0), (Stat::HandToHandSkill, 334.0), (Stat::SwordSkill, 334.0), (Stat::ThrowingSkill, 378.0), (Stat::EvasionSkill, 404.0)], // dnc

    &[(Stat::DaggerSkill, 334.0), (Stat::ClubSkill, 378.0), (Stat::StaffSkill, 378.0), (Stat::ThrowingSkill, 334.0), (Stat::EvasionSkill, 300.0), (Stat::DivineMagicSkill, 404.0), (Stat::ElementalMagicSkill, 404.0), (Stat::DarkMagicSkill, 404.0)], // sch

    &[(Stat::ClubSkill, 404.0), (Stat::StaffSkill, 378.0), (Stat::DaggerSkill, 368.0), (Stat::EvasionSkill, 334.0), (Stat::ElementalMagicSkill, 404.0), (Stat::DarkSkill, 373.0)], // geo

    &[(Stat::GreatSwordSkill, 424.0), (Stat::SwordSkill, 417.0), (Stat::GreatAxeSkill, 398.0), (Stat::AxeSkill, 388.0), (Stat::ClubSkill, 368.0), (Stat::EvasionSkill, 404.0), (Stat::DivineMagicSkill, 398.0)], // run

];

pub(super) const MERIT_STATS: [&[(Stat, f64)]; 22] = [

    &[(Stat::Da, 5.0)], // war

    &[(Stat::KickAttacks, 5.0)], // mnk

    &[], // whm

    &[(Stat::MagicAttack, 10.0), (Stat::MagicAccuracy, 25.0)], // blm

    &[(Stat::MagicAccuracy, 40.0), (Stat::Accuracy, 0.0), (Stat::EnSpellDamage, 15.0)], // rdm

    &[(Stat::Ta, 5.0), (Stat::Accuracy, 15.0), (Stat::RangedAccuracy, 15.0)], // thf

    &[], // pld

    &[], // drk

    &[], // bst

    &[], // brd

    &[(Stat::Recycle, 25.0)], // rng

    &[], // smn

    &[(Stat::Zanshin, 5.0), (Stat::StoreTp, 10.0)], // sam

    &[(Stat::SubtleBlow, 5.0), (Stat::NinjutsuMagicAccuracy, 25.0), (Stat::NinjutsuMagicAttack, 30.0), (Stat::NinjutsuMagicDamage, 0.0)], // nin

    &[], // drg

    &[], // blu

    &[], // cor

    &[], // pup

    &[], // dnc

    &[(Stat::HelixMagicAccuracy, 15.0), (Stat::HelixMagicAttack, 10.0)], // sch

    &[], // geo

    &[], // run

];

pub(super) const MASTERY_STATS: [&[(Stat, f64)]; 22] = [

    &[(Stat::Accuracy, 26.0), (Stat::RangedAccuracy, 26.0), (Stat::Attack, 70.0), (Stat::RangedAttack, 70.0), (Stat::MagicAccuracy, 36.0), (Stat::FencerTpBonus, 230.0), (Stat::CritRate, 10.0), (Stat::CritDamage, 10.0), (Stat::Da, 10.0), (Stat::Evasion, 36.0), (Stat::MagicEvasion, 36.0), (Stat::WeaponSkillDamage, 3.0)], // war

    &[(Stat::Accuracy, 41.0), (Stat::RangedAccuracy, 41.0), (Stat::Attack, 40.0), (Stat::RangedAttack, 40.0), (Stat::MagicAccuracy, 36.0), (Stat::Evasion, 42.0), (Stat::MagicEvasion, 36.0), (Stat::SubtleBlow, 10.0), (Stat::MartialArts, 10.0), (Stat::KickAttacksAttack, 40.0), (Stat::KickAttacksAccuracy, 20.0)], // mnk

    &[(Stat::Accuracy, 14.0), (Stat::RangedAccuracy, 14.0), (Stat::MagicAccuracy, 70.0), (Stat::MagicAttack, 22.0), (Stat::MagicDefense, 50.0), (Stat::DivineMagicSkill, 36.0)], // whm

    &[(Stat::MagicBurstDamageTrait, 43.0), (Stat::MagicAccuracy, 32.0), (Stat::MagicDamage, 43.0), (Stat::MagicDefense, 14.0), (Stat::MagicAttack, 50.0), (Stat::MagicEvasion, 42.0), (Stat::ElementalMagicSkill, 36.0), (Stat::DarkMagicSkill, 36.0)], // blm

    &[(Stat::MagicAttack, 48.0), (Stat::MagicAccuracy, 90.0), (Stat::MagicDefense, 28.0), (Stat::MagicEvasion, 56.0), (Stat::Accuracy, 22.0), (Stat::RangedAccuracy, 22.0), (Stat::EnSpellDamage, 23.0)], // rdm

    &[(Stat::SneakAttackBonus, 20.0), (Stat::TrickAttackBonus, 20.0), (Stat::Attack, 50.0), (Stat::RangedAttack, 50.0), (Stat::Evasion, 70.0), (Stat::Accuracy, 36.0), (Stat::RangedAccuracy, 36.0), (Stat::MagicEvasion, 36.0), (Stat::MagicAccuracy, 36.0), (Stat::Ta, 8.0), (Stat::CritDamage, 8.0), (Stat::DualWield, 5.0), (Stat::TaAttack, 20.0)], // thf

    &[(Stat::Accuracy, 28.0), (Stat::RangedAccuracy, 28.0), (Stat::Attack, 28.0), (Stat::RangedAttack, 28.0), (Stat::Evasion, 22.0), (Stat::MagicEvasion, 42.0), (Stat::DivineMagicSkill, 36.0), (Stat::MagicAccuracy, 42.0)], // pld

    &[(Stat::Attack, 106.0), (Stat::RangedAttack, 106.0), (Stat::Evasion, 22.0), (Stat::MagicEvasion, 36.0), (Stat::Accuracy, 22.0), (Stat::RangedAccuracy, 22.0), (Stat::MagicAccuracy, 42.0), (Stat::DarkMagicSkill, 36.0), (Stat::CritDamage, 8.0), (Stat::WeaponSkillDamage, 8.0)], // drk

    &[(Stat::Attack, 70.0), (Stat::RangedAttack, 70.0), (Stat::Accuracy, 36.0), (Stat::RangedAccuracy, 36.0), (Stat::MagicEvasion, 36.0), (Stat::MagicAccuracy, 36.0), (Stat::FencerTpBonus, 230.0), (Stat::Evasion, 36.0)], // bst

    &[(Stat::Evasion, 22.0), (Stat::Accuracy, 21.0), (Stat::RangedAccuracy, 21.0), (Stat::MagicDefense, 15.0), (Stat::MagicEvasion, 36.0), (Stat::MagicAccuracy, 36.0)], // brd

    &[(Stat::DoubleShot, 20.0), (Stat::Attack, 70.0), (Stat::RangedAttack, 70.0), (Stat::Evasion, 14.0), (Stat::Accuracy, 70.0), (Stat::RangedAccuracy, 70.0), (Stat::MagicEvasion, 36.0), (Stat::ConverveTp, 15.0), (Stat::TrueShot, 8.0), (Stat::RangedCritDamage, 8.0), (Stat::Barrage, 1.0), (Stat::BarrageRangedAttack, 60.0)], // rng

    &[(Stat::MagicDefense, 22.0), (Stat::MagicEvasion, 22.0), (Stat::Evasion, 22.0), (Stat::SummoningMagicSkill, 36.0)], // smn

    &[(Stat::Attack, 70.0), (Stat::RangedAttack, 70.0), (Stat::Evasion, 36.0), (Stat::Accuracy, 36.0), (Stat::RangedAccuracy, 36.0), (Stat::MagicEvasion, 36.0), (Stat::Zanshin, 10.0), (Stat::ZanshinAttack, 40.0), (Stat::StoreTp, 8.0), (Stat::SkillchainBonus, 8.0)], // sam

    &[(Stat::NinjutsuMagicDamage, 40.0), (Stat::NinjutsuMagicAccuracy, 20.0), (Stat::Attack, 70.0), (Stat::RangedAttack, 70.0), (Stat::Evasion, 64.0), (Stat::Accuracy, 56.0), (Stat::RangedAccuracy, 56.0), (Stat::MagicAttack, 28.0), (Stat::MagicEvasion, 50.0), (Stat::MagicAccuracy, 50.0), (Stat::NinjutsuSkill, 36.0), (Stat::Daken, 14.0), (Stat::WeaponSkillDamage, 5.0)], // nin

    &[(Stat::Attack, 70.0), (Stat::RangedAttack, 70.0), (Stat::Evasion, 36.0), (Stat::Accuracy, 64.0), (Stat::RangedAccuracy, 64.0), (Stat::MagicEvasion, 36.0), (Stat::CritDamage, 8.0)], // drg

    &[(Stat::Attack, 70.0), (Stat::RangedAttack, 70.0), (Stat::Evasion, 36.0), (Stat::Accuracy, 36.0), (Stat::RangedAccuracy, 36.0), (Stat::MagicDefense, 36.0), (Stat::MagicAttack, 36.0), (Stat::MagicEvasion, 36.0), (Stat::BlueMagicSkill, 36.0)], // blu

    &[(Stat::TripleShot, 20.0), (Stat::TrueShot, 6.0), (Stat::RangedAccuracy, 56.0), (Stat::Attack, 36.0), (Stat::RangedAttack, 36.0), (Stat::Accuracy, 36.0), (Stat::Evasion, 22.0), (Stat::MagicAttack, 14.0), (Stat::MagicEvasion, 36.0), (Stat::MagicAccuracy, 36.0), (Stat::QuickDrawDamage, 40.0)], // cor

    &[(Stat::MartialArts, 40.0), (Stat::Attack, 42.0), (Stat::RangedAttack, 42.0), (Stat::Evasion, 56.0), (Stat::Accuracy, 50.0), (Stat::RangedAccuracy, 50.0), (Stat::MagicEvasion, 36.0), (Stat::MagicAccuracy, 36.0)], // pup

    &[(Stat::FlourishChrPct, 20.0), (Stat::BuildingFlourishWsd, 20.0), (Stat::Attack, 42.0), (Stat::RangedAttack, 42.0), (Stat::Evasion, 64.0), (Stat::Accuracy, 64.0), (Stat::RangedAccuracy, 64.0), (Stat::MagicEvasion, 36.0), (Stat::MagicAccuracy, 36.0), (Stat::SubtleBlow, 13.0), (Stat::CritDamage, 8.0), (Stat::SkillchainBonus, 8.0), (Stat::DualWield, 5.0)], // dnc

    &[(Stat::MagicDefense, 22.0), (Stat::MagicAttack, 36.0), (Stat::MagicEvasion, 42.0), (Stat::MagicAccuracy, 42.0), (Stat::DarkMagicSkill, 36.0), (Stat::ElementalMagicSkill, 36.0), (Stat::MagicBurstDamageTrait, 13.0)], // sch

    &[(Stat::MagicAccuracy, 50.0), (Stat::MagicAttack, 62.0), (Stat::MagicDefense, 28.0), (Stat::MagicEvasion, 50.0), (Stat::ElementalMagicSkill, 36.0), (Stat::DarkMagicSkill, 36.0), (Stat::MagicDamage, 13.0)], // geo

    &[(Stat::LungeBonus, 20.0), (Stat::Attack, 50.0), (Stat::RangedAttack, 50.0), (Stat::Evasion, 56.0), (Stat::Accuracy, 56.0), (Stat::RangedAccuracy, 56.0), (Stat::MagicDefense, 56.0), (Stat::MagicEvasion, 70.0), (Stat::MagicAccuracy, 36.0)], // run

];

pub(super) const ALL_COMBAT_SKILLS: &[Stat] = &[Stat::HandToHandSkill, Stat::DaggerSkill, Stat::SwordSkill, Stat::GreatSwordSkill, Stat::AxeSkill, Stat::GreatAxeSkill, Stat::ScytheSkill, Stat::PolearmSkill, Stat::KatanaSkill, Stat::GreatKatanaSkill, Stat::ClubSkill, Stat::StaffSkill, Stat::ArcherySkill, Stat::MarksmanshipSkill, Stat::ThrowingSkill, Stat::EvasionSkill, Stat::DivineMagicSkill, Stat::ElementalMagicSkill, Stat::DarkMagicSkill, Stat::NinjutsuSkill, Stat::SummoningMagicSkill, Stat::BlueMagicSkill];

pub(super) const METADATA: &[Stat] = &[Stat::Name, Stat::Name2, Stat::Type, Stat::Dmg, Stat::Delay, Stat::Jobs, Stat::SkillType, Stat::Rank];

pub(super) const RELIC_MAIN: &[(&str, &[(Stat, f64)])] = &[

    ("Mandau", &[(Stat::CritRate, 5.0), (Stat::CritDamage, 5.0)]),

    ("Ragnarok", &[(Stat::CritRate, 10.0), (Stat::Accuracy, 15.0)]),

    ("Guttler", &[(Stat::AttackPct, 0.09765625)]),

    ("Bravura", &[(Stat::Dt, -20.0)]),

    ("Apocalypse", &[(Stat::JaHaste, 10.0), (Stat::Accuracy, 15.0)]),

    ("Gungnir", &[(Stat::AttackPct, 0.048828125), (Stat::Da, 5.0)]),

    ("Kikoku", &[(Stat::AttackPct, 0.09765625), (Stat::SubtleBlow, 10.0)]),

    ("Amanomurakumo", &[(Stat::Zanshin, 10.0), (Stat::StoreTp, 10.0)]),

    ("Mjollnir", &[(Stat::Accuracy, 20.0), (Stat::MagicAccuracy, 20.0)]),

    ("Claustrum", &[(Stat::Dt, -20.0)]),

    ("Spharai", &[(Stat::KickAttacks, 15.0), (Stat::SubtleBlow, 10.0)]),

];

pub(super) const RELIC_RANGED: &[(&str, &[(Stat, f64)])] = &[

    ("Yoichinoyumi", &[(Stat::RangedAccuracy, 30.0)]),

    ("Annihilator", &[(Stat::RangedAttackPct, 0.09765625)]),

];

pub(super) const MYTHIC: &[(&str, [(Stat, f64); 2])] = &[

    ("Conqueror", [(Stat::Accuracy, 46.15), (Stat::Attack, 90.15)]),

    ("Glanzfaust", [(Stat::Accuracy, 46.15), (Stat::Attack, 90.15)]),

    ("Vajra", [(Stat::Accuracy, 46.15), (Stat::Attack, 90.15)]),

    ("Burtgang", [(Stat::Accuracy, 46.15), (Stat::Attack, 90.15)]),

    ("Liberator", [(Stat::Accuracy, 46.15), (Stat::Attack, 90.15)]),

    ("Aymur", [(Stat::Accuracy, 46.15), (Stat::Attack, 90.15)]),

    ("Kogarasumaru", [(Stat::Accuracy, 46.15), (Stat::Attack, 90.15)]),

    ("Nagi", [(Stat::Accuracy, 46.15), (Stat::Attack, 90.15)]),

    ("Nirvana", [(Stat::Accuracy, 46.15), (Stat::Attack, 90.15)]),

    ("Ryunohige", [(Stat::Accuracy, 46.15), (Stat::Attack, 90.15)]),

    ("Kenkonken", [(Stat::Accuracy, 46.15), (Stat::Attack, 90.15)]),

    ("Terpsichore", [(Stat::Accuracy, 46.15), (Stat::Attack, 90.15)]),

    ("Epeolatry", [(Stat::Accuracy, 46.15), (Stat::Attack, 90.15)]),

    ("Gastraphetes", [(Stat::RangedAccuracy, 46.15), (Stat::RangedAttack, 90.15)]),

    ("Death Penalty", [(Stat::RangedAccuracy, 46.15), (Stat::RangedAttack, 90.15)]),

    ("Tizona", [(Stat::Accuracy, 46.15), (Stat::MagicAccuracy, 46.15)]),

    ("Carnwenhan", [(Stat::MagicAccuracy, 46.15), (Stat::Accuracy, 46.15)]),

    ("Yagrush", [(Stat::MagicAccuracy, 46.15), (Stat::Accuracy, 46.15)]),

    ("Laevateinn", [(Stat::MagicAccuracy, 46.15), (Stat::MagicAttack, 46.15)]),

    ("Tupsimati", [(Stat::MagicAccuracy, 46.15), (Stat::MagicAttack, 46.15)]),

    ("Idris", [(Stat::MagicAccuracy, 46.15), (Stat::MagicAttack, 46.15)]),

    ("Murgleis", [(Stat::MagicAccuracy, 46.15), (Stat::MagicAttack, 46.15)]),

];

pub(super) const PRIME_STATS: &[(&str, &[Stat])] = &[

    ("Caliburnus", &[Stat::Pdl]),

    ("Dokoku", &[Stat::Pdl]),

    ("Earp", &[Stat::Pdl]),

    ("Foenaria", &[Stat::Pdl]),

    ("Gae Buide", &[Stat::Pdl]),

    ("Helheim", &[Stat::Pdl]),

    ("Kusanagi-no-Tsurugi", &[Stat::Pdl]),

    ("Laphria", &[Stat::Pdl]),

    ("Mpu Gandring", &[Stat::Pdl]),

    ("Pinaka", &[Stat::Pdl]),

    ("Spalirisos", &[Stat::Pdl]),

    ("Varga Purnikawa", &[Stat::Pdl]),

    ("Lorg Mor", &[Stat::MagicDamage]),

    ("Opashoro", &[Stat::MagicDamage, Stat::MagicAttack]),

];

pub(super) const PRIME_TIERS: &[(Stat, [[f64; 3]; 2])] = &[

    (Stat::Pdl, [[4.0, 7.0, 10.0], [6.0, 9.0, 12.0]]),

    (Stat::MagicDamage, [[20.0, 20.0, 20.0], [30.0, 30.0, 30.0]]),

    (Stat::MagicAttack, [[20.0, 20.0, 20.0], [30.0, 30.0, 30.0]]),

];

pub(super) const SMITE: [f64; 6] = [0.0, 0.09765625, 0.1484375, 0.19921875, 0.25, 0.296875];

pub(super) const FENCER: [(f64, f64); 9] = [(0.0, 0.0), (200.0, 3.0), (300.0, 5.0), (400.0, 7.0), (450.0, 9.0), (500.0, 10.0), (550.0, 11.0), (600.0, 12.0), (630.0, 13.0)];
