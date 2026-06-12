"""Shared type definitions for wsdist_beta.

Central home for the reused structural shapes (gear pieces, gearsets, stat
sheets, buff tables) and the numpy float-array alias used across the calc
engine.

The gear/stat dictionaries are accessed with runtime-computed string keys and
hold heterogeneous values (a stat is usually a number, but e.g. ``"WSC"`` holds
a list, and gear metadata keys hold strings/lists). They are therefore aliased
as ``dict[str, Any]`` rather than ``TypedDict``: a TypedDict forbids the dynamic
``piece[stat]`` access these dicts are built around. The aliases still document
intent and give every function signature that passes them a real type.
"""

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

# Accumulated player/enemy stat sheet keyed by stat name. Mostly float values,
# with a few keys (e.g. "WSC") holding lists.
Stats: TypeAlias = dict[str, Any]

# Buff tables (see buffs.py) are deeply/heterogeneously nested; consumers narrow
# on access.
Buffs: TypeAlias = dict[str, Any]
