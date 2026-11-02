"""Rebuild the separate GPL Windows engine from its complete corresponding source, offline."""
import argparse
import os
import pathlib
import subprocess


def build(source, target):
    source, target = source.resolve(), target.resolve()
    if not source.is_dir() or not (source / 'vendor').is_dir():
        raise ValueError('complete corresponding source required')
    env = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_INCREMENTAL='0',
               RUSTFLAGS='-C target-feature=+crt-static')
    # Cargo resolves the relative vendor path from cwd for this inline configuration.
    subprocess.run(['cargo', 'build', '--locked', '--offline', '--release', '--target',
                    'x86_64-pc-windows-msvc', '--manifest-path', str(source / 'Cargo.toml'),
                    '--config', 'source.crates-io.replace-with="vendored-sources"',
                    '--config', 'source.vendored-sources.directory="vendor"',
                    '--example', 'opennovel_native_service'], cwd=source, env=env, check=True)
    return target / 'x86_64-pc-windows-msvc/release/examples/opennovel_native_service.exe'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=pathlib.Path, default=pathlib.Path(__file__).resolve().parent)
    parser.add_argument('--target', type=pathlib.Path, required=True)
    args = parser.parse_args()
    print(build(args.source, args.target))
