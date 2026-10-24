"""Config-driven fresh game build using the established receipt/manifest driver."""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--phase', choices=('configure', 'build'), required=True)
    parser.add_argument('--attempt', required=True)
    args = parser.parse_args()
    config_path = args.config.resolve()
    config_path.relative_to(ROOT)
    cfg = json.loads(config_path.read_bytes())
    spec = importlib.util.spec_from_file_location('game_batch_base', ROOT / 'scripts/research/build-v2-game-entry-335.py')
    base = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(base)
    driver = base.driver
    driver.RUN = (ROOT / cfg['run']).resolve()
    driver.BUILD = (ROOT / cfg['build']).resolve()
    driver.OUT = (ROOT / cfg['experiment']).resolve()
    driver.RUN.relative_to(ROOT / 'iterations/v2/001-original-recovery/runs')
    driver.BUILD.relative_to(ROOT / 'local/builds/v2')
    driver.OUT.relative_to(ROOT / 'local/experiments')
    # The receipt scanner must resolve the same explicit repo include paths as
    # CMake, including shared headers owned by prior run directories.
    import v2_source_dependencies as dependencies
    extra_includes = tuple((ROOT / item).resolve() for item in cfg.get('source_include_dirs', []))
    for directory in extra_includes:
        directory.relative_to(ROOT)
        assert directory.is_dir(), directory
    dependencies.EXTRA_INCLUDE_DIRS = extra_includes
    source_hashes = driver.source_hashes

    def pins(paths, compiled_sources=()):
        # CMake supplies the complete generation dependencies, including nested
        # historical run CMakeLists. Avoid another hardcoded chain per batch.
        cmake = set()
        if extra_includes:
            import xml.etree.ElementTree as ET
            declared = set()
            for project in driver.BUILD.rglob('*.vcxproj'):
                for node in ET.parse(project).iter():
                    if node.tag.rsplit('}', 1)[-1] == 'AdditionalIncludeDirectories':
                        for item in (node.text or '').split(';'):
                            if item and '$' not in item and '%' not in item and Path(item).is_absolute():
                                declared.add(Path(item).resolve())
            assert set(extra_includes) <= declared, 'Receipt include paths must be declared by CMake'
        iteration = ROOT / 'iterations/v2/001-original-recovery'
        for stamp in driver.BUILD.glob('**/generate.stamp.depend'):
            for line in stamp.read_text(encoding='utf8').splitlines():
                if not line or line.startswith('#'):
                    continue
                path = Path(line).resolve()
                if path.is_file() and path.is_relative_to(iteration):
                    cmake.add(path)
        return source_hashes([*paths, Path(__file__), config_path,
                              driver.RUN.parent / '335-resource-open-game-integration/CMakeLists.txt',
                              *sorted(cmake)], compiled_sources=compiled_sources)

    driver.source_hashes = pins
    capture = driver.capture

    def fresh_capture(command, name):
        if '--build' in command:
            # A retry in the same tree must still compile the complete graph.
            # An incremental one-file receipt cannot prove a fresh source build.
            command = [*command, '--clean-first']
        return capture(command, name)

    driver.capture = fresh_capture
    sys.argv = [sys.argv[0], '--phase', args.phase, '--attempt', args.attempt]
    driver.main()
    if args.phase == 'build':
        summary = json.loads((driver.RUN / f'build-{args.attempt}.json').read_bytes())
        receipt = json.loads((ROOT / summary['receipt']).read_bytes())
        log = (ROOT / receipt['log']).read_text(encoding='utf8')
        from collections import Counter
        events = re.findall(r'(?m)^\s+([A-Za-z0-9_.-]+\.cpp)\s*$', log)
        assert Counter(events) == Counter(Path(p).name for p in summary['compiled_source_roots']), 'Incomplete fresh compile; receipt retained'
        assert not summary['compiler_errors'] and not summary['other_linker_errors'], 'See build report'


if __name__ == '__main__':
    main()
