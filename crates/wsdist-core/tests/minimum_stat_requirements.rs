//! Minimum stat totals bound the optimizer's search; a zero minimum leaves its stat unconstrained.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use wsdist_core::model::{
    Abilities, ActionRequest, BuffInputs, CandidatePools, EvaluationContext, GearId, Job, MINIMUM_STATS, Metric,
    OptimizeOptions, OptimizeRequest, OptimizeResult, OptimizeTermination, PlayerInput, Slot, WsRequest,
};
use wsdist_core::optimizer::optimize;
use wsdist_core::optimizer::probe::{optimizer_ws_type, stat_totals};
use wsdist_core::rng::RandomSources;
use wsdist_core::weaponskills::resolve_name;
use wsdist_core::{Catalog, aggregate_buffs, build_enemy};

/// Indices into `MINIMUM_STATS`.
const SUBTLE_BLOW: usize = 0;
const SUBTLE_BLOW_II: usize = 1;
const DOUBLE_ATTACK: usize = 2;

/// Chirich Ring +1 A/B and Fickblix's ring give 10 Subtle Blow each, Ninurta's Sash 6 and
/// Niqmaddu Ring 5 Subtle Blow II: two rings and the sash reach at most 26 Subtle Blow.
/// Over the Warrior's own 33 Double Attack, Fickblix's ring adds 10 and the belts 9 (Ioskeha),
/// 5 (Sailfi) or 3 (Kentarch): at most 52.
const RINGS: &[&str] = &["Epaminondas's Ring", "Sroda Ring", "Regal Ring", "Niqmaddu Ring",
    "Chirich Ring +1 A", "Chirich Ring +1 B", "Fickblix's ring"];
const WAISTS: &[&str] = &["Sailfi Belt +1", "Kentarch Belt +1", "Fotia Belt", "Ninurta's Sash", "Ioskeha Belt +1"];

fn catalog() -> Arc<Catalog> {
    Arc::new(Catalog::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap())
}

/// The first `slot` piece named `name` (or with that variant name) that a Warrior can equip.
fn piece(catalog: &Catalog, slot: Slot, name: &str) -> GearId {
    catalog.slot(slot).iter().copied().find(|&id| {
        let piece = catalog.piece(id);
        (piece.name2.as_ref() == name || piece.name.as_ref() == name) && piece.eligible_for(Job::War)
    }).unwrap_or_else(|| panic!("{name} is not a Warrior {} piece", slot.name()))
}

fn request(catalog: &Catalog, min_stats: [f64; MINIMUM_STATS.len()]) -> OptimizeRequest {
    let mut gearset = [catalog.empty; Slot::COUNT];
    gearset[Slot::Main.index()] = piece(catalog, Slot::Main, "Naegling");
    gearset[Slot::Sub.index()] = piece(catalog, Slot::Sub, "Blurred Shield +1");
    let mut abilities = Abilities::default();
    abilities.set_storm(Some(Arc::from("None")));
    let player = PlayerInput { main_job: Job::War, sub_job: Some(Job::Sam), master_level: 20, gearset,
        effects: Arc::new(aggregate_buffs(&BuffInputs::default(), catalog).unwrap()), abilities };
    let enemy = build_enemy(&catalog.enemy(catalog.enemy_menu()[0]).to_input(), &player.effects).unwrap();
    let mut candidates: CandidatePools = std::array::from_fn(|_| Vec::new());
    for (slot, names) in [(Slot::Ring1, RINGS), (Slot::Ring2, RINGS), (Slot::Waist, WAISTS)] {
        candidates[slot.index()] = names.iter().map(|name| piece(catalog, slot, name)).collect();
    }
    let ws = "Savage Blade";
    OptimizeRequest { player, enemy, candidates,
        action: ActionRequest::Ws(WsRequest { skill: resolve_name(ws).unwrap(), ws_type: optimizer_ws_type(ws),
            tp: 2000.0, metric: Metric::new("Damage dealt") }),
        ws_name: Arc::from(ws), spell_name: Arc::from("None"), min_pdt: 0.0, min_mdt: 0.0, min_stats,
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
fn each_minimum_steers_the_search_to_a_satisfying_set() {
    let catalog = catalog();
    let free = run(&catalog, &request(&catalog, minimums(&[])));
    assert!(free.feasible);
    for (index, minimum, needed) in [
        (SUBTLE_BLOW, 26.0, &["Ninurta's Sash"][..]),
        (DOUBLE_ATTACK, 52.0, &["Fickblix's ring", "Ioskeha Belt +1"][..]),
    ] {
        let stat = MINIMUM_STATS[index].name();
        assert!(free.stat_totals[index] < minimum, "the damage-only set {:?} already has {stat} {}", names(&catalog, &free), free.stat_totals[index]);
        let result = run(&catalog, &request(&catalog, minimums(&[(index, minimum)])));
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
    for required in [&[(SUBTLE_BLOW, 30.0)][..], &[(SUBTLE_BLOW, 26.0), (SUBTLE_BLOW_II, 5.0)], &[(DOUBLE_ATTACK, 53.0)]] {
        let result = run(&catalog, &request(&catalog, minimums(required)));
        let label = format!("{required:?}: {:?}", names(&catalog, &result));
        assert!(!result.feasible, "{label}");
        assert!(matches!(result.termination, OptimizeTermination::UnchangedDtStages | OptimizeTermination::UnsatisfiedDt), "{label}");
        assert!(!result.warnings.is_empty(), "{label}: the failure is surfaced");
    }
}
