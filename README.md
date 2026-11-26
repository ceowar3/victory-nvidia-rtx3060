# VIctory RTX 3060 — experimental integration

A Windows x64 fork/integration of VIctory with paired-lane shader translation,
NVIDIA descriptor-heap handling and CPU/GPU transfer work. Maintained by this
repository's owner; original authors and GPL notices are preserved.

**Not a finished PC port. No stable 30 FPS result is verified.** Some shaders,
lighting, textures and streaming behavior remain incorrect. See
[build notes](docs/BUILD-NOTES.md) for the actual tests and limitations.

This project does not distribute the game, its executable, extracted shaders,
assets, saves or generated caches. Users provide their own compatible files.

## Requirements

Windows 11 x64, AVX2 CPU, D3D12 GPU with Shader Model 6.6 and mesh shaders.
Local validation used an RTX 3060 12 GB and 64 GB system RAM; this is not a
universal hardware guarantee. Building requires MSVC C++ Build Tools, a Windows
SDK, Git and the Rust version pinned in rust-toolchain.toml.

## Build

```powershell
cargo build --release --locked -p victory
```

For the official SDK's DXIL validator, install the Windows SDK rather than
copying dependencies from a game. See the original documentation below.

## Run

```powershell
.\Run.ps1 -GameExe 'D:\MyGames\Title\game_scarlett_master.exe'
```

The portable launcher uses `%LOCALAPPDATA%\VIctory-RTX3060` for its own cache,
data and logs. It does not import or delete another runtime's saves. Keep the
game outside this source repository. `-VerboseLogs` enables verbose diagnostics;
`-LegacyDescriptors` restores the earlier descriptor-pool setting. Texture
readback and unverified shader experiments are disabled in this launcher.

## Verification

```powershell
cargo test -p victory-gpu --locked --lib -- --test-threads=1
cargo xtask policy
```

GPU tests need a suitable D3D12 device; a passing source-only check does not prove
gameplay compatibility. See [BUILD-NOTES](docs/BUILD-NOTES.md) and the
[preserved upstream documentation](docs/README-paired-upstream.md).

## License and credits

GPL-3.0-or-later: [LICENSE](LICENSE). VIctory originates with
[Marvisak and its contributors](https://github.com/Marvisak/VIctory); this snapshot
is based on [fxceless/victory-fork](https://github.com/fxceless/victory-fork).
[Detailed provenance](docs/BUILD-NOTES.md#provenance-and-license) and
[upstream format-table exception](docs/asset-policy.md) are retained.

No affiliation with or endorsement from the game's developer or publisher is claimed.
