# RTX 3060 experimental snapshot

This is a modified VIctory runtime, not a PC game release. The repository owner
maintains this integration. Upstream authors retain authorship of their code.

## Included changes

- Paired-lane translation based on fxceless PR 854, preserving the local NVIDIA path.
- Pixel helper-lane and mask handling fixes, with output regression tests.
- Host blend-state sanitation in both graphics and mesh pipeline streams.
- Configurable capped dynamic descriptor pool, adapted from
  trustno1-coder/VIctory-NVIDIA commit a9632bc8de343aa20adf4f1123f16445f5b1b21b.
  On the tested driver the requested 524288 becomes 499999 usable dynamic slots,
  up from 262144; the null slot and title partition remain reserved.
- CPU/GPU queue, copy and timing work derived from fxceless, with optional
  small-buffer placement and merged card-run experiments.
- Experimental texture readback based on fxceless PR 999, with allocation-ticket
  invalidation and single-subresource selection guards. It remains disabled in
  the portable launcher because concurrent CPU-write correctness is unresolved.
- Bounded diagnostic caller-frame logging for unimplemented pipeline methods.

## Observed local checks

Windows 11, Core i9-14900K, RTX 3060 12 GB, 64 GB RAM, MSVC Rust 1.98.1:

- GPU library: 198 passed, 0 failed, 5 ignored, serial, enlarged descriptor pool.
- Release build: `cargo build --release --locked -p victory` succeeded.
- Real driver log confirmed 499999 dynamic descriptors; the title launched.
- Load/intro counters varied and included 25 FPS in one sample. This is **not**
  a matched gameplay benchmark or a stable 30 FPS claim.

The source tests need suitable Windows/D3D12 tooling; some GPU tests cannot run
on a GPU-less CI runner. Existing warnings and upstream failures must be reported,
not converted into claims that all checks passed.

## Known limitations

Pixel/mesh lane-mask translations still fail for some materials. Dark rooms,
texture invalidation, streaming stalls and missing pipeline methods remain.
Experimental ambient hooks, broad ray-walk substitutions and grouped scalar-load
options tested only in separate copies are not advertised as integrated fixes.
There is no verified stable 30 FPS cinematic or gameplay result.

## Provenance and license

The starting history is fxceless/victory-fork at
0751197e744c92d392a65d02fe40973f566843e2, including its original VIctory ancestry.
Relevant reference projects include Marvisak/VIctory, fxceless/victory-fork,
Belobug/VIctory-Belobug-AMD, DexNom/VIctory, avatarmanz/VIctory-7900XTX and
trustno1-coder/VIctory-NVIDIA. Reference-only projects are not all code contributors
to this snapshot. Existing copyright notices and commit authors are preserved.

Runtime code is GPL-3.0-or-later; see LICENSE and the preserved upstream README.
The upstream BCPack format-table exception is documented in docs/asset-policy.md
and is outside the runtime's GPL coverage. Do not represent that file as newly
authored project code. This snapshot makes no claim to the game or its assets.
