"""Shared type definitions for wsdist_beta.

Central home for the reused structural shapes (gear pieces, gearsets, stat
sheets, buff tables) and the numpy float-array alias used across the calc
engine.

A gear piece is a ``GearPiece`` dataclass: identity/classification metadata
(name, type, skill_type, jobs, dmg, delay, rank, wsc) are typed attributes,
while the open, additive stat pool lives in ``stats: dict[str, int]``. The
engine accumulates a piece by iterating ``piece.stats``; consumers read
metadata through attributes. The stat sheets below stay plain dicts because
they are built with runtime-computed keys and hold summed float values.
"""

from dataclasses import dataclass, field
from collections.abc import Mapping
from typing import Any, TypeAlias

import numpy as np
import numpy.typing as npt

# The calc kernels mix float32 and float64 arrays (numba-compiled), so accept any
# floating dtype rather than pinning to float64.
FloatArray: TypeAlias = npt.NDArray[np.floating[Any]]

# Metadata keys that are identity/classification rather than additive numeric
# stats. They become typed attributes on GearPiece and are excluded from the
# numeric `stats` pool. Kept as a set so `from_flat` can partition a flat dict.
GEAR_METADATA_KEYS: frozenset[str] = frozenset(
    {"Name", "Name2", "Type", "Skill Type", "Jobs", "DMG", "Delay", "Rank", "WSC"}
)


@dataclass(slots=True)
class GearPiece:
    """A single equipment piece.

    Metadata (identity/classification) lives in typed attributes; the open,
    additive stat pool ("STR", "Attack", "Magic Accuracy", "PDT", ...) lives in
    `stats`, whose values are fixed-point ints (percent-like stats such as
    "Store TP"/"Crit Rate"/"ftp" are stored scaled and divided at the point of
    use). Build one from the flat dict literals in gear.py via `from_flat`.
    """

    name: str
    jobs: list[str] = field(default_factory=list[str])
    name2: str = ""
    type: str = "None"
    skill_type: str = "None"
    dmg: int = 0
    delay: int = 0
    rank: int | None = None
    wsc: tuple[str, int] | None = None
    stats: dict[str, int] = field(default_factory=dict[str, int])

    def __post_init__(self) -> None:
        if not self.name2:
            self.name2 = self.name

    @classmethod
    def from_flat(cls, d: dict[str, Any]) -> "GearPiece":
        # Partition a flat gear dict literal into metadata attributes + stats.
        wsc = d.get("WSC")
        return cls(
            name=d["Name"],
            name2=d.get("Name2") or d["Name"],
            type=d.get("Type", "None"),
            skill_type=d.get("Skill Type", "None"),
            jobs=list(d["Jobs"]) if "Jobs" in d else [],
            dmg=d.get("DMG", 0),
            delay=d.get("Delay", 0),
            rank=d.get("Rank"),
            wsc=(wsc[0], wsc[1]) if wsc is not None else None,
            stats={k: v for k, v in d.items() if k not in GEAR_METADATA_KEYS},
        )

# Equipment set: slot name ("main", "sub", "head", ...) -> gear piece.
Gearset: TypeAlias = dict[str, GearPiece]

# Accumulated player stat sheet keyed by stat name. Every value is numeric;
# non-float data (the WSC list, the Wyvern attack flag) lives in dedicated
# create_player attributes instead.
Stats: TypeAlias = dict[str, float]

# Enemy stat sheet keyed by stat name. Unlike the player sheet, every value is
# numeric (Defense, Evasion, VIT, ..., Magic Damage Taken) -- no list/str keys --
# so the value type narrows to float. Kept a plain dict (not a TypedDict) because
# the debuff loop writes runtime-computed keys (``stats[stat] -= ...``) and
# ``stats.pop("Magic DT%")``, both of which a TypedDict forbids.
EnemyStats: TypeAlias = dict[str, float]

# Active buff sheet from aggregate_buffs(): source name ("brd", "cor", "geo",
# "whm", "food") -> stat name -> numeric amount. Non-numeric food keys ("Name",
# "Type", ...) are filtered out before they reach here, so leaves are all numeric.
# Read-only Mapping (not dict) because consumers only iterate/read it, and the
# covariant value type lets callers pass int- or float-valued dicts (dict is
# invariant and would reject dict[str, int] literals against a float leaf).
Buffs: TypeAlias = Mapping[str, Mapping[str, float]]
