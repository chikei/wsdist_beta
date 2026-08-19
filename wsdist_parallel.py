'''
Process-pool backend for the wsdist gear optimizer.

Scoring one candidate gear set costs a create_player build plus a damage
calculation, and both are pure Python holding the GIL, so threads buy nothing.
This module spreads batches of candidates over worker processes instead. Workers
receive the immutable evaluation context once at startup and then only see small
integer tuples per candidate, so the per-batch payload stays far below the work
it dispatches.

The pool is a drop-in for wsdist.SequentialMapper: both call wsdist.score_chunk
with the same context, so a run scores identically whichever backend is used.

Author: Kastra (Asura server)
'''
import multiprocessing
import multiprocessing.spawn
import sys
from concurrent.futures import ProcessPoolExecutor
from multiprocessing.context import BaseContext
from typing import Any

import wsdist
from wsdist import EvalContext, Group

# Worker-process state, populated by _init_worker.
_ctx: EvalContext | None = None


def _init_worker(ctx: EvalContext, warmup_ids: tuple[int, ...]) -> None:
    #
    # Adopt the evaluation context and pay the numba JIT cost up front, so the
    # first real batch is not stalled behind compilation. Scoring the incumbent
    # itself exercises exactly the kernels the run will use.
    #
    global _ctx
    _ctx = ctx
    wsdist.score_chunk(ctx, warmup_ids, [((0,), [(warmup_ids[0],)])], float("inf"), float("inf"))


def _score_worker(job: tuple[tuple[int, ...], list[Group], float, float]) -> list[float]:
    base_ids, groups, pdt_thresh, mdt_thresh = job
    assert _ctx is not None, "worker used before _init_worker ran"
    return wsdist.score_chunk(_ctx, base_ids, groups, pdt_thresh, mdt_thresh)


def _skip_main_module_in_workers() -> None:
    #
    # Every worker process re-executes the parent's __main__ before it runs
    # anything of ours. Workers only ever call score_chunk, so that import buys
    # nothing and costs plenty: from the GUI it pulls PySide6 into every worker,
    # and when the parent has no importable __main__ at all (python -c, a REPL,
    # an embedded interpreter) the workers die on startup and the pool is
    # unusable. The preload list below already gives workers everything they
    # need, so drop the instruction to import __main__.
    #
    original = getattr(multiprocessing.spawn, "get_preparation_data", None)
    if original is None or getattr(original, "_wsdist_skips_main", False):
        return # A future CPython moved this; workers just pay for __main__ again.

    def get_preparation_data(name: str) -> dict[str, Any]:
        data = original(name)
        data.pop("init_main_from_name", None)
        data.pop("init_main_from_path", None)
        return data

    get_preparation_data._wsdist_skips_main = True # pyright: ignore[reportFunctionMemberAccess]
    multiprocessing.spawn.get_preparation_data = get_preparation_data


def _mp_context() -> BaseContext:
    #
    # forkserver keeps worker startup to a fork from a clean, already-imported
    # server process; the preload list must be set explicitly or the server
    # imports __main__ and drags the GUI in with it. Windows has no fork.
    #
    _skip_main_module_in_workers()
    if sys.platform == "win32":
        return multiprocessing.get_context("spawn")
    context = multiprocessing.get_context("forkserver")
    context.set_forkserver_preload(["wsdist_parallel"])
    return context


class PoolMapper:
    #
    # Scores candidate batches across worker processes.
    #
    # Three constants keep dispatch proportionate to the work it carries.
    # Batches below LOCAL_LIMIT are scored here rather than shipped out at all,
    # because a round trip costs more than a couple of hundred candidates do;
    # those only occur in the few rounds after an improvement shrinks the batch.
    # Larger batches are cut into OVERSUBSCRIBE shares per worker so that a
    # share which happens to be all cheap rejects does not leave the round
    # waiting on one straggler, but never into shares below MIN_SHARE, where
    # per-share overhead would start to dominate.
    #
    LOCAL_LIMIT = 192
    MIN_SHARE = 24
    OVERSUBSCRIBE = 4

    def __init__(self, ctx: EvalContext, warmup_ids: tuple[int, ...], workers: int) -> None:
        self.width = workers
        # Big enough that a dispatch round carries real work, and capped so the
        # batch thrown away after an improvement stays a small fraction of the
        # pass.
        self.batch_floor = 256
        self.batch_ceiling = 256*workers
        self._ctx = ctx
        self._executor = ProcessPoolExecutor(
            max_workers=workers,
            mp_context=_mp_context(),
            initializer=_init_worker,
            initargs=(ctx, warmup_ids),
        )

    def score(self, base_ids: tuple[int, ...], groups: list[Group], total: int, pdt_thresh: float, mdt_thresh: float) -> list[float]:
        if total == 0:
            return []
        if total < self.LOCAL_LIMIT:
            return wsdist.score_chunk(self._ctx, base_ids, groups, pdt_thresh, mdt_thresh)
        parts = min(self.OVERSUBSCRIBE*self.width, max(1, total//self.MIN_SHARE))
        jobs = [(base_ids, share, pdt_thresh, mdt_thresh) for share in wsdist.split_groups(groups, total, parts)]
        metrics: list[float] = []
        for result in self._executor.map(_score_worker, jobs):
            metrics.extend(result)
        return metrics

    def close(self) -> None:
        self._executor.shutdown(wait=False, cancel_futures=True)
