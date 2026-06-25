# Verification

Use the project virtual environment for verification.

```bash
.venv/bin/pyright
```

Runs strict static type checking for the files listed in `pyrightconfig.json`.
Expected result: `0 errors, 0 warnings, 0 informations`.

```bash
MPLCONFIGDIR=/tmp .venv/bin/python -m unittest discover -s tests
```

Runs the optimizer helper regression tests under `tests/`. `MPLCONFIGDIR=/tmp`
keeps Matplotlib import-time cache writes inside a writable directory.
Expected result: all tests pass.
