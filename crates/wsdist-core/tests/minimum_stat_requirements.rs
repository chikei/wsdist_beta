//! Minimum stat totals bound the optimizer's search; a zero minimum leaves its stat unconstrained.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use wsdist_core::model::{
    Abilities, ActionRequest, BuffInputs, CandidatePools, EvaluationContext, GearId, Job, MINIMUM_STATS, Metric,
    MinimumStat, OptimizeOptions, OptimizeRequest, OptimizeResult, OptimizeTermination, Player, PlayerInput, Slot,
    WsRequest, build_player,
};
use wsdist_core::optimizer::optimize;
use wsdist_core::optimizer::probe::{optimizer_ws_type, stat_totals};
use wsdist_core::rng::RandomSources;
use wsdist_core::stats::{Stat, Stats};
use wsdist_core::weaponskills::resolve_name;
use wsdist_core::{Catalog, aggregate_buffs, build_enemy};

/// Indices into `MINIMUM_STATS`.
const COMBINED_SUBTLE_BLOW: usize = 0;
const DOUBLE_ATTACK: usize = 1;

/// Chirich Ring +1 A/B and Fickblix's ring give 10 Subtle Blow each, Ninurta's Sash 6 and
/// Niqmaddu Ring 5 Subtle Blow II; Fickblix's ring also gives 10 Double Attack and the belts
/// 9 (Ioskeha), 5 (Sailfi) or 3 (Kentarch).
const RINGS: &[&str] = &["Epaminondas's Ring", "Sroda Ring", "Regal Ring", "Niqmaddu Ring",
    "Chirich Ring +1 A", "Chirich Ring +1 B", "Fickblix's ring"];
const WAISTS: &[&str] = &["Sailfi Belt +1", "Kentarch Belt +1", "Fotia Belt", "Ninurta's Sash", "Ioskeha Belt +1"];

struct Setup { main: Job, sub: Job, weapons: &'static [(Slot, &'static str)], ws: &'static str }

/// Over the Warrior's own 33 Double Attack the candidates reach at most 52.
const WARRIOR: Setup = Setup { main: Job::War, sub: Job::Sam,
    weapons: &[(Slot::Main, "Naegling"), (Slot::Sub, "Blurred Shield +1")], ws: "Savage Blade" };
/// Over the Monk's own 35 Subtle Blow the candidates reach 61 raw, but Subtle Blow counts only up
/// to 50: the best combined total is 55, from one Subtle Blow ring, Ninurta's Sash and Niqmaddu Ring.
const MONK: Setup = Setup { main: Job::Mnk, sub: Job::War, weapons: &[(Slot::Main, "Karambit")], ws: "Victory Smite" };

fn catalog() -> Arc<Catalog> {
    Arc::new(Catalog::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap())
}

/// The first `slot` piece named `name` (or with that variant name) that `job` can equip.
fn find_piece(catalog: &Catalog, job: Job, slot: Slot, name: &str) -> Option<GearId> {
    catalog.slot(slot).iter().copied().find(|&id| {
        let piece = catalog.piece(id);
        (piece.name2.as_ref() == name || piece.name.as_ref() == name) && piece.eligible_for(job)
    })
}

fn piece(catalog: &Catalog, job: Job, slot: Slot, name: &str) -> GearId {
    find_piece(catalog, job, slot, name).unwrap_or_else(|| panic!("{name} is not a {} {} piece", job.name(), slot.name()))
}

fn player_input(catalog: &Catalog, setup: &Setup) -> PlayerInput {
    let mut gearset = [catalog.empty; Slot::COUNT];
    for &(slot, name) in setup.weapons { gearset[slot.index()] = piece(catalog, setup.main, slot, name); }
    let mut abilities = Abilities::default();
    abilities.set_storm(Some(Arc::from("None")));
    PlayerInput { main_job: setup.main, sub_job: Some(setup.sub), master_level: 20, gearset,
        effects: Arc::new(aggregate_buffs(&BuffInputs::default(), catalog).unwrap()), abilities }
}

fn request(catalog: &Catalog, setup: &Setup, min_stats: [f64; MINIMUM_STATS.len()]) -> OptimizeRequest {
    let player = player_input(catalog, setup);
    let enemy = build_enemy(&catalog.enemy(catalog.enemy_menu()[0]).to_input(), &player.effects).unwrap();
    let mut candidates: CandidatePools = std::array::from_fn(|_| Vec::new());
    for (slot, names) in [(Slot::Ring1, RINGS), (Slot::Ring2, RINGS), (Slot::Waist, WAISTS)] {
        // The Subtle Blow and Double Attack pieces the tests need suit both jobs; some belts do not.
        candidates[slot.index()] = names.iter().filter_map(|name| find_piece(catalog, setup.main, slot, name)).collect();
    }
    OptimizeRequest { player, enemy, candidates,
        action: ActionRequest::Ws(WsRequest { skill: resolve_name(setup.ws).unwrap(), ws_type: optimizer_ws_type(setup.ws),
            tp: 2000.0, metric: Metric::new("Damage dealt") }),
        ws_name: Arc::from(setup.ws), spell_name: Arc::from("None"), min_pdt: 0.0, min_mdt: 0.0, min_stats,
        options: OptimizeOptions { seed: Some(11), workers: 1, ..OptimizeOptions::default() },
        similar_results: false, similar_percent: 0.0, verbose_swaps: false }
}

fn run(catalog: &Arc<Catalog>, request: &OptimizeRequest) -> OptimizeResult {
    let mut context = EvaluationContext::new(Arc::clone(catalog), RandomSources::from_seed(0));
    optimize(request, &mut context, &AtomicBool::new(false), &mut |_| {}).unwrap()
}

fn names(catalog: &Catalog, result: &OptimizeResult) -> Vec<String> {
    [Slot::Ring1, Slot::Ring2, Slot::Waist].map(|slot| catalog.piece(result.gearset[slot.index()]).name2.to_string()).to_vec()
}

fn minimums(required: &[(usize, f64)]) -> [f64; MINIMUM_STATS.len()] {
    let mut minimums = [0.0; MINIMUM_STATS.len()];
    for &(index, minimum) in required { minimums[index] = minimum; }
    minimums
}

#[test]
fn combined_subtle_blow_caps_each_channel_at_50_and_their_sum_at_75() {
    for (subtle_blow, subtle_blow_ii, combined) in [
        (30.0, 20.0, 50.0), (61.0, 0.0, 50.0), (61.0, 5.0, 55.0), (0.0, 60.0, 50.0),
        (45.0, 30.0, 75.0), (50.0, 30.0, 75.0), (60.0, 60.0, 75.0),
    ] {
        let mut stats = Stats::default();
        stats.set(Stat::SubtleBlow, subtle_blow);
        stats.set(Stat::SubtleBlowIi, subtle_blow_ii);
        assert_eq!(MinimumStat::CombinedSubtleBlow.total(&stats), combined, "Subtle Blow {subtle_blow}, Subtle Blow II {subtle_blow_ii}");
    }
}

#[test]
fn main_jobs_carry_their_innate_subtle_blow() {
    let catalog = catalog();
    let mut context = EvaluationContext::new(Arc::clone(&catalog), RandomSources::from_seed(0));
    let mut player = Player::new([catalog.empty; Slot::COUNT]);
    for (main, innate) in [(Job::Mnk, 35.0), (Job::Dnc, 33.0), (Job::Nin, 32.0), (Job::War, 0.0)] {
        let setup = Setup { main, sub: Job::Sam, weapons: &[], ws: "" };
        build_player(&player_input(&catalog, &setup), &mut context, &mut player).unwrap();
        assert_eq!(player.stats.get(Stat::SubtleBlow), innate, "{}/sam", main.name());
    }
}

#[test]
fn each_minimum_steers_the_search_to_a_satisfying_set() {
    let catalog = catalog();
    for (setup, index, minimum, needed) in [
        (&MONK, COMBINED_SUBTLE_BLOW, 55.0, &["Niqmaddu Ring", "Ninurta's Sash"][..]),
        (&WARRIOR, DOUBLE_ATTACK, 52.0, &["Fickblix's ring", "Ioskeha Belt +1"][..]),
    ] {
        let stat = MINIMUM_STATS[index].name();
        let free = run(&catalog, &request(&catalog, setup, minimums(&[])));
        assert!(free.feasible);
        assert!(free.stat_totals[index] < minimum, "the damage-only set {:?} already has {stat} {}", names(&catalog, &free), free.stat_totals[index]);
        let result = run(&catalog, &request(&catalog, setup, minimums(&[(index, minimum)])));
        let label = format!("{stat} {minimum}: {:?}", names(&catalog, &result));
        let player = result.player.as_ref().expect("a satisfying set is returned");
        assert_eq!(stat_totals(player), result.stat_totals, "{label}: reported totals are the returned player's");
        assert!(result.feasible && result.stat_totals[index] >= minimum, "{label} has {stat} {}", result.stat_totals[index]);
        assert!(result.warnings.is_empty(), "{label}: {:?}", result.warnings);
        assert!(needed.iter().all(|name| names(&catalog, &result).iter().any(|worn| worn == name)), "{label} needs {needed:?}");
    }
}

#[test]
fn unreachable_minimums_are_reported_infeasible() {
    let catalog = catalog();
    for (setup, required) in [(&MONK, (COMBINED_SUBTLE_BLOW, 56.0)), (&WARRIOR, (DOUBLE_ATTACK, 53.0))] {
        let result = run(&catalog, &request(&catalog, setup, minimums(&[required])));
        let label = format!("{required:?}: {:?}", names(&catalog, &result));
        assert!(!result.feasible, "{label}");
        assert!(matches!(result.termination, OptimizeTermination::UnchangedDtStages | OptimizeTermination::UnsatisfiedDt), "{label}");
        assert!(!result.warnings.is_empty(), "{label}: the failure is surfaced");
    }
}
