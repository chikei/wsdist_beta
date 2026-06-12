"""Shared type definitions for wsdist_beta.

Central home for the reused structural shapes (gear pieces, gearsets, stat
sheets, buff tables) and the numpy float-array alias used across the calc
engine.

The gear dictionaries are accessed with runtime-computed string keys and hold
heterogeneous values (a stat is usually a number, but gear metadata keys hold
strings/lists). They are therefore aliased as ``dict[str, Any]`` rather than
``TypedDict``: a TypedDict forbids the dynamic ``piece[stat]`` access these
dicts are built around. The aliases still document intent and give every
function signature that passes them a real type.
"""

from collections.abc import Mapping
from typing import Any, TypeAlias

import numpy as np
import numpy.typing as npt

# The calc kernels mix float32 and float64 arrays (numba-compiled), so accept any
# floating dtype rather than pinning to float64.
FloatArray: TypeAlias = npt.NDArray[np.floating[Any]]

# A single equipment piece: fixed metadata keys ("Name", "Type", "Skill Type",
# "Jobs") plus open stat keys ("STR", "Attack", "Magic Accuracy", ...). Values
# are numbers for stats and str / list[str] for metadata.
GearPiece: TypeAlias = dict[str, Any]

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
