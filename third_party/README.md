# Dependency license supplements

`license-overrides.json` records original license text omitted from some published
Rust crate archives. Every supplement includes its retrieval URL and SHA-256;
the release generator checks these hashes before packaging. Upstream GitHub files
are stored without Git newline conversion so hashes also match on Windows. Files
are pinned to the crate's `.cargo_vcs_info.json` commit. For `fxhash` and `mac`,
whose manifests declare dual Apache/MIT licensing, the Apache-2.0 option is used.
`selectors` uses the canonical Mozilla MPL-2.0 text. Dependencies keep their
licenses; the project's Apache-2.0 choice does not relicense them.

Run `npm run release:notices` after installing the locked dependencies. Generated
notices list exact crate versions and source download locations, including MPL
sources. Ordinary builds do not fetch license files from upstream. Dependency
updates that omit license text fail this step until a reviewed supplement exists.

The generated inventory intentionally includes build/test crates and may include
dependencies not linked into the final Windows executable. It is a notice
inventory, not a legal opinion or a claim that every listed crate ships as code.
