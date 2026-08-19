import dataclasses
import itertools
import unittest

import numpy as np

import gear
from create_player import create_enemy, create_player
from enemies import preset_enemies
from wsdist_types import GearPiece
from wsdist import (
    MAX_SWAP_SLOTS,
    EvalContext,
    OptimizerOptions,
    SequentialMapper,
    _dt_for_player,
    _dt_sum,
    _dt_vector,
    _intern_pieces,
    _is_valid_gearset,
    _jse_ear_names,
    _Neighborhood,
    _normalize_optimizer_options,
    _optimize_pass,
    _slot_combos,
    score_chunk,
)


def base_set() -> dict[str, GearPiece]:
    return {
        "main": gear.Heishi,
        "sub": gear.Empty,
        "ranged": gear.Empty,
        "ammo": gear.Empty,
        "head": gear.Empty,
        "neck": gear.Empty,
        "ear1": gear.Empty,
        "ear2": gear.Empty,
        "body": gear.Empty,
        "hands": gear.Empty,
        "ring1": gear.Empty,
        "ring2": gear.Empty,
        "back": gear.Empty,
        "waist": gear.Empty,
        "legs": gear.Empty,
        "feet": gear.Empty,
    }


class OptimizerHelperTest(unittest.TestCase):
    def valid(self, gearset: dict[str, GearPiece], action_type: str = "attack round", ws_name: str = "", spell_name: str = "") -> bool:
        return _is_valid_gearset(
            gearset,
            "nin",
            "war",
            action_type,
            ws_name,
            spell_name,
            {"Katana": ["Blade: Shun"], "Archery": ["Namas Arrow"], "Marksmanship": ["Last Stand"]},
            {"Blade: Metsu": "Kikoku"},
            _jse_ear_names(),
        )

    def test_rejects_duplicate_unique_jewelry(self) -> None:
        gearset = base_set()
        gearset["ring1"] = gear.Epona_Ring
        gearset["ring2"] = gear.Epona_Ring

        self.assertFalse(self.valid(gearset))

    def test_rejects_invalid_offhand_weapon_for_non_dual_wield_job(self) -> None:
        gearset = base_set()
        gearset["sub"] = gear.Crepuscular_Knife

        self.assertFalse(
            _is_valid_gearset(
                gearset,
                "war",
                "drg",
                "attack round",
                "",
                "",
                {},
                {},
                _jse_ear_names(),
            )
        )

    def test_rejects_ranged_ammo_mismatch(self) -> None:
        gearset = base_set()
        gearset["ranged"] = gear.Donar_Gun
        gearset["ammo"] = gear.Yoichi_Arrow

        self.assertFalse(self.valid(gearset))

    def test_rejects_impact_without_required_cloak(self) -> None:
        gearset = base_set()

        self.assertFalse(self.valid(gearset, action_type="spell cast", spell_name="Impact"))

    def test_accepts_impact_with_required_cloak_and_empty_head(self) -> None:
        gearset = base_set()
        gearset["body"] = gear.Twilight_Cloak

        self.assertTrue(self.valid(gearset, action_type="spell cast", spell_name="Impact"))

    def test_rejects_restricted_weapon_skill_without_required_weapon(self) -> None:
        gearset = base_set()

        self.assertFalse(self.valid(gearset, action_type="weapon skill", ws_name="Blade: Metsu"))

    def test_optimizer_options_are_bounded_and_seeded(self) -> None:
        options = _normalize_optimizer_options(
            OptimizerOptions(
                iterations=0,
                max_swap_slots=9,
                restart_count=0,
                seed=123,
                dt_step=0,
                workers=-3,
            )
        )

        self.assertEqual(options.iterations, 1)
        self.assertEqual(options.max_swap_slots, MAX_SWAP_SLOTS)
        self.assertEqual(options.restart_count, 1)
        self.assertEqual(options.seed, 123)
        self.assertEqual(options.dt_step, 1)
        self.assertEqual(options.workers, 0)


class SlotCombosTest(unittest.TestCase):
    def test_yields_every_subset_up_to_the_limit_exactly_once(self) -> None:
        combos = _slot_combos(5, 3)
        expected = {c for size in (1, 2, 3) for c in itertools.combinations(range(5), size)}

        self.assertEqual(len(combos), len(expected))
        self.assertEqual(set(combos), expected)

    def test_raising_the_limit_keeps_the_smaller_neighbourhood_order(self) -> None:
        pairs = _slot_combos(4, 2)
        triples = _slot_combos(4, 3)

        self.assertEqual([c for c in triples if len(c) <= 2], pairs)


class NeighborhoodTest(unittest.TestCase):
    #
    # Pools hold piece ids; id 0 stands for "already equipped" in slot 0 below.
    #
    def enumerate_all(self, combos, pools, base_ids, batch):
        np.random.seed(4)
        neighborhood = _Neighborhood(combos, pools)
        seen: list[tuple[tuple[int, ...], tuple[int, ...]]] = []
        while not neighborhood.exhausted():
            groups, total = neighborhood.take(base_ids, batch)
            if not total:
                break
            for combo, candidates in groups:
                seen.extend((combo, item_ids) for item_ids in candidates)
        return seen

    def test_enumerates_the_product_minus_candidates_that_change_nothing(self) -> None:
        pools = [[0, 1, 2], [3, 4], [5, 6]]
        base_ids = (0, 3, 6)
        combos = [(0,), (1,), (0, 1), (0, 2)]

        seen = self.enumerate_all(combos, pools, base_ids, 4)

        self.assertEqual(sorted(seen), sorted([
            ((0,), (1,)), ((0,), (2,)),
            ((1,), (4,)),
            ((0, 1), (1, 4)), ((0, 1), (2, 4)),
            ((0, 2), (1, 5)), ((0, 2), (2, 5)),
        ]))

    def test_batch_size_changes_nothing_about_what_is_enumerated(self) -> None:
        pools = [[0, 1, 2, 3], [4, 5, 6], [7, 8]]
        base_ids = (1, 4, 9)
        combos = _slot_combos(3, 3)

        reference = self.enumerate_all(combos, pools, base_ids, 1)
        self.assertTrue(reference)
        for batch in (2, 5, 17, 1000):
            self.assertEqual(self.enumerate_all(combos, pools, base_ids, batch), reference)

    def test_an_empty_pool_freezes_every_combination_that_uses_it(self) -> None:
        pools = [[0, 1], [], [2, 3]]
        base_ids = (9, 9, 9)

        seen = self.enumerate_all(_slot_combos(3, 2), pools, base_ids, 3)

        self.assertEqual({combo for combo, _ in seen}, {(0,), (2,), (0, 2)})


def melee_ws_context(slot_items: dict[str, list[GearPiece]]) -> tuple[EvalContext, tuple[int, ...], list[list[int]], list[int]]:
    #
    # A real evaluation context over a handful of swappable slots, small enough
    # to optimize in milliseconds.
    #
    enemy_stats = dict(preset_enemies["Apex Toad"])
    enemy = create_enemy(enemy_stats)
    enemy.stats["Base Defense"] = enemy_stats["Defense"]
    enemy.stats["Magic Damage Taken"] = enemy.stats.pop("Magic DT%")

    start = base_set()
    slots = tuple(start)
    check_gear = {slot: slot_items.get(slot, []) for slot in slots}
    pieces, piece_id = _intern_pieces([start.values(), *(check_gear[slot] for slot in slots)])
    ctx = EvalContext(
        main_job="nin", sub_job="war", master_level=50, buffs={}, abilities={}, enemy=enemy,
        ws_name="Blade: Shun", spell_name="", action_type="weapon skill", min_tp=1000,
        ws_type="melee", spell_type="Elemental Magic", input_metric="Damage dealt",
        ws_dict={"Katana": ["Blade: Shun"]}, restricted_ws={}, jse_ears=_jse_ear_names(),
        use_fast_dt_gate=True, slots=slots, pieces=tuple(pieces),
        dt_vectors=tuple(_dt_vector(piece.stats) for piece in pieces),
        buff_dt=_dt_sum([]),
    )
    base_ids = tuple(piece_id[id(start[slot])] for slot in slots)
    pools = [[piece_id[id(item)] for item in check_gear[slot]] for slot in slots]
    swappable = [index for index, slot in enumerate(slots) if pools[index]]
    return ctx, base_ids, pools, swappable


class FixedBatchMapper(SequentialMapper):
    def __init__(self, ctx: EvalContext, floor: int, ceiling: int) -> None:
        super().__init__(ctx)
        self.batch_floor = floor
        self.batch_ceiling = ceiling


class SpeculativeBatchingTest(unittest.TestCase):
    #
    # The optimizer scores candidates in batches against a frozen incumbent and
    # then replays the batch to find the first improvement. That is only a
    # legitimate optimization if the batch size cannot change the answer, which
    # is also what makes the worker pool safe: the pool is nothing more than a
    # bigger batch.
    #
    def sweep(self, batch: tuple[int, int]) -> tuple[tuple[int, ...], float, list[float]]:
        ctx, base_ids, pools, swappable = melee_ws_context({
            "head": [gear.Empty, gear.Malignance_Chapeau, gear.Hattori_Zukin2, gear.Ryuo_Somen_A],
            "hands": [gear.Empty, gear.Malignance_Gloves, gear.Hattori_Tekko2, gear.Ryuo_Tekko_A],
            "feet": [gear.Empty, gear.Malignance_Boots, gear.Hattori_Kyahan2, gear.Ryuo_SuneAte_C],
        })
        combos = [tuple(swappable[axis] for axis in combo) for combo in _slot_combos(len(swappable), 2)]
        swaps: dict[str, list[object]] = {slot: [] for slot in ctx.slots}

        np.random.seed(11)
        return _optimize_pass(
            ctx, FixedBatchMapper(ctx, *batch), _Neighborhood(combos, pools),
            base_ids, 0.0001, [0.0, 0.0, 0.0], swaps, 1, 200, 200, False, 1, 1, 8,
        )

    def test_batch_size_does_not_change_the_set_the_sweep_lands_on(self) -> None:
        reference_ids, reference_metric, _ = self.sweep((1, 1))

        # A batch bigger than the whole neighbourhood scores every candidate
        # against the starting set, so replay has to do all the work.
        for batch in ((8, 64), (256, 4096), (100000, 100000)):
            ids, metric, _ = self.sweep(batch)
            self.assertEqual(ids, reference_ids, f"batch={batch}")
            self.assertEqual(metric, reference_metric, f"batch={batch}")

    def test_the_sweep_actually_improves_on_the_starting_set(self) -> None:
        ids, metric, output = self.sweep((8, 64))

        self.assertGreater(metric, 0.0001)
        self.assertGreater(output[0], 0.0)
        self.assertNotEqual(ids, tuple())


class DamageTakenGateTest(unittest.TestCase):
    #
    # score_chunk screens candidates with a damage-taken figure summed straight
    # from gear and buffs, before paying for a create_player build. That shortcut
    # is only safe while it agrees with the build it is skipping, in both
    # directions: too strict and good sets vanish, too loose and it buys nothing.
    #
    def test_the_fast_gate_screens_exactly_what_the_full_build_would(self) -> None:
        ctx, base_ids, pools, _ = melee_ws_context({
            "body": [gear.Malignance_Tabard, gear.Hattori_Ningi2],
            "ring1": [gear.Defending_Ring, gear.Gelatinous_Ring],
        })
        unscreened = dataclasses.replace(ctx, use_fast_dt_gate=False)
        body, ring = ctx.slots.index("body"), ctx.slots.index("ring1")
        groups = [((body, ring), [(b, r) for b in pools[body] for r in pools[ring]])]

        screened_anything = False
        for pdt in range(-30, 5, 5):
            for mdt in (-20, 0, 20):
                fast = score_chunk(ctx, base_ids, groups, pdt, mdt)
                full = score_chunk(unscreened, base_ids, groups, pdt, mdt)
                self.assertEqual(
                    [m != m for m in fast], [m != m for m in full],
                    f"gate disagrees at PDT<={pdt} MDT<={mdt}",
                )
                screened_anything = screened_anything or any(m != m for m in full)

        self.assertTrue(screened_anything, "thresholds were too loose to exercise the gate")



if __name__ == "__main__":
    unittest.main()
