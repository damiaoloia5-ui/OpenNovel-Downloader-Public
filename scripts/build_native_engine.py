"""Maintainer-only Windows component build; source, dependencies and checksums stay bundled."""
import argparse
import hashlib
import json
import pathlib
import shutil

from build_offline_engine import source_archive
from rebuild_native_engine import build


def assemble(source_input, output, repo):
    if output.exists():
        raise FileExistsError('use a new output directory; preserve previous builds')
    inventory = json.loads((repo / 'third_party/local-engine-source-lock.json').read_text(encoding='utf-8'))
    for entry in inventory['files']:
        if hashlib.sha256((source_input / 'upstream' / entry['path']).read_bytes()).hexdigest() != entry['sha256']:
            raise ValueError('reviewed source changed')
    provenance = json.loads((source_input / 'provenance.json').read_text(encoding='utf-8'))
    if provenance['candidate_commit'] != inventory['commit'] or provenance['unknown_dependency_licenses']:
        raise ValueError('source provenance or dependency license review missing')
    source = output / 'source'
    shutil.copytree(source_input, source)
    wrapper = repo / 'scripts/research/engine_probe_examples/opennovel_native_service.rs'
    shutil.copy2(wrapper, source / 'upstream/rust/examples' / wrapper.name)
    # A separate workspace preserves all reviewed upstream files unchanged.
    (source / 'Cargo.toml').write_text('[workspace]\nmembers = ["upstream/rust"]\nresolver = "2"\n\n[profile.release]\nopt-level = "s"\nlto = true\ncodegen-units = 1\nstrip = true\n', encoding='utf-8')
    shutil.copy2(source / 'upstream/rust/Cargo.lock', source / 'Cargo.lock')
    shutil.copy2(repo / 'scripts/rebuild_native_engine.py', source / 'REBUILD-NATIVE.py')
    shutil.copy2(repo / 'scripts/build_native_engine.py', source / 'BUILD-NATIVE.py')
    (source / 'REBUILD-NATIVE.md').write_text('''# Independent Windows GPL component

The combined engine executable is GPL-3.0. Upstream source and license are in upstream/;
all pinned registry dependencies and their licenses are in vendor/. The original OpenNovel
stdio adapter and build scripts retain Apache-2.0 (OPENNOVEL-LICENSE.txt).
This executable is not linked into OpenNovel. It is launched with no AppContainer network
capabilities, explicit inherited pipes and a kill-on-close job. It exposes no device or account API.

Windows x64 build prerequisites: Rust 1.96 MSVC target, MSVC C++ tools and Windows SDK,
Python 3.11 or newer. End users do NOT need these tools.
From this extracted source directory run:

    python REBUILD-NATIVE.py --target C:\\native-engine-build

Cargo uses --locked --offline and the bundled vendor directory. The CRT is statically linked.
The Windows-subsystem adapter uses inherited pipes, avoiding console initialization in AppContainer.
Output: x86_64-pc-windows-msvc/release/examples/opennovel_native_service.exe under --target.
The legacy Linux rebuild instructions in REBUILD.md describe the preceding container variant only.
''', encoding='utf-8')
    binary = build(source, output / 'target')
    distribution = output / 'distribution'
    distribution.mkdir()
    shutil.copy2(binary, distribution / 'engine.exe')
    source_archive(source, distribution / 'corresponding-source.tar.gz')
    files = {p.name: {'bytes': p.stat().st_size, 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in distribution.iterdir()}
    manifest = {'schema_version': 1, 'backend': 'windows-appcontainer', 'candidate_commit': inventory['commit'],
                'binary_license': 'GPL-3.0', 'target': 'x86_64-pc-windows-msvc', 'rust': '1.96',
                'wrapper_sha256': hashlib.sha256(wrapper.read_bytes()).hexdigest(),
                'vendored_packages': provenance['vendored_packages'], 'files': files}
    (distribution / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
    return distribution


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=pathlib.Path, required=True, help='verified previous complete source with vendor/')
    parser.add_argument('--output', type=pathlib.Path, required=True, help='new build directory')
    args = parser.parse_args()
    print(assemble(args.source.resolve(), args.output.resolve(), pathlib.Path(__file__).resolve().parents[1]))
