'''
File containing algorithm to automatically build and test gear sets for set finding.

Hill climbing over gear swaps: each pass walks combinations of up to
OptimizerOptions.max_swap_slots slots and takes the first swap that beats the
current set, repeating until a pass finds nothing.

Scoring a candidate set costs a full create_player build plus a damage
calculation, and a pass tests hundreds of thousands of them, so candidates are
scored in speculative batches that can be spread over worker processes. A batch
is scored against a frozen set and then replayed in order, which reproduces the
one-at-a-time scan exactly: the number of workers changes how long a search
takes, never what it finds.

Each WS set typically has one deep global minima that this algorithm finds relatively well.

Critical hit weapon skills (and those with Shining One equipped) can have two minima (one Crit build, one WS damage build). 
This algorithm may get caught in a crit build if starting from a crit build, but this only affects crit WSs.
    
Author: Kastra (Asura server)
'''
import itertools
import math
import os
import sys
from datetime import datetime # For timestamping new sets to put on BG Wiki
from dataclasses import dataclass

from create_player import *
from actions import *

from wsdist_types import Buffs, GearPiece, Gearset

# Use an external gear.py file
# https://stackoverflow.com/questions/47350078/importing-external-module-in-single-file-exe-created-with-pyinstaller
sys.path.append(os.path.dirname(sys.executable))
from gear import *

# Imported last so these names are not shadowed by the wildcard imports above.
from collections.abc import Iterable, Mapping
from typing import Any, Protocol, cast

import numpy as np


@dataclass(frozen=True)
class OptimizerOptions:
    iterations: int = 10
    max_swap_slots: int = 2
    restart_count: int = 1
    seed: int | None = None
    dt_step: int = 1
    # Worker processes used to score candidate sets. 0 picks a sensible default
    # from the CPU count, 1 keeps everything in this process.
    workers: int = 0


# Above this many candidate evaluations per pass, spinning up a worker pool pays
# for itself; below it the pool startup dominates and we stay single-process.
PARALLEL_CANDIDATE_THRESHOLD = 20_000

# Largest neighbourhood the optimizer will enumerate. Four-slot moves are only
# reachable with a worker pool; see the candidate-count guard in build_set.
MAX_SWAP_SLOTS = 4

# Speculative batches grow by this factor for every batch that finds no
# improvement, and shrink by it whenever one does. Growing amortizes the cost of
# a dispatch round; shrinking bounds the work thrown away when the incumbent
# moves out from under a batch that was already in flight.
BATCH_GROWTH = 4
BATCH_SHRINK = 4


def _normalize_optimizer_options(options: OptimizerOptions | None) -> OptimizerOptions:
    if options is None:
        options = OptimizerOptions()
    return OptimizerOptions(
        iterations=max(1, options.iterations),
        max_swap_slots=min(MAX_SWAP_SLOTS, max(1, options.max_swap_slots)),
        restart_count=max(1, options.restart_count),
        seed=options.seed,
        dt_step=max(1, options.dt_step),
        workers=max(0, options.workers),
    )


def _jse_ear_names() -> list[str]:
    names = ["Hattori", "Heathen's", "Lethargy", "Ebers", "Wicce", "Peltast's", "Boii", "Bhikku", "Skulker's", "Chevalier's", "Nukumi", "Fili", "Amini", "Kasuga", "Beckoner's", "Hashishin", "Chasseur's", "Karagoz", "Maculele", "Arbatel", "Azimuth", "Erilaz"]
    return [k + " Earring +1" for k in names] + [k + " Earring +2" for k in names]


def _copy_gearset(gearset: Gearset) -> Gearset:
    return {slot: piece for slot, piece in gearset.items()}


def _prefilter_check_gear(check_gear: dict[str, Any], main_job: str) -> dict[str, list[GearPiece]]:
    return {
        slot: [
            item
            for item in cast("list[GearPiece]", items)
            if main_job in item.jobs
        ]
        for slot, items in check_gear.items()
    }


# Damage-taken stats, in the order used by every DT vector below.
DT_STAT_KEYS = ("PDT", "MDT", "DT", "PDT2", "MDT2", "DT2")

# Per-piece / per-buff damage-taken contribution, aligned to DT_STAT_KEYS.
DTVector = tuple[float, float, float, float, float, float]


def _dt_vector(stats: Mapping[str, float]) -> DTVector:
    return (
        float(stats.get("PDT", 0)), float(stats.get("MDT", 0)), float(stats.get("DT", 0)),
        float(stats.get("PDT2", 0)), float(stats.get("MDT2", 0)), float(stats.get("DT2", 0)),
    )


def _dt_totals(pdt: float, mdt: float, dt: float, pdt2: float, mdt2: float, dt2: float) -> tuple[float, float]:
    return max(-50.0, pdt + dt) + pdt2 + dt2, max(-50.0, mdt + dt) + mdt2 + dt2


def _dt_sum(vectors: Iterable[DTVector]) -> DTVector:
    total = [0.0] * 6
    for vector in vectors:
        for i in range(6):
            total[i] += vector[i]
    return cast("DTVector", tuple(total))


def _dt_for_player(player: "create_player") -> tuple[float, float]:
    pdt = player.stats.get("PDT", 0) + player.stats.get("DT", 0)
    mdt = player.stats.get("MDT", 0) + player.stats.get("DT", 0)
    pdt = -50 if pdt < -50 else pdt
    mdt = -50 if mdt < -50 else mdt
    pdt += player.stats.get("PDT2", 0) + player.stats.get("DT2", 0)
    mdt += player.stats.get("MDT2", 0) + player.stats.get("DT2", 0)
    return pdt, mdt


def _can_use_fast_dt_gate(abilities: dict[str, Any]) -> bool:
    return int(abilities.get("Aftermath", 0) or 0) == 0


def _slot_combos(slot_count: int, max_swap_slots: int) -> list[tuple[int, ...]]:
    #
    # Every distinct set of at most <max_swap_slots> slots to swap together, as
    # indices into the caller's slot ordering. Ordered by the lexicographic walk
    # over combinations-with-replacement, so raising max_swap_slots keeps the
    # smaller neighbourhoods in the order they were visited before and appends
    # the larger ones behind them.
    #
    seen: set[tuple[int, ...]] = set()
    combos: list[tuple[int, ...]] = []
    for padded in itertools.combinations_with_replacement(range(slot_count), max_swap_slots):
        combo = tuple(sorted(set(padded)))
        if combo in seen:
            continue
        seen.add(combo)
        combos.append(combo)
    return combos


def _is_valid_gearset(test_set: Gearset, main_job: str, sub_job: str, action_type: str, ws_name: str, spell_name: str, ws_dict: dict[str, list[str]], restricted_ws: dict[str, str], jse_ears: list[str]) -> bool:
    if (test_set["ring1"] == test_set["ring2"]) and (test_set["ring1"].name != "Empty"):
        return False
    if (test_set["ear1"] == test_set["ear2"]) and (test_set["ear1"].name != "Empty"):
        return False
    if (test_set["main"] == test_set["sub"]) and (test_set["main"].name != "Empty"):
        return False

    one_handed = ["Axe", "Club", "Dagger", "Sword", "Katana"]
    if (test_set["main"].skill_type in one_handed) and (test_set["sub"].type == "Grip"):
        return False

    two_handed = ["Great Sword", "Great Katana", "Great Axe", "Polearm", "Scythe", "Staff"]
    if (test_set["main"].skill_type in two_handed) and (test_set["sub"].type == "Weapon" or test_set["sub"].type == "Shield"):
        return False

    if (test_set["main"].skill_type == "Hand-to-Hand") and (test_set["sub"].name != "Empty"):
        return False

    archery = ["Empyreal Arrow", "Flaming Arrow", "Namas Arrow", "Jishnu's Radiance", "Apex Arrow", "Refulgent Arrow", "Sidewinder", "Blast Arrow", "Piercing Arrow"]
    marksmanship = ["Last Stand", "Hot Shot", "Leaden Salute", "Wildfire", "Coronach", "Trueflight", "Detonator", "Blast Shot", "Slug Shot", "Split Shot"]
    if (action_type == "weapon skill") and (ws_name in archery + marksmanship):
        if (ws_name in archery) and (test_set["ranged"].skill_type != "Archery" or test_set["ammo"].type != "Arrow"):
            return False
        if (ws_name in marksmanship) and (test_set["ranged"].skill_type != "Marksmanship" or test_set["ammo"].type not in ["Bolt", "Bullet"]):
            return False
        if (test_set["ranged"].type == "Crossbow") and (test_set["ammo"].type != "Bolt"):
            return False
        if (test_set["ranged"].type == "Gun") and (test_set["ammo"].type != "Bullet"):
            return False

    if (action_type == "spell cast") and (spell_name == "Ranged Attack"):
        if (test_set["ranged"].type not in ["Gun", "Bow", "Crossbow"]) or (test_set["ammo"].type not in ["Bullet", "Arrow", "Bolt"]):
            return False

    if (test_set["ranged"].type == "Gun") and (test_set["ammo"].type not in ["Bullet", "None"]):
        return False
    if (test_set["ranged"].type == "Bow") and (test_set["ammo"].type not in ["Arrow", "None"]):
        return False
    if (test_set["ranged"].type == "Crossbow") and (test_set["ammo"].type not in ["Bolt", "None"]):
        return False

    if (test_set["ammo"].type == "Bullet") and (test_set["ranged"].type != "Gun"):
        return False
    if (test_set["ammo"].type == "Arrow") and (test_set["ranged"].type != "Bow"):
        return False
    if (test_set["ammo"].type == "Bolt") and (test_set["ranged"].type != "Crossbow"):
        return False

    if (test_set["ranged"].type == "Instrument") and (test_set["ammo"].type != "None"):
        return False

    if (main_job not in ["nin", "dnc", "thf", "blu"] and sub_job not in ["nin", "dnc"]) and (test_set["sub"].type == "Weapon"):
        return False

    if (test_set["ear1"].name in jse_ears) and (test_set["ear2"].name == "Balder Earring +1"):
        return False
    if (test_set["ear2"].name in jse_ears) and (test_set["ear1"].name == "Balder Earring +1"):
        return False

    if (test_set["body"].name in ["Cohort Cloak", "Cohort Cloak +1", "Crepuscular Cloak", "Twilight Cloak"]) and (test_set["head"].name != "Empty"):
        return False

    if action_type == "spell cast":
        if (spell_name == "Impact") and (test_set["body"].name not in ["Crepuscular Cloak", "Twilight Cloak"]):
            return False

    if action_type == "weapon skill":
        if ws_name in restricted_ws:
            if (restricted_ws[ws_name] != test_set["main"].name) and (restricted_ws[ws_name] != test_set["ranged"].name):
                return False

        ws_on_main = ws_name in ws_dict.get(test_set["main"].skill_type, [])
        ws_on_ranged = ws_name in ws_dict.get(test_set["ranged"].skill_type, [])
        if (not ws_on_main) and (not ws_on_ranged):
            return False

    return True


# Printed width of a metric per action, as (decimals, total digits).
METRIC_FORMAT = {"weapon skill": (1, 8), "spell cast": (1, 8), "attack round": (3, 8)}


def _evaluate_metric(player: "create_player", enemy: "create_enemy", ws_name: str, spell_name: str, action_type: str, min_tp: float, ws_type: str, spell_type: str, input_metric: str) -> tuple[float, list[Any], int, int, int]:
    if action_type == "weapon skill":
        metric_base, output = average_ws(player, enemy, ws_name, min_tp, ws_type, input_metric)
    elif action_type == "spell cast":
        metric_base, output = cast_spell(player, enemy, spell_name, spell_type, input_metric)
    elif action_type == "attack round":
        metric_base, output, _ = average_attack_round(player, enemy, 0, min_tp, input_metric)
    else:
        raise ValueError(f"Unknown action_type ({action_type})")

    decimals, nondecimals = METRIC_FORMAT[action_type]
    invert = output[-1]
    metric = metric_base**invert

    metric = 0.0001 if metric <= 0 else metric
    return metric, output, invert, decimals, nondecimals


# Candidates are batched by slot combination: one group names the slots being
# swapped (indices into EvalContext.slots) and lists the piece ids to try in
# them, one tuple per candidate. Grouping keeps the enumeration in tight inner
# loops and keeps worker payloads to one small int tuple per candidate.
Group = tuple[tuple[int, ...], list[tuple[int, ...]]]

# Metric sentinel for candidates rejected before they were ever scored (illegal
# combination, or damage taken above the current requirement).
REJECTED = float("nan")


@dataclass(frozen=True)
class EvalContext:
    #
    # Everything scoring a candidate needs, and nothing that changes during a
    # search. Frozen and picklable so worker processes score candidates with
    # byte-identical inputs; candidates themselves travel as small int tuples
    # indexing <slots> and <pieces>.
    #
    main_job: str
    sub_job: str
    master_level: int
    buffs: Buffs
    abilities: dict[str, Any]
    enemy: Any
    ws_name: str
    spell_name: str
    action_type: str
    min_tp: float
    ws_type: str
    spell_type: str
    input_metric: str
    ws_dict: dict[str, list[str]]
    restricted_ws: dict[str, str]
    jse_ears: list[str]
    use_fast_dt_gate: bool
    slots: tuple[str, ...]
    pieces: tuple[GearPiece, ...]
    dt_vectors: tuple[DTVector, ...]
    buff_dt: DTVector

    def gearset(self, ids: tuple[int, ...]) -> Gearset:
        return {slot: self.pieces[pid] for slot, pid in zip(self.slots, ids)}


def _intern_pieces(groups: Iterable[Iterable[GearPiece]]) -> tuple[list[GearPiece], dict[int, int]]:
    #
    # Assign every reachable gear piece a small integer id, collapsing pieces
    # that compare equal onto one id. Candidates then reference gear by id, which
    # makes "is this already equipped?" an int compare and keeps worker payloads
    # to a few bytes per candidate.
    #
    # Returns the id -> piece table and an id(object) -> piece id lookup. The
    # caller must keep the source pieces alive while using the lookup.
    #
    pieces: list[GearPiece] = []
    by_identity: dict[int, int] = {}
    by_value: dict[Any, int] = {}
    for group in groups:
        for piece in group:
            if id(piece) in by_identity:
                continue
            key = (piece.name, piece.name2, piece.type, piece.skill_type, piece.dmg,
                   piece.delay, piece.rank, piece.wsc, tuple(piece.jobs),
                   tuple(sorted(piece.stats.items())))
            pid = by_value.get(key)
            if pid is None:
                pid = len(pieces)
                by_value[key] = pid
                pieces.append(piece)
            by_identity[id(piece)] = pid
    return pieces, by_identity


def score_chunk(ctx: EvalContext, base_ids: tuple[int, ...], groups: list[Group], pdt_thresh: float, mdt_thresh: float) -> list[float]:
    #
    # Score every candidate in every group against one frozen incumbent,
    # returning one metric (or REJECTED) per candidate in enumeration order.
    #
    # This is the entire per-candidate cost of the optimizer and the only thing
    # worker processes run. Work that depends on the incumbent alone is hoisted
    # out of the loop, and work that depends only on which slots change is
    # hoisted to the group, so a candidate pays for its own pieces and nothing
    # else.
    #
    slots = ctx.slots
    pieces = ctx.pieces
    dt_vectors = ctx.dt_vectors
    base_set = ctx.gearset(base_ids)
    base_dt = _dt_sum([ctx.buff_dt, *(dt_vectors[pid] for pid in base_ids)])

    metrics: list[float] = []
    for combo, candidates in groups:
        combo_slots = [slots[slot_index] for slot_index in combo]
        vacated = _dt_sum(dt_vectors[base_ids[slot_index]] for slot_index in combo)
        stripped = [base_dt[i] - vacated[i] for i in range(6)]

        for item_ids in candidates:
            test_set = dict(base_set)
            for axis, pid in enumerate(item_ids):
                test_set[combo_slots[axis]] = pieces[pid]

            if not _is_valid_gearset(test_set, ctx.main_job, ctx.sub_job, ctx.action_type, ctx.ws_name, ctx.spell_name, ctx.ws_dict, ctx.restricted_ws, ctx.jse_ears):
                metrics.append(REJECTED)
                continue

            if ctx.use_fast_dt_gate:
                dt = list(stripped)
                for pid in item_ids:
                    added = dt_vectors[pid]
                    for i in range(6):
                        dt[i] += added[i]
                pdt, mdt = _dt_totals(*dt)
                if pdt > pdt_thresh or mdt > mdt_thresh:
                    metrics.append(REJECTED)
                    continue

            player = create_player(ctx.main_job, ctx.sub_job, ctx.master_level, test_set, ctx.buffs, ctx.abilities)
            pdt, mdt = _dt_for_player(player)
            if pdt > pdt_thresh or mdt > mdt_thresh:
                metrics.append(REJECTED)
                continue

            metric, _, _, _, _ = _evaluate_metric(player, ctx.enemy, ctx.ws_name, ctx.spell_name, ctx.action_type, ctx.min_tp, ctx.ws_type, ctx.spell_type, ctx.input_metric)
            metrics.append(metric)
    return metrics


def split_groups(groups: list[Group], total: int, parts: int) -> list[list[Group]]:
    #
    # Cut a batch into <parts> shares of roughly equal candidate count, splitting
    # a group across shares when it is larger than one share.
    #
    target = max(1, -(-total // parts))
    shares: list[list[Group]] = []
    share: list[Group] = []
    filled = 0
    for combo, candidates in groups:
        start = 0
        while start < len(candidates):
            take = min(target - filled, len(candidates) - start)
            share.append((combo, candidates[start:start + take]))
            filled += take
            start += take
            if filled >= target:
                shares.append(share)
                share, filled = [], 0
    if share:
        shares.append(share)
    return shares


class Mapper(Protocol):
    #
    # Scores a batch of candidates. The sequential and process-pool backends are
    # interchangeable: both call score_chunk with the same context, so a run
    # produces identical results whichever is used.
    #
    # <batch_floor> and <batch_ceiling> bound the speculative batch the driver
    # builds: they belong to the backend because the right batch size is set by
    # what a dispatch costs, which is nothing in-process and a pipe round trip
    # across a pool.
    #
    width: int
    batch_floor: int
    batch_ceiling: int

    def score(self, base_ids: tuple[int, ...], groups: list[Group], total: int, pdt_thresh: float, mdt_thresh: float) -> list[float]: ...

    def close(self) -> None: ...


class SequentialMapper:
    #
    # In-process scoring. Used when the neighbourhood is small or the user asked
    # for a single worker. Batches exist only to amortize the per-batch setup in
    # score_chunk, so they stay small and waste nothing after an improvement.
    #
    width = 1
    batch_floor = 8
    batch_ceiling = 64

    def __init__(self, ctx: EvalContext) -> None:
        self._ctx = ctx

    def score(self, base_ids: tuple[int, ...], groups: list[Group], total: int, pdt_thresh: float, mdt_thresh: float) -> list[float]:
        return score_chunk(self._ctx, base_ids, groups, pdt_thresh, mdt_thresh)

    def close(self) -> None:
        pass


class _Neighborhood:
    #
    # Lazily enumerates candidate moves for one optimizer pass, in a fixed order:
    # slot combinations outermost, then the product of the item lists of the
    # slots in that combination.
    #
    # The cursor is resumable. The driver hands out batches against a frozen
    # incumbent; when a batch turns out to contain an improvement it rewinds to
    # the position just after that candidate and carries on against the new
    # incumbent. Everything before the improvement was scored against the same
    # incumbent the sequential scan would have used, so the result is exactly
    # what a one-candidate-at-a-time first-improvement scan produces.
    #
    # Each combination draws its item order once per pass and keeps it, so a
    # rewind never reshuffles and the random stream does not depend on how far
    # any speculative batch ran ahead.
    #
    def __init__(self, combos: list[tuple[int, ...]], pools: list[list[int]]) -> None:
        self._combos = combos
        self._pools = pools          # slot index -> candidate piece ids
        self._orders: dict[int, list[list[int]]] = {}
        self._combo_index = 0
        self._odometer: list[int] = []
        self._entered = False

    def position(self) -> tuple[int, tuple[int, ...], bool]:
        return (self._combo_index, tuple(self._odometer), self._entered)

    def seek(self, position: tuple[int, tuple[int, ...], bool]) -> None:
        self._combo_index, odometer, self._entered = position
        self._odometer = list(odometer)

    def exhausted(self) -> bool:
        return self._combo_index >= len(self._combos)

    def _order(self, combo_index: int) -> list[list[int]]:
        order = self._orders.get(combo_index)
        if order is None:
            order = [list(self._pools[slot_index]) for slot_index in self._combos[combo_index]]
            for pool in order:
                np.random.shuffle(pool)
            self._orders[combo_index] = order
        return order

    def take(self, base_ids: tuple[int, ...], limit: int) -> tuple[list[Group], int]:
        #
        # Up to <limit> candidates that actually change the incumbent, grouped by
        # slot combination, plus how many there are.
        #
        # The inner axis is walked directly and the outer axes carry like an
        # odometer, so per candidate this costs one integer compare and one tuple
        # build. That matters: with many workers this enumeration, not the
        # scoring, is what the whole search waits on.
        #
        groups: list[Group] = []
        taken = 0
        while taken < limit and self._combo_index < len(self._combos):
            combo = self._combos[self._combo_index]
            pools = self._order(self._combo_index)
            if not all(pools):
                self._combo_index += 1
                self._entered = False
                continue
            if not self._entered:
                self._odometer = [0] * len(combo)
                self._entered = True

            odometer = self._odometer
            inner_axis = len(combo) - 1
            inner_pool = pools[inner_axis]
            inner_equipped = base_ids[combo[inner_axis]]
            candidates: list[tuple[int, ...]] = []

            while self._entered and taken < limit:
                prefix: tuple[int, ...] = ()
                for axis in range(inner_axis):
                    pid = pools[axis][odometer[axis]]
                    if pid == base_ids[combo[axis]]:
                        prefix = () # This row keeps an equipped piece; skip the whole row.
                        break
                    prefix += (pid,)
                else:
                    index = odometer[inner_axis]
                    while index < len(inner_pool) and taken < limit:
                        pid = inner_pool[index]
                        index += 1
                        if pid == inner_equipped:
                            continue
                        candidates.append(prefix + (pid,))
                        taken += 1
                    odometer[inner_axis] = index
                    if index < len(inner_pool):
                        break # Hit the batch limit part-way through the inner axis.

                odometer[inner_axis] = 0
                axis = inner_axis - 1
                while axis >= 0:
                    odometer[axis] += 1
                    if odometer[axis] < len(pools[axis]):
                        break
                    odometer[axis] = 0
                    axis -= 1
                if axis < 0:
                    self._combo_index += 1
                    self._entered = False

            if candidates:
                groups.append((combo, candidates))
        return groups, taken


def _score_with_output(ctx: EvalContext, ids: tuple[int, ...]) -> tuple[float, list[Any], int, int, int]:
    #
    # Full evaluation of one set, including the display output that score_chunk
    # throws away. Only used for the incumbent and for accepted moves, so the
    # duplicated work is a few evaluations per run.
    #
    player = create_player(ctx.main_job, ctx.sub_job, ctx.master_level, ctx.gearset(ids), ctx.buffs, ctx.abilities)
    return _evaluate_metric(player, ctx.enemy, ctx.ws_name, ctx.spell_name, ctx.action_type, ctx.min_tp, ctx.ws_type, ctx.spell_type, ctx.input_metric)


def _describe_swap(ctx: EvalContext, base_ids: tuple[int, ...], combo: tuple[int, ...], item_ids: tuple[int, ...]) -> tuple[str, str, str]:
    return (
        " & ".join(ctx.slots[slot_index] for slot_index in combo),
        " & ".join(ctx.pieces[base_ids[slot_index]].name2 for slot_index in combo),
        " & ".join(ctx.pieces[pid].name2 for pid in item_ids),
    )


def _resolve_workers(options: OptimizerOptions) -> int:
    if options.workers:
        return options.workers
    if hasattr(os, "sched_getaffinity"):
        return max(1, len(os.sched_getaffinity(0)))
    return max(1, os.cpu_count() or 1)


def _make_mapper(ctx: EvalContext, base_ids: tuple[int, ...], workers: int, candidates: int) -> Mapper:
    #
    # A worker pool only pays for its startup once the neighbourhood is large,
    # so small searches stay in this process. If the pool cannot be created at
    # all (frozen build, restricted platform) the search still runs, serially.
    #
    if workers <= 1 or candidates < PARALLEL_CANDIDATE_THRESHOLD:
        return SequentialMapper(ctx)
    try:
        import wsdist_parallel
        mapper = wsdist_parallel.PoolMapper(ctx, base_ids, workers)
    except Exception as error:
        print(f"Parallel evaluation unavailable ({error}); scoring in a single process.")
        return SequentialMapper(ctx)
    print(f"Scoring ~{candidates} candidates per pass across {workers} worker processes.")
    return mapper


def _optimize_pass(ctx: EvalContext, mapper: Mapper, neighborhood: _Neighborhood, best_ids: tuple[int, ...], best_metric: float, best_output: list[Any], swaps: dict[str, list[Any]], next_best_percent: float, pdt_thresh: float, mdt_thresh: float, verbose_swaps: bool, invert: int, decimals: int, nondecimals: int) -> tuple[tuple[int, ...], float, list[Any]]:
    #
    # One full first-improvement sweep of the neighbourhood.
    #
    # Candidates are scored in speculative batches against a frozen incumbent and
    # then replayed in enumeration order. Everything ahead of the first
    # improvement saw the same incumbent a strictly sequential scan would have
    # used, so replay reproduces that scan exactly; the rest of the batch is
    # discarded and enumeration resumes right after the accepted candidate.
    # Batches grow while nothing improves and shrink on every accept, which keeps
    # the discarded work to a few percent even at high worker counts.
    #
    batch_floor = mapper.batch_floor
    batch_ceiling = mapper.batch_ceiling
    batch = batch_floor
    while not neighborhood.exhausted():
        start = neighborhood.position()
        groups, total = neighborhood.take(best_ids, batch)
        if not total:
            break

        metrics = mapper.score(best_ids, groups, total, pdt_thresh, mdt_thresh)
        scanned = 0
        accepted: tuple[tuple[int, ...], tuple[int, ...], float] | None = None
        for combo, candidates in groups:
            for item_ids in candidates:
                metric = metrics[scanned]
                scanned += 1

                if metric != metric: # REJECTED: illegal set, or too much damage taken.
                    continue

                if metric > best_metric:
                    accepted = (combo, item_ids, metric)
                    break

                if len(combo) == 1:
                    slot = ctx.slots[combo[0]]
                    try:
                        if (best_metric%metric / best_metric < (float(next_best_percent)/100)) and (slot not in ["main","sub","ranged","back"]):
                            swaps[slot].append([ctx.pieces[item_ids[0]].name2, metric**invert])
                    except Exception:
                        # print(f"Error on \"{ctx.pieces[item_ids[0]].name2}\" - Metric = {metric}  - Best Metric = {best_metric}")
                        pass
            if accepted is not None:
                break

        if accepted is None:
            batch = min(batch_ceiling, batch*BATCH_GROWTH)
            continue

        combo, item_ids, metric = accepted
        if verbose_swaps:
            where, before, after = _describe_swap(ctx, best_ids, combo, item_ids)
            print(f"[{where:<15s}]: [{before}] -> [{after}]  [{best_metric**invert:>{nondecimals}.{decimals}f} -> {metric**invert:>{nondecimals}.{decimals}f}]")

        # Rewind to just after the accepted candidate. Replaying enumeration
        # against the incumbent the batch was built with lands on exactly the
        # position a sequential scan would be in; scoring resumes from there
        # against the new incumbent.
        neighborhood.seek(start)
        neighborhood.take(best_ids, scanned)

        swapped = list(best_ids)
        for axis, slot_index in enumerate(combo):
            swapped[slot_index] = item_ids[axis]
        best_ids = tuple(swapped)
        best_metric = metric
        _, best_output, _, _, _ = _score_with_output(ctx, best_ids)
        batch = max(batch_floor, batch//BATCH_SHRINK)

    return best_ids, best_metric, best_output


def _single_start_options(options: OptimizerOptions, restart_index: int) -> OptimizerOptions:
    return OptimizerOptions(
        iterations=options.iterations,
        max_swap_slots=options.max_swap_slots,
        restart_count=1,
        seed=None if options.seed is None else options.seed + restart_index,
        dt_step=options.dt_step,
        workers=options.workers,
    )


def _copy_check_gear(check_gear: dict[str, Any]) -> dict[str, list[GearPiece]]:
    return {slot: list(cast("list[GearPiece]", items)) for slot, items in check_gear.items()}

def format_bgwiki(ws_name: str, tp: float, player: "create_player", best_metric: Any) -> None:
    #
    # Input: A player class containing job and gear info.
    # Output: None
    #
    # Prints to the terminal the player gearset in BG Wiki format, ignoring augments.
    #
    buffs = "High"


    # Certain items have shortened names on BG Wiki. Use the item_list.txt file to find and replace these names for BG Wiki.
    item_list = np.loadtxt("item_list.csv", unpack=False, dtype=str, delimiter=';', usecols=(1,2), skiprows=1)
    name_map = {k[0].lower():k[1] for k in item_list}

    backaugs: list[str] = []
    for stat in player.gearset["back"].stats:
        if stat.lower() in ["str","dex","vit","agi","int","mnd","chr","da","store tp","dual wield","crit rate","weapon skill damage", "magic attack"]:
            backaugs.append(stat)

    linosaugs: list[str] = []
    for stat in player.gearset["ranged"].stats:
        if stat.lower() in ["str","dex","vit","agi","int","mnd","chr","da","store tp","dual wield","crit rate","weapon skill damage", "magic attack","qa","da","ta"]:
            linosaugs.append(stat)

    # Moonshade natually looks best in the left ear slot.
    if "moonshade" in player.gearset["ear2"].name.lower():
        ear2 = player.gearset["ear2"]
        ear1 = player.gearset["ear1"]
        player.gearset["ear1"] = ear2
        player.gearset["ear2"] = ear1

    # JSE earrings work in the right ear slot
    jse_ears1 = [k + " Earring +1" for k in ["Hattori", "Heathen's", "Lethargy", "Ebers", "Wicce", "Peltast's", "Boii", "Bhikku", "Skulker's", "Chevalier's", "Nukumi", "Fili", "Amini", "Kasuga", "Beckoner's", "Hashishin", "Chasseur's", "Karagoz", "Maculele", "Arbatel", "Azimuth", "Erilaz"]]
    jse_ears2 = [k + " Earring +2" for k in ["Hattori", "Heathen's", "Lethargy", "Ebers", "Wicce", "Peltast's", "Boii", "Bhikku", "Skulker's", "Chevalier's", "Nukumi", "Fili", "Amini", "Kasuga", "Beckoner's", "Hashishin", "Chasseur's", "Karagoz", "Maculele", "Arbatel", "Azimuth", "Erilaz"]]
    if player.gearset["ear1"].name2 in jse_ears1 or player.gearset["ear1"].name2 in jse_ears2:
        ear2 = player.gearset["ear2"]
        ear1 = player.gearset["ear1"]
        player.gearset["ear1"] = ear2
        player.gearset["ear2"] = ear1

    # Do it again because the above doesn't always work??
    empy = ["Hattori", "Heathen", "Lethargy", "Eber", "Wicce", "Peltast", "Boii", "Bhikku", "Skulker", "Chevalier", "Nukumi", "Fili", "Amini", "Kasuga", "Beckoner", "Hashishin", "Chasseur", "Karagoz", "Maculele", "Arbatel", "Azimuth", "Erilaz"]
    for name in empy:
        if name.lower() in player.gearset["ear1"].name.lower():
            ear2 = player.gearset["ear2"]
            ear1 = player.gearset["ear1"]
            player.gearset["ear1"] = ear2
            player.gearset["ear2"] = ear1

    # Epami looks best in the left ring slot, but only if sroda is not also equipped.
    if "epami" in player.gearset["ring2"].name.lower():
        if "sroda" not in player.gearset["ring1"].name.lower():
            ring2 = player.gearset["ring2"]
            ring1 = player.gearset["ring1"]
            player.gearset["ring1"] = ring2
            player.gearset["ring2"] = ring1

    # Sroda looks best in the left ring slot.
    if "sroda" in player.gearset["ring2"].name.lower():
            ring2 = player.gearset["ring2"]
            ring1 = player.gearset["ring1"]
            player.gearset["ring1"] = ring2
            player.gearset["ring2"] = ring1

    # Niqmaddu and Regal look best in the right ring slot
    if ("niqmaddu" in player.gearset["ring1"].name.lower() and "regal" not in player.gearset["ring2"].name.lower()) or ("regal" in player.gearset["ring1"].name.lower() and "niqmaddu" not in player.gearset["ring2"].name.lower()):
            ring2 = player.gearset["ring2"]
            ring1 = player.gearset["ring1"]
            player.gearset["ring1"] = ring2
            player.gearset["ring2"] = ring1

    # player.gearset[slot]["Name"] = name_map[player.gearset[slot]["Name"].lower()]

    hardcode_gearset = {slot:name_map[player.gearset[slot].name.lower()] for slot in player.gearset}
    for slot in hardcode_gearset:
        hardcode_gearset[slot] = "" if hardcode_gearset[slot].lower()=="empty" else hardcode_gearset[slot]


            # |RangeAug = {", ".join(linosaugs)}
    bgwiki_text = f"""
    {'{'}{'{'}
        Guide Equipment Set
        |Set Name Background=#604028
        |Set Name Text Color=
        |Set Name Text Shadow=#000080
        |Set Name= {ws_name}[[{ws_name}|*]]
        |Set Border Color=#51414F
        |Equipment Set=
        {'{'}{'{'}
            Equipment Set
            |CaptionTop = {buffs} buff
            |CaptionBottom = {best_metric:.0f} damage
            |Main = {' '.join(k.capitalize() for k in hardcode_gearset["main"].split())} (Level 119 III)
            |Sub = {' '.join(k.capitalize() for k in hardcode_gearset["sub"].split())}
            |Range = {' '.join(k.capitalize() for k in hardcode_gearset["ranged"].split())}
            |Ammo = {' '.join(k.capitalize() for k in hardcode_gearset["ammo"].split())}
            |Head = {' '.join(k.capitalize() for k in hardcode_gearset["head"].split())}
            |Neck = {' '.join(k.capitalize() for k in hardcode_gearset["neck"].split())}
            |Ear1 = {' '.join(k.capitalize() for k in hardcode_gearset["ear1"].split())}
            |Ear2 = {' '.join(k.capitalize() for k in hardcode_gearset["ear2"].split())}
            |Body = {' '.join(k.capitalize() for k in hardcode_gearset["body"].split())}
            |Hands = {' '.join(k.capitalize() for k in hardcode_gearset["hands"].split())}
            |Ring1 = {' '.join(k.capitalize() for k in hardcode_gearset["ring1"].split())}
            |Ring2 = {' '.join(k.capitalize() for k in hardcode_gearset["ring2"].split())}
            |Back = {' '.join(k.capitalize() for k in hardcode_gearset["back"].split())}
            |BackAug = {", ".join(backaugs)}
            |Waist = {' '.join(k.capitalize() for k in hardcode_gearset["waist"].split())}
            |Legs = {' '.join(k.capitalize() for k in hardcode_gearset["legs"].split())}
            |Feet = {' '.join(k.capitalize() for k in hardcode_gearset["feet"].split())}
            |List = Y
            |Background =
        {'}'}{'}'}
        |Equipment Set Notes=ML{player.master_level} {player.main_job.upper()}/{player.sub_job.upper()}: {int(tp)} TP
        Updated {datetime.now().strftime("%Y %b. %d")}
    {'}'}{'}'}\n
    """
    print(bgwiki_text)

def build_set(main_job: str, sub_job: str, master_level: int, buffs: Buffs, abilities: dict[str, Any], enemy: "create_enemy", ws_name: str, spell_name: str, action_type: str, min_tp: float, check_gear: dict[str, Any], starting_gearset: Gearset, pdt_requirement: float, mdt_requirement: float, input_metric: str, print_swaps: bool, next_best_percent: float, optimizer_options: OptimizerOptions | None = None, ) -> tuple[Any, Any]:
    #
    # Build a valid gear set, test it, and return the best set found.
    #
    # action_type = "ranged attack", "weapon skill", "tp round", "spell cast"
    #
    main_job = main_job.lower()
    sub_job = sub_job.lower()
    options = _normalize_optimizer_options(optimizer_options)
    if options.seed is not None:
        np.random.seed(options.seed)
    n_iter = options.iterations

    # Defaults for values otherwise only set inside the optimization loop / branches.
    best_metric = 0.0001
    best_output: list[Any] = [0.0, 0.0, 0.0]
    invert = 1
    decimals = 1
    nondecimals = 8
    swaps: dict[str, list[Any]] = {}

    verbose_swaps = abilities.get("Verbose Swaps", False)
    use_fast_dt_gate = _can_use_fast_dt_gate(abilities)

    ws_dict = {"Katana": ["Blade: Retsu", "Blade: Teki", "Blade: To", "Blade: Chi", "Blade: Ei", "Blade: Jin", "Blade: Ten", "Blade: Ku", "Blade: Yu", "Blade: Metsu", "Blade: Kamu", "Blade: Hi", "Blade: Shun", "Zesho Meppo",],
        "Great Katana": ["Tachi: Enpi", "Tachi: Goten", "Tachi: Kagero", "Tachi: Jinpu", "Tachi: Koki","Tachi: Yukikaze", "Tachi: Gekko", "Tachi: Kasha", "Tachi: Ageha","Tachi: Kaiten", "Tachi: Rana", "Tachi: Fudo", "Tachi: Shoha", "Tachi: Mumei"],
        "Dagger": [ "Viper Bite", "Dancing Edge", "Shark Bite", "Evisceration", "Aeolian Edge", "Mercy Stroke", "Mandalic Stab", "Mordant Rime", "Pyrrhic Kleos", "Rudra's Storm", "Exenterator", "Ruthless Stroke"],
        "Sword": ["Fast Blade", "Fast Blade II", "Burning Blade", "Red Lotus Blade", "Seraph Blade", "Circle Blade", "Swift Blade", "Savage Blade", "Sanguine Blade", "Knights of Round", "Death Blossom", "Expiacion", "Chant du Cygne", "Requiescat", "Imperator"],
        "Scythe": ["Slice", "Dark Harvest", "Shadow of Death", "Nightmare Scythe", "Spinning Scythe", "Guillotine", "Cross Reaper", "Spiral Hell", "Infernal Scythe", "Catastrophe", "Quietus", "Insurgency", "Entropy", "Origin", ], 
        "Great Sword":["Hard Slash", "Freezebite", "Shockwave", "Sickle Moon", "Spinning Slash", "Ground Strike", "Herculean Slash", "Resolution", "Scourge", "Dimidiation", "Torcleaver", "Fimbulvetr", ], 
        "Club":["Shining Strike", "Seraph Strike", "Skullbreaker", "True Strike", "Judgment", "Hexa Strike", "Black Halo", "Randgrith", "Exudation", "Mystic Boon", "Realmrazer", "Dagda"], 
        "Polearm":["Double Thrust", "Thunder Thrust", "Raiden Thrust", "Penta Thrust", "Wheeling Thrust", "Impulse Drive", "Sonic Thrust", "Geirskogul", "Drakesbane", "Camlann's Torment", "Stardiver", "Diarmuid", ], 
        "Staff":["Heavy Swing", "Rock Crusher", "Earth Crusher", "Starburst", "Sunburst", "Shell Crusher", "Full Swing", "Cataclysm", "Retribution", "Gate of Tartarus", "Omniscience", "Vidohunir", "Garland of Bliss", "Shattersoul", "Oshala"], 
        "Great Axe":["Iron Tempest", "Shield Break", "Armor Break", "Weapon Break", "Raging Rush", "Full Break", "Steel Cyclone", "Fell Cleave", "Metatron Torment", "King's Justice", "Ukko's Fury", "Upheaval", "Disaster"], 
        "Axe":["Raging Axe", "Spinning Axe", "Rampage", "Calamity", "Mistral Axe", "Decimation", "Bora Axe", "Onslaught", "Primal Rend", "Cloudsplitter", "Ruinator", "Blitz", ], 
        "Archery":["Flaming Arrow", "Piercing Arrow", "Dulling Arrow", "Sidewinder", "Blast Arrow", "Empyreal Arrow", "Refulgent Arrow", "Namas Arrow", "Jishnu's Radiance", "Apex Arrow", "Sarv"], 
        "Marksmanship":["Hot Shot", "Split Shot", "Sniper Shot", "Slug Shot", "Blast Shot", "Detonator", "Coronach", "Leaden Salute", "Trueflight", "Wildfire", "Last Stand", "Terminus", ], 
        "Hand-to-Hand":["Combo","One Inch Punch","Raging Fists","Spinning Attack","Howling Fist","Dragon Kick","Asuran Fists","Tornado Kick","Ascetic's Fury","Stringing Pummel","Final Heaven","Victory Smite","Shijin Spiral","Maru Kala","Dragon Blow",],
        }

    melee_ws = [ws for skill in ws_dict if skill not in ["Archery","Marksmanship"] for ws in ws_dict[skill]]
    ranged_ws = [ws for skill in ws_dict if skill in ["Archery","Marksmanship"] for ws in ws_dict[skill]]
    
    ws_type = "melee" if ws_name in melee_ws else "ranged" if ws_name in ranged_ws else "None"
    
    if " Shot" in spell_name:
        spell_type = "Quick Draw"
    elif spell_name=="Ranged Attack":
        spell_type="Ranged Attack"
    elif (": Ichi" in spell_name) or (": Ni" in spell_name) or (": San" in spell_name):
        spell_type = "Ninjutsu"
    else:
        spell_type = "Elemental Magic"

    if options.restart_count > 1:
        best_restart_player: Any = None
        best_restart_output: list[Any] = []
        best_restart_metric = 0.0001
        for restart_index in range(options.restart_count):
            restart_player, restart_output = build_set(
                main_job,
                sub_job,
                master_level,
                buffs,
                abilities,
                enemy,
                ws_name,
                spell_name,
                action_type,
                min_tp,
                _copy_check_gear(check_gear),
                _copy_gearset(starting_gearset),
                pdt_requirement,
                mdt_requirement,
                input_metric,
                print_swaps,
                next_best_percent,
                _single_start_options(options, restart_index),
            )
            restart_metric, _, _, _, _ = _evaluate_metric(restart_player, enemy, ws_name, spell_name, action_type, min_tp, ws_type, spell_type, input_metric)
            if restart_metric > best_restart_metric:
                best_restart_metric = restart_metric
                best_restart_player = restart_player
                best_restart_output = restart_output

        return best_restart_player, best_restart_output

    # List of weapon skills and their associated weapons.
    restricted_ws = {"Blade: Metsu":"Kikoku",
                    "Final Heaven":"Spharai",
                    "Mercy Stroke":"Mandau",
                    "Knights of Round":"Excalibur",
                    "Scourge":"Ragnarok",
                    "Onslaught":"Guttler",
                    "Metatron Torment":"Bravura",
                    "Catastrophe":"Apocalypse",
                    "Geirskogul":"Gungnir",
                    "Tachi: Kaiten":"Amanomurakumo",
                    "Randgrith":"Mjollnir",
                    "Gate of Tartarus":"Claustrum",
                    "Namas Arrow":"Yoichinoyumi",
                    "Coronach":"Annihilator",
                    "Fast Blade II":"Onion Sword III",
                    "Dragon Blow":"Dragon Fangs",
                    "Imperator":"Caliburnus",
                    "Zesho Meppo":"Dokoku",
                    "Terminus":"Earp",
                    "Origin":"Foenaria",
                    "Diarmuid":"Gae Buide",
                    "Fimbulvetr":"Helheim",
                    "Tachi: Mumei":"Kusanagi-no-Tsurugi",
                    "Disaster":"Laphria",
                    "Dagda":"Lorg Mor",
                    "Ruthless Stroke":"Mpu Gandring",
                    "Oshala":"Opashoro",
                    "Sarv":"Pinaka",
                    "Blitz":"Spalirisos",
                    "Maru Kala":"Varga Purnikawa",
                    }

    # If testing a melee WS, only check instruments in the "ranged" slot.
    # This does not apply to RNG or COR who might want savage blade sets to test gun/bow options
    if ws_type=="melee" and main_job not in ["rng", "cor"]:
        check_gear["ranged"] = [k for k in check_gear["ranged"] if k.type not in ["Crossbow", "Gun", "Bow"]]
        check_gear["ammo"] = [k for k in check_gear["ammo"] if k.type not in ["Bolt", "Bullet", "Arrow"] and "antitail" not in k.name2]

    check_gear = _prefilter_check_gear(check_gear, main_job.lower())

    # Rather than start with an empty slot, randomly build a set from the selected gear so we likely start with some accuracy+ and avoid getting stuck.
    # Do not adjust slots that are not being checked.
    for slot in starting_gearset:

        # Unequip gear you can't wear if it's already equipped, even if the slot is "frozen"
        if main_job.lower() not in starting_gearset[slot].jobs:
            starting_gearset[slot] = Empty

        frozen_slot = (len(check_gear[slot]) == 0)
        if not frozen_slot:
            starting_gearset[slot] = np.random.choice(check_gear[slot])
            
            # Avoid wearing two rare items in initial gearset to prevent "unphysical" sets.
            if slot == "ring2" and (starting_gearset["ring1"].name2 == starting_gearset["ring2"].name2):
                starting_gearset["ring2"] = Empty
            if slot == "ear2" and (starting_gearset["ear1"].name2 == starting_gearset["ear2"].name2):
                starting_gearset["ear2"] = Empty


    best_set =  starting_gearset.copy()

    # Define JSE earrings now. We'll use them later to prevent Balder's Earring+1 and a JSE+2 being equipped at the same time since we ignore right_ear requirement for testing.
    jse_ears = _jse_ear_names()

    # Intern every piece the search can reach so a candidate set is a tuple of
    # small ints. That makes "is this already equipped?" an int compare and keeps
    # the payload shipped to worker processes to a few bytes per candidate.
    slots = tuple(starting_gearset)
    pieces, piece_id = _intern_pieces([best_set.values(), *(check_gear[slot] for slot in slots)])
    ctx = EvalContext(
        main_job=main_job, sub_job=sub_job, master_level=master_level, buffs=buffs, abilities=abilities,
        enemy=enemy, ws_name=ws_name, spell_name=spell_name, action_type=action_type, min_tp=min_tp,
        ws_type=ws_type, spell_type=spell_type, input_metric=input_metric, ws_dict=ws_dict,
        restricted_ws=restricted_ws, jse_ears=jse_ears, use_fast_dt_gate=use_fast_dt_gate,
        slots=slots, pieces=tuple(pieces),
        dt_vectors=tuple(_dt_vector(piece.stats) for piece in pieces),
        buff_dt=_dt_sum(_dt_vector(buff_stats) for buff_stats in buffs.values()),
    )
    best_ids = tuple(piece_id[id(best_set[slot])] for slot in slots)
    pools = [[piece_id[id(item)] for item in check_gear[slot]] for slot in slots]

    # Slots with nothing selected are frozen. Leaving them out of the
    # neighbourhood matters most for three- and four-slot swaps, where every
    # extra slot multiplies the combination count.
    swappable = [index for index in range(len(slots)) if pools[index]]
    candidates_per_pass = sum(
        math.prod(len(pools[swappable[axis]]) for axis in combo)
        for combo in _slot_combos(len(swappable), options.max_swap_slots)
    )

    # invert only depends on the action and metric, so one evaluation of the
    # starting set fixes it for the whole run. The starting set may be one the
    # search would reject, so fall back rather than fail on it.
    decimals, nondecimals = METRIC_FORMAT[action_type]
    try:
        _, _, invert, _, _ = _score_with_output(ctx, best_ids)
    except Exception:
        invert = 1

    mapper = _make_mapper(ctx, best_ids, _resolve_workers(options), candidates_per_pass)

    pdt = 200 # How much PDT the set has
    mdt = 200

    conditional_converge_count = 0 # Break out of the loop if converged.
    pdt_old = 200 # Used to check if the automatic set finder gets stuck trying to find a set that doesn't exist. Compare this value to the old value. If no change in 3 consecutive iterations, then break out.
    mdt_old = 200

    pdt_thresh = pdt_requirement # How much PDT the final set is aiming for, taken from the user input.
    mdt_thresh = mdt_requirement

    pdt_thresh_temp = 200 # How much PDT the current new set must have to be accepted. The starting values are high to ensure that the code enters the loop to begin with.
    mdt_thresh_temp = 200


    try:
        while pdt > pdt_thresh or mdt > mdt_thresh:
            # print(f"\nChecking conditions: PDT:{pdt_thresh_temp},  MDT:{mdt_thresh_temp}")

            # Reset some variables between PDT/MDT iterations.

            best_metric = 0.0001 # Metric used to find the best set. This part of the code exclusively looks for "highest number".

            for z in range(n_iter):
                print(f"Current iteration: {z+1}")

                converged_ids = best_ids # The loop is considered converged if the set after a full iteration is the same as this set (no improvements were found).

                # A list of items in each slot that are within some % of the best item in that slot.
                swaps = {"ammo":[],"head":[],"neck":[],"ear1":[],"ear2":[],"body":[],"hands":[],"ring1":[],"ring2":[],"waist":[],"legs":[],"feet":[]}

                # Randomize the order that we check gear slots in
                slot_order = list(swappable)
                np.random.shuffle(slot_order)
                combos = [tuple(slot_order[axis] for axis in combo) for combo in _slot_combos(len(slot_order), options.max_swap_slots)]

                best_ids, best_metric, best_output = _optimize_pass(
                    ctx, mapper, _Neighborhood(combos, pools), best_ids, best_metric, best_output,
                    swaps, next_best_percent, pdt_thresh_temp, mdt_thresh_temp,
                    verbose_swaps, invert, decimals, nondecimals,
                )

                if best_ids == converged_ids: # If no improvement is found after one full iteration.
                    break # Break out of the main loop and check PDT/MDT conditions.

            best_set = ctx.gearset(best_ids)
            best_player0 = create_player(main_job, sub_job, master_level, best_set, buffs, abilities)
            pdt, mdt = _dt_for_player(best_player0)

            # Compare the pdt and mdt values from this iteration with the previous iteration.
            if pdt == pdt_old and mdt == mdt_old:
                conditional_converge_count += 1
                if conditional_converge_count >= 3:
                    print("Unable to find a set which satisfies the conditions better than the current set. Exiting.")
                    break
            else:
                conditional_converge_count = 0

            # Save the PDT and MDT values from this iteration to compare with the next iteration.
            pdt_old = pdt
            mdt_old = mdt

            # Update the temporary PDT and MDT requirements so that the next set is slightly closer to the true requirements.
            pdt_thresh_temp = pdt - options.dt_step if pdt-options.dt_step > pdt_thresh else pdt_thresh
            mdt_thresh_temp = mdt - options.dt_step if mdt-options.dt_step > mdt_thresh else mdt_thresh

            print(f"Current best set: PDT:{pdt},  MDT:{mdt}")
    finally:
        mapper.close()


    # At this point, we've found the best conditional set.

    # Swap the earrings to make sure the "Right Ear:" effect earrings show up in the ear2 slot.
    if best_set["ear1"].name in jse_ears+["Balder Earring +1"]:
        best_set["ear1"],best_set["ear2"] = best_set["ear2"],best_set["ear1"]

    # Record the stats for the best gear set.
    best_player = create_player(main_job, sub_job, master_level, best_set, buffs, abilities)


    header = {"weapon skill":ws_name,"spell cast":spell_name,"attack round":"Melee TP set"}[action_type]
    # Print a fancy output.
    print("==============================================================")
    print(f"Best   \"{input_metric}\"   \"{header}\"   set")
    print("==============================================================")
    for k in best_player.gearset:
        print(f"{k:>10s}  {best_player.gearset[k].name2:<50s}")
    print()
    if action_type=="attack round":
        if input_metric=="Time to WS":
            print(f"Avg WS Time = {best_metric**invert:<{nondecimals}.{decimals}f} s")
            print(f"Avg TP per round = {best_output[1]:<5.1f} TP")
        else:
            print(f"Avg Damage per round = {best_output[0]:<{nondecimals}.{decimals}f} damage")
            print(f"Avg time per round = {best_output[2]:<5.1f} s")
            print(f"Avg TP per round = {best_output[1]:<5.1f} TP")
    else:
        print(f"Avg Damage = {best_output[0]:<{nondecimals}.{decimals}f} damage")
        print(f"Avg TP return = {best_output[1]:<5.1f} TP")
    print("==============================================================")
    print("==============================================================")

    if print_swaps:
        print(f"\nList of potential swaps within {next_best_percent}% of the best set ({float(best_metric)**invert:<{nondecimals}.{decimals}f}):")
        for slot in swaps:
            for swap in swaps[slot]:
                line = f"{slot:<6s} {swap[0]:<50s} {float(swap[1]):<{nondecimals}.{decimals}f} {best_metric%swap[1]**invert/best_metric*100:>5.1f}%"
                print(line)

    # Print additional output formatted for BG Wiki item sets.
    if False:
        format_bgwiki(ws_name, (min_tp), best_player, best_metric)

    return(best_player, best_output)

if __name__ == "__main__":

    main_job = sys.argv[1]
    sub_job = sys.argv[2]
    master_level = int(sys.argv[3])
    buffs: dict[str, Any] = {}
    abilities: dict[str, Any] = {}
    enemy = create_enemy(preset_enemies["Apex Toad"])
    ws_name = "Blade: Metsu"
    spell_name = "Waterja"
    action_type = "weapon skill"
    min_tp = 1000
    check_gear = gear_dict
    starting_gearset: Gearset = { "main" : Heishi,
                        'sub' : Crepuscular_Knife,
                        'ranged' : Empty,
                        'ammo' : Seki,
                        'head' : Malignance_Chapeau,
                        'body' : Tatenashi_Haramaki,
                        'hands' : Malignance_Gloves,
                        'legs' : Samnuha_Tights,
                        'feet' : Malignance_Boots,
                        'neck' : Ninja_Nodowa,
                        'waist' : Sailfi_Belt,
                        'ear1' : Dedition_Earring,
                        'ear2' : Telos_Earring,
                        'ring1' : Gere_Ring,
                        'ring2' : Epona_Ring,
                        'back' : np.random.choice(cast(Any, [k for k in capes if "nin" in k.jobs and "DEX Store TP" in k.name2 and "Ranged" not in k.stats]))}
    pdt_requirement = -50
    mdt_requirement = -21
    print_swaps = True
    next_best_percent = 1

    metric = "Damage Dealt"

    player, output = build_set(main_job, sub_job, master_level, buffs, abilities, enemy, ws_name, spell_name, action_type, min_tp, check_gear, starting_gearset, pdt_requirement, mdt_requirement, metric, print_swaps, next_best_percent)
    print(player.stats)


    # TODO: If hit rate is < 20% in initial set, then begin by finding and equipping the max accuracy piece in each slot before finding the best set.
