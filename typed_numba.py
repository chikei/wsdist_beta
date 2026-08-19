"""Typed pass-through for ``numba.njit``.

numba ships no usable type information, so under strict pyright a bare
``@njit`` erases the decorated function's signature (every call site degrades to
``Unknown``). Importing ``njit`` from here instead keeps the original signature
for the type checker while delegating to the real numba decorator at runtime.
This is the single boundary where numba's untyped surface is contained.

Compilation is cached on disk (``cache=True``). Compiling this module set costs
about 2.5 s, which every fresh process would otherwise pay: the GUI on startup,
and each optimizer worker process on spawn. With the cache a warm process
reaches its first damage calculation in ~0.15 s.
"""

from collections.abc import Callable
from typing import TypeVar

from numba import njit as _njit  # pyright: ignore[reportMissingTypeStubs, reportUnknownVariableType]

_F = TypeVar("_F", bound=Callable[..., object])


def njit(func: _F) -> _F:
    """Compile ``func`` with numba, presenting its original signature to callers."""
    return _njit(cache=True)(func)  # pyright: ignore[reportUnknownVariableType, reportReturnType, reportUnknownArgumentType]
