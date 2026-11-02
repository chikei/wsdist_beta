//! Candidate pruning may only drop armor and accessories that cannot change the score.

use std::path::Path;
use std::sync::Arc;

use wsdist_core::actions::{evaluate_action, optimizer_spell_type, resolve_spell_name};
use wsdist_core::model::{
    Ability, Abilities, ActionRequest, BuffInputs, CandidatePools, Enemy, EvaluationContext, GearId, Job, Metric,
    OptimizeOptions, OptimizeRequest, Player, PlayerInput, RandomSource, Slot, SpellRequest, WsRequest,
};
use wsdist_core::optimizer::probe::{dt_for_player, optimizer_ws_type};
use wsdist_core::optimizer::prune_candidates;
use wsdist_core::rng::RandomSources;
use wsdist_core::weaponskills::resolve_name;
use wsdist_core::{Catalog, aggregate_buffs, build_enemy, build_player};

const WEAPON_SLOTS: [Slot; 4] = [Slot::Main, Slot::Sub, Slot::Ranged, Slot::Ammo];

enum Action { Round(&'static str), Ws(&'static str, &'static str), Spell(&'static str, &'static str) }

struct Target {
    main: Job,
    sub: Job,
    weapons: &'static [(Slot, &'static str)],
    abilities: &'static [Ability],
    action: Action,
    dt: [f64; 2],
}

const TARGETS: &[Target] = &[
    Target { main: Job::War, sub: Job::Sam, weapons: &[(Slot::Main, "Naegling"), (Slot::Sub, "Blurred Shield +1")],
        abilities: &[Ability::Berserk], action: Action::Ws("Savage Blade", "Damage dealt"), dt: [0.0, 0.0] },
    Target { main: Job::War, sub: Job::Sam, weapons: &[(Slot::Main, "Naegling"), (Slot::Sub, "Blurred Shield +1")],
        abilities: &[], action: Action::Ws("Savage Blade", "TP return"), dt: [-30.0, -20.0] },
    Target { main: Job::Thf, sub: Job::Dnc, weapons: &[(Slot::Main, "Tauret"), (Slot::Sub, "Gleti's Knife")],
        abilities: &[Ability::SneakAttack, Ability::TrickAttack, Ability::ClimacticFlourish],
        action: Action::Ws("Rudra's Storm", "Damage dealt"), dt: [0.0, 0.0] },
    Target { main: Job::Thf, sub: Job::Nin, weapons: &[(Slot::Main, "Tauret"), (Slot::Sub, "Gleti's Knife")],
        abilities: &[], action: Action::Ws("Aeolian Edge", "Magic accuracy"), dt: [0.0, 0.0] },
    Target { main: Job::Nin, sub: Job::War, weapons: &[(Slot::Main, "Kikoku"), (Slot::Sub, "Hitaki"), (Slot::Ammo, "Seki Shuriken")],
        abilities: &[], action: Action::Ws("Blade: Chi", "Damage dealt"), dt: [0.0, 0.0] },
    Target { main: Job::Sam, sub: Job::War, weapons: &[(Slot::Main, "Masamune"), (Slot::Sub, "Utu Grip")],
        abilities: &[Ability::Hasso], action: Action::Round("Time to WS"), dt: [0.0, 0.0] },
    Target { main: Job::Nin, sub: Job::War, weapons: &[(Slot::Main, "Kikoku"), (Slot::Sub, "Hitaki"), (Slot::Ammo, "Seki Shuriken")],
        abilities: &[], action: Action::Round("DPS"), dt: [-50.0, 0.0] },
    Target { main: Job::Rdm, sub: Job::Nin, weapons: &[(Slot::Main, "Naegling"), (Slot::Sub, "Daybreak")],
        abilities: &[Ability::EnSpell], action: Action::Round("Damage dealt"), dt: [0.0, 0.0] },
    Target { main: Job::Rng, sub: Job::War, weapons: &[(Slot::Main, "Malevolence"), (Slot::Ranged, "Fomalhaut"), (Slot::Ammo, "Chrono Bullet")],
        abilities: &[Ability::Sharpshot], action: Action::Ws("Last Stand", "Damage dealt"), dt: [0.0, 0.0] },
    Target { main: Job::Cor, sub: Job::Nin, weapons: &[(Slot::Main, "Naegling"), (Slot::Ranged, "Death Penalty"), (Slot::Ammo, "Chrono Bullet")],
        abilities: &[], action: Action::Ws("Leaden Salute", "Damage dealt"), dt: [0.0, 0.0] },
    Target { main: Job::Blm, sub: Job::Rdm, weapons: &[(Slot::Main, "Laevateinn"), (Slot::Sub, "Khonsu")],
        abilities: &[Ability::MagicBurst], action: Action::Spell("Fire VI", "Damage dealt"), dt: [0.0, 0.0] },
    Target { main: Job::Nin, sub: Job::War, weapons: &[(Slot::Main, "Kikoku"), (Slot::Sub, "Hitaki")],
        abilities: &[Ability::Futae], action: Action::Spell("Raiton: San", "Damage dealt"), dt: [0.0, 0.0] },
    Target { main: Job::Cor, sub: Job::Nin, weapons: &[(Slot::Main, "Naegling"), (Slot::Ranged, "Death Penalty"), (Slot::Ammo, "Chrono Bullet")],
        abilities: &[], action: Action::Spell("Fire Shot", "TP return"), dt: [0.0, 0.0] },
    Target { main: Job::Rng, sub: Job::War, weapons: &[(Slot::Main, "Malevolence"), (Slot::Ranged, "Fomalhaut"), (Slot::Ammo, "Chrono Bullet")],
        abilities: &[Ability::Barrage], action: Action::Spell("Ranged Attack", "TP > Damage"), dt: [0.0, 0.0] },
];

fn catalog() -> Arc<Catalog> {
    Arc::new(Catalog::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap())
}

fn weapon(catalog: &Catalog, slot: Slot, name: &str, job: Job) -> GearId {
    catalog.slot(slot).iter().copied().find(|&id| { let piece = catalog.piece(id); piece.name.as_ref() == name && piece.eligible_for(job) })
        .unwrap_or_else(|| panic!("{name} is not a {} piece for {job:?}", slot.name()))
}

fn request(catalog: &Catalog, target: &Target, with_empty: bool) -> OptimizeRequest {
    let mut gearset = [catalog.empty; Slot::COUNT];
    for &(slot, name) in target.weapons { gearset[slot.index()] = weapon(catalog, slot, name, target.main); }
    let mut abilities = Abilities::default();
    abilities.enhancing_skill = 500.0;
    abilities.set_storm(Some(Arc::from("None")));
    for &ability in target.abilities { abilities.set(ability, true); }
    let player = PlayerInput { main_job: target.main, sub_job: Some(target.sub), master_level: 20, gearset,
        effects: Arc::new(aggregate_buffs(&BuffInputs::default(), catalog).unwrap()), abilities };
    let effects = Arc::clone(&player.effects);
    let enemy = build_enemy(&catalog.enemy(catalog.enemy_menu()[0]).to_input(), &effects).unwrap();
    // Every eligible armor piece, as after "Select ALL"; weapons stay equipped.
    let candidates: CandidatePools = std::array::from_fn(|index| {
        let slot = Slot::ALL[index];
        if WEAPON_SLOTS.contains(&slot) { return Vec::new(); }
        catalog.slot(slot).iter().copied()
            .filter(|&id| catalog.piece(id).eligible_for(target.main) && (with_empty || id != catalog.empty)).collect()
    });
    let (action, ws_name, spell_name) = match target.action {
        Action::Round(metric) => (ActionRequest::Round { metric: Metric::new(metric), starting_tp: 0.0, threshold: 1000.0 }, "None", "None"),
        Action::Ws(name, metric) => (ActionRequest::Ws(WsRequest { skill: resolve_name(name).unwrap(), ws_type: optimizer_ws_type(name),
            tp: 2000.0, metric: Metric::new(metric) }), name, "None"),
        Action::Spell(name, metric) => (ActionRequest::Spell(SpellRequest { spell: resolve_spell_name(name).unwrap(),
            spell_type: optimizer_spell_type(name), metric: Metric::new(metric) }), "None", name),
    };
    OptimizeRequest { player, enemy, action, candidates, ws_name: Arc::from(ws_name), spell_name: Arc::from(spell_name),
        min_pdt: target.dt[0], min_mdt: target.dt[1], options: OptimizeOptions::default(),
        similar_results: false, similar_percent: 0.0, verbose_swaps: false }
}

fn label(target: &Target) -> String {
    let action = match target.action { Action::Round(metric) => format!("round {metric}"),
        Action::Ws(name, metric) | Action::Spell(name, metric) => format!("{name} {metric}") };
    format!("{:?}/{:?} {action}", target.main, target.sub)
}

/// Score and per-channel DT feasibility, with identical random draws.
fn evaluate(request: &OptimizeRequest, enemy: &Enemy, gearset: [GearId; Slot::COUNT],
    context: &mut EvaluationContext<RandomSources>, player: &mut Player) -> (f64, [bool; 2]) {
    let input = PlayerInput { gearset, ..request.player.clone() };
    build_player(&input, context, player).unwrap();
    context.random.reseed(11);
    let metric = evaluate_action(player, enemy, &request.action, context).unwrap().metric;
    let dt = dt_for_player(player);
    (metric, [dt[0] <= request.min_pdt, dt[1] <= request.min_mdt])
}

#[test]
fn pruned_armor_never_changes_score_or_damage_taken_feasibility() {
    let catalog = catalog();
    let mut context = EvaluationContext::new(Arc::clone(&catalog), RandomSources::from_seed(0));
    let mut player = Player::new([catalog.empty; Slot::COUNT]);
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = |bound: usize| {
        state ^= state << 13; state ^= state >> 7; state ^= state << 17;
        (state % bound as u64) as usize
    };
    for target in TARGETS {
        let label = label(target);
        let request = request(&catalog, target, true);
        let pruned = prune_candidates(&request, &catalog);
        let removed: Vec<Vec<GearId>> = request.candidates.iter().zip(&pruned)
            .map(|(pool, kept)| pool.iter().copied().filter(|id| !kept.contains(id)).collect()).collect();
        let total: usize = request.candidates.iter().map(Vec::len).sum();
        let dropped: usize = removed.iter().map(Vec::len).sum();
        assert!(pruned.iter().zip(&request.candidates).all(|(kept, pool)| kept.iter().all(|id| pool.contains(id))), "{label}: pruning only removes");
        assert!(WEAPON_SLOTS.iter().all(|slot| pruned[slot.index()] == request.candidates[slot.index()]), "{label}: weapon pools stay whole");
        println!("{label}: pruned {dropped} of {total} candidates");
        for sample in 0..120 {
            // Bias toward removed pieces so every slot exercises them.
            let original: [GearId; Slot::COUNT] = std::array::from_fn(|index| {
                let pool = if !removed[index].is_empty() && next(2) == 0 { &removed[index] } else { &request.candidates[index] };
                if pool.is_empty() { request.player.gearset[index] } else { pool[next(pool.len())] }
            });
            let neutral = original.map(|id| if removed.iter().any(|pool| pool.contains(&id)) { catalog.empty } else { id });
            let expected = evaluate(&request, &request.enemy, original, &mut context, &mut player);
            let actual = evaluate(&request, &request.enemy, neutral, &mut context, &mut player);
            assert_eq!(actual.0.to_bits(), expected.0.to_bits(), "{label} sample {sample}: score {} != {}", actual.0, expected.0);
            assert_eq!(actual.1, expected.1, "{label} sample {sample}: DT feasibility");
        }
    }
}

#[test]
fn a_slot_without_empty_keeps_one_distinct_neutral_candidate() {
    let catalog = catalog();
    for target in TARGETS {
        let label = label(target);
        let with_empty = request(&catalog, target, true);
        let neutral_pools = prune_candidates(&with_empty, &catalog);
        let request = request(&catalog, target, false);
        let pruned = prune_candidates(&request, &catalog);
        let mut stand_ins = [None; Slot::COUNT];
        for slot in Slot::ALL.iter().copied().filter(|slot| !WEAPON_SLOTS.contains(slot)) {
            let index = slot.index();
            let kept: Vec<GearId> = neutral_pools[index].iter().copied().filter(|&id| id != catalog.empty).collect();
            let extra: Vec<GearId> = pruned[index].iter().copied().filter(|id| !kept.contains(id)).collect();
            let lost = request.candidates[index].len() > kept.len();
            assert_eq!(extra.len(), usize::from(lost), "{label} {}: one stand-in exactly when candidates were dropped", slot.name());
            stand_ins[index] = extra.first().copied();
        }
        for (first, second) in [(Slot::Ring1, Slot::Ring2), (Slot::Ear1, Slot::Ear2)] {
            if let (Some(a), Some(b)) = (stand_ins[first.index()], stand_ins[second.index()]) {
                assert_ne!(a, b, "{label}: {} and {} stand-ins can be worn together", first.name(), second.name());
            }
        }
    }
}
