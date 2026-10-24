"""Pin compiled recovery sources and their quoted transitive includes."""
import hashlib
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]
INCLUDE = ROOT / 'iterations/v2/001-original-recovery/source/include'
# Explicit batch include paths; the default preserves historical callers.
EXTRA_INCLUDE_DIRS = ()


def source_hashes(paths, compiled_sources=()):
    pending = [Path(path) for path in (*paths, *compiled_sources)]
    pending.append(Path(__file__))
    seen = set()
    while pending:
        path = pending.pop()
        path = (path if path.is_absolute() else ROOT / path).resolve()
        if path in seen:
            continue
        if not path.is_file():
            raise FileNotFoundError(path)
        seen.add(path)
        if path.suffix not in ('.cpp', '.hpp', '.h', '.inc'):
            continue
        for name in re.findall(r'^\s*#\s*include\s*"([^"]+)"',
                               path.read_text(encoding='utf-8'), re.M):
            candidates = (path.parent / name, INCLUDE / name,
                          *(directory / name for directory in EXTRA_INCLUDE_DIRS))
            included = next((p for p in candidates if p.is_file()), None)
            if included is None:
                raise FileNotFoundError(f'{path}: quoted include {name}')
            pending.append(included)
    return {p.relative_to(ROOT).as_posix(): hashlib.sha256(
        p.read_bytes().replace(b'\r\n', b'\n')).hexdigest() for p in sorted(seen)}
