//! Subtle Blow minima bound the optimizer's search; zero leaves them unconstrained.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use wsdist_core::model::{
    Abilities, ActionRequest, BuffInputs, CandidatePools, EvaluationContext, GearId, Job, Metric, OptimizeOptions,
    OptimizeRequest, OptimizeResult, OptimizeTermination, PlayerInput, Slot, WsRequest,
};
use wsdist_core::optimizer::optimize;
use wsdist_core::optimizer::probe::{optimizer_ws_type, subtle_blow_for_player};
use wsdist_core::rng::RandomSources;
use wsdist_core::weaponskills::resolve_name;
use wsdist_core::{Catalog, aggregate_buffs, build_enemy};

/// Chirich Ring +1 A/B and Fickblix's ring give 10 Subtle Blow each, Ninurta's Sash 6 and
/// Niqmaddu Ring 5 Subtle Blow II: two rings and the sash reach at most 26 Subtle Blow.
const RINGS: &[&str] = &["Epaminondas's Ring", "Sroda Ring", "Regal Ring", "Niqmaddu Ring",
    "Chirich Ring +1 A", "Chirich Ring +1 B", "Fickblix's ring"];
const WAISTS: &[&str] = &["Sailfi Belt +1", "Kentarch Belt +1", "Fotia Belt", "Ninurta's Sash"];

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

fn request(catalog: &Catalog, min_subtle_blow: f64, min_subtle_blow_ii: f64) -> OptimizeRequest {
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
        ws_name: Arc::from(ws), spell_name: Arc::from("None"), min_pdt: 0.0, min_mdt: 0.0,
        min_subtle_blow, min_subtle_blow_ii,
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

#[test]
fn subtle_blow_minimum_steers_the_search_to_a_satisfying_set() {
    let catalog = catalog();
    let free = run(&catalog, &request(&catalog, 0.0, 0.0));
    assert!(free.feasible);
    assert!(free.subtle_blow < 26.0, "the damage-only set {:?} must not already meet the requirement", names(&catalog, &free));

    let result = run(&catalog, &request(&catalog, 26.0, 0.0));
    let player = result.player.as_ref().expect("a satisfying set is returned");
    assert_eq!(subtle_blow_for_player(player)[0], result.subtle_blow, "the reported total is the returned player's");
    assert!(result.feasible && result.subtle_blow >= 26.0, "{:?} has Subtle Blow {}", names(&catalog, &result), result.subtle_blow);
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    assert!(names(&catalog, &result).contains(&"Ninurta's Sash".to_owned()), "26 needs the sash: {:?}", names(&catalog, &result));
}

#[test]
fn unreachable_subtle_blow_minimum_is_reported_infeasible() {
    let catalog = catalog();
    for (min_subtle_blow, min_subtle_blow_ii) in [(30.0, 0.0), (26.0, 5.0)] {
        let result = run(&catalog, &request(&catalog, min_subtle_blow, min_subtle_blow_ii));
        let label = format!("Subtle Blow {min_subtle_blow}, II {min_subtle_blow_ii}: {:?}", names(&catalog, &result));
        assert!(!result.feasible, "{label}");
        assert!(matches!(result.termination, OptimizeTermination::UnchangedDtStages | OptimizeTermination::UnsatisfiedDt), "{label}");
        assert!(!result.warnings.is_empty(), "{label}: the failure is surfaced");
    }
}
