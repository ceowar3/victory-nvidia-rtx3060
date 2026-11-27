//! Typed 16-bit buffer accesses (`buffer_*_format_d16_*`) through views of the
//! console's integer and float formats: the bits of the halves must come back
//! unchanged through an integer view, where a float round trip through
//! `legacyF32ToF16` zeroes a denormal and canonicalizes a NaN.
//!
//! Every view here is a real `DXGI_FORMAT`: `R16G16B16A16_SINT` is a signed
//! descriptor, not a `UINT` one with a flag. `FLOAT` is the control. The d16
//! load's handle stays a float one: the driver is taken to pass the view's
//! 16-bit halves through as float *bits* (PR 93f44d9's shortcut). The table of
//! strides (`victory_shader::emit::SLOT_STRIDES`) says which views are integer
//! ones; dword `i` holds the stride of heap slot `i` in its low 30 bits and
//! `VIEW_INTEGER`/`VIEW_SIGNED` above them. With the flags clear the same
//! program takes the float path: that is what the tree before PR 93f44d9 did
//! for every view (no word was read at all).

use windows::Win32::Graphics::Dxgi::Common::*;

use super::host::{Dispatch, Host, Root, Stride, View};
use super::{HostShader, validator};
use crate::native::{
    ComputeIo, HwStage, NativeProgram, ResourceType, RootParam, RootType, StageIo, UsedResource,
};
use crate::{Container, dxbc, io, ir, resource, structure};

const S_ENDPGM: u32 = 0xBF81_0000;
/// `victory_shader::emit::VIEW_INTEGER` (spelled here so the same fixture
/// compiles in a tree without the patch: there the word is never read).
const VIEW_INTEGER: u32 = 1 << 31;
/// `victory_shader::emit::VIEW_SIGNED`.
const VIEW_SIGNED: u32 = 1 << 30;

/// A wave32 compute program of one thread: 4 root constants in s0..s3 and, in
/// s4, a table with the view's descriptor at dword 0 and a raw UAV's at dword
/// 8, both loaded into s8.. and s16... `body` follows.
fn native(body: &[u32]) -> NativeProgram {
    let mut words = vec![
        0xBE85_0380, // s_mov_b32 s5, 0
        0xF408_0202, // s_load_dwordx4 s[8:11], s[4:5], null
        0xFA00_0000,
        0xF408_0402, // s_load_dwordx4 s[16:19], s[4:5], 0x20
        0xFA00_0020,
        0xF408_0502, // s_load_dwordx4 s[20:23], s[4:5], 0x40
        0xFA00_0040,
    ];
    words.extend_from_slice(body);
    words.push(S_ENDPGM);
    let range = |kind, table_offset_dwords, dwords, stride| UsedResource {
        kind,
        root: RootType::Table,
        table_offset_dwords,
        lower_bound: 0,
        stride,
        dwords,
        user_data: 4,
        slot: 1,
    };
    NativeProgram {
        stage: HwStage::Cs,
        wave64: false,
        vgprs: 16,
        layout: vec![
            RootParam {
                root: RootType::Constants,
                table_offset_dwords: 0,
                sgprs: 4,
                base_register: 0,
                user_data: 0,
                slot: 0,
            },
            RootParam {
                root: RootType::Table,
                table_offset_dwords: 0,
                sgprs: 1,
                base_register: 0,
                user_data: 4,
                slot: 1,
            },
        ],
        used: vec![
            // The typed view: 4 channels of 2 bytes, one element of 8.
            range(ResourceType::Srv, 0, 4, 8),
            // The typed UAV (a load test uses a raw one here).
            range(ResourceType::Uav, 8, 4, 0),
            // The second element's typed SRV.
            range(ResourceType::Srv, 16, 4, 8),
        ],
        io: StageIo::Cs(ComputeIo {
            rsrc2: 5 << 1, // 5 user SGPRs: s0..s4
            threads: [1, 1, 1],
        }),
        code: words.iter().flat_map(|w| w.to_le_bytes()).collect(),
    }
}

/// `native` translated as the title's programs are, and validated.
fn translate(native: &NativeProgram) -> HostShader {
    let container = dxbc::build(&[]);
    let c = Container::parse(&container).expect("container");
    let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
    let st = structure::structure(&p).expect("structure");
    let res = resource::track(&p, &native.layout, native.stage, Some(&st)).expect("track");
    let io = io::raise(&p, native).expect("io::raise");
    let shader = super::emit(&c, native, &p, &st, &io, &res).expect("emit");
    if let Ok(mut v) = validator::Validator::load(validator::Validator::default_path().as_deref()) {
        v.validate(&mut shader.dxil.clone())
            .expect("DXIL validation failed");
    } else {
        eprintln!("no dxil.dll; validation skipped");
    }
    shader
}

/// Runs the program on hardware and WARP. `words` is the table of strides:
/// dword `i` is heap slot `i`. `views` are the descriptors, in heap order.
fn run(
    shader: &HostShader,
    words: &[u32],
    views: &[View<'_>],
) -> Vec<(&'static str, Vec<u32>)> {
    let mut out = Vec::new();
    let _one = super::valu_tests::ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    for (label, warp) in [("hardware", false), ("warp", true)] {
        let Ok(host) = Host::new(warp) else {
            eprintln!("no SM 6.6 {label} device; skipped");
            continue;
        };
        let (root, pso) = host
            .compute_pipeline(shader)
            .unwrap_or_else(|e| panic!("{label}: compute_pipeline failed: {e}"));
        let mut ud = vec![0u32; shader.user_data_dwords().max(1) as usize];
        for u in &shader.user_data {
            ud[u.index as usize] = 0;
        }
        // Root parameter 1 is the table of strides (parameter 0 is the
        // user data); a tree without the patch never asks for it, and
        // binding a parameter the signature does not have is a crash.
        let mut roots = vec![(0, Root::Constants(ud))];
        if shader
            .root_srvs
            .iter()
            .any(|&s| s == super::shader::SLOT_STRIDES)
        {
            roots.push((
                1,
                Root::View {
                    srv: true,
                    data: words.to_vec(),
                },
            ));
        }
        let words = host
            .dispatch(
                &root,
                &pso,
                &Dispatch {
                    roots,
                    views,
                    samplers: &[],
                    groups: [1, 1, 1],
                },
            )
            .unwrap_or_else(|e| panic!("{label}: dispatch failed: {e}"));
        out.push((label, words[0].clone()));
    }
    out
}

/// `buffer_load_format_d16_xyzw v[4:5], off, s[8:11], 0` and the same
/// instruction over the second element's descriptor at s[20:23], then the four
/// VGPRs raw to `s[16:19]` at offsets 0..12. v0 stays the null address VGPR.
const LOAD2_STORE: &[u32] = &[
    0xE20C_0000, // load element 0 -> v4:5
    0x8002_0400,
    0xE20C_0000, // load element 1 -> v6:7 (s[20:23], element 0 of its buffer)
    0x8005_0600,
    0xE070_0000, // buffer_store_dword v4, off, s[16:19], 0
    0x8004_0400,
    0xE070_0004, // v5, offset 4
    0x8004_0500,
    0xE070_0008, // v6, offset 8
    0x8004_0600,
    0xE070_000C, // v7, offset 12
    0x8004_0700,
];

/// `buffer_load_format_d16_xyzw v[4:5], off, s[8:11], 0` then
/// `buffer_store_format_d16_xyzw v[4:5], off, s[16:19], 0`: a typed store to
/// the UAV at table dword 8 (heap slot 1) followed by the host read-back.
const LOAD_STORE_D16: &[u32] = &[
    0xE20C_0000, // buffer_load_format_d16_xyzw v[4:5], off, s[8:11], 0
    0x8002_0400,
    0xE21C_0000, // buffer_store_format_d16_xyzw v[4:5], off, s[16:19], 0
    0x8004_0400,
];

// Element 0: 0x7c00, 0x0001, 0xffff, 0x0000
// Element 1: 0x8000, 0x7fff, 0x0000, 0xffff
const UINT_DATA: [u32; 4] = [0x0001_7c00, 0x0000_ffff, 0x7fff_8000, 0xffff_0000];

// Element 0: -1 (0xffff), -32768 (0x8000), 0x7fff, 0x0001
// Element 1: 0x8000, 0x7c00 (+inf as a half), 0x7e00 (NaN as a half), 0
const SINT_DATA: [u32; 4] = [0x8000_ffff, 0x0001_7fff, 0x7c00_8000, 0x0000_7e00];

// Element 0: 1.0, -1.0, 0.5, 2.0
// Element 1: 0x0001 (denormal), 0x7c00 (+inf), 0x7e00 (NaN), 0x8001
const F16_DATA: [u32; 4] = [0xbc00_3c00, 0x4000_3800, 0x7c00_0001, 0x8001_7e00];

fn assert_words(results: &[(&str, Vec<u32>)], expected: &[u32], what: &str) {
    assert!(!results.is_empty(), "no device ran the fixture");
    for (label, words) in results {
        assert_eq!(
            words[..expected.len()],
            *expected,
            "{label}: {what}: got {words:08x?}"
        );
        assert!(
            words[expected.len()..].iter().all(|&w| w == 0),
            "{label}: {what}: wrote past the element: {words:08x?}"
        );
    }
}

/// Two descriptors for the two elements: slot 0 is element 0, slot 2 is
/// element 1 as element 0 of its own buffer (the translator refuses a typed
/// load at a nonzero constant offset). Slot 1 is the raw UAV.
fn load_views(data: &[u32], format: DXGI_FORMAT) -> [View<'_>; 3] {
    assert_eq!(data.len(), 4, "two RGBA16 elements");
    [
        View::Buffer { data: &data[..2], stride: Stride::Format(format, 8) },
        View::RawUav(8),
        View::Buffer { data: &data[2..], stride: Stride::Format(format, 8) },
    ]
}

#[test]
fn a_d16_load_through_a_uint_view_keeps_the_halves() {
    let shader = translate(&native(LOAD2_STORE));
    let word = 8 | VIEW_INTEGER;
    let results = run(&shader, &[word, word, word, 0], &load_views(&UINT_DATA, DXGI_FORMAT_R16G16B16A16_UINT));
    assert_words(&results, &UINT_DATA, "UINT load");
}

#[test]
fn a_d16_load_through_a_real_sint_view_keeps_the_signed_halves() {
    let shader = translate(&native(LOAD2_STORE));
    let word = 8 | VIEW_INTEGER | VIEW_SIGNED;
    let results = run(&shader, &[word, word, word, 0], &load_views(&SINT_DATA, DXGI_FORMAT_R16G16B16A16_SINT));
    assert_words(&results, &SINT_DATA, "SINT load");
}

#[test]
fn a_d16_load_through_a_float_view_keeps_the_finite_controls() {
    let shader = translate(&native(LOAD2_STORE));
    let results = run(&shader, &[8, 8, 8, 0], &load_views(&F16_DATA, DXGI_FORMAT_R16G16B16A16_FLOAT));
    assert!(!results.is_empty(), "no device ran the fixture");
    for (label, words) in &results {
        // 1.0, -1.0, 0.5, 2.0: exact halves, no word needed.
        assert_eq!(words[..2], F16_DATA[..2], "{label}: finite F16: {words:08x?}");
    }
}

#[test]
fn a_d16_load_through_a_float_view_observes_denormal_and_nan() {
    // Observation only: the driver may canonicalize these, and this test does
    // not invent a canonical form. 0x7c00 is +inf, not a NaN.
    let shader = translate(&native(LOAD2_STORE));
    let results = run(&shader, &[8, 8, 8, 0], &load_views(&F16_DATA, DXGI_FORMAT_R16G16B16A16_FLOAT));
    assert!(!results.is_empty(), "no device ran the fixture");
    for (label, words) in &results {
        eprintln!("{label}: F16 denormal/inf/NaN halves observed {words:08x?}");
    }
}

/// One `buffer_store_format_d16_xyzw` writes one RGBA16 element; the second
/// element of the UAV stays zero.
fn store_words(data: &[u32]) -> [u32; 4] {
    [data[0], data[1], 0, 0]
}

#[test]
fn a_d16_store_through_a_uint_uav_keeps_the_zero_extended_bits() {
    let shader = translate(&native(LOAD_STORE_D16));
    let word = 8 | VIEW_INTEGER;
    let results = run(&shader, &[word, word, 0, 0], &[
        View::Buffer { data: &UINT_DATA[..2], stride: Stride::Format(DXGI_FORMAT_R16G16B16A16_UINT, 8) },
        View::TypedUav(DXGI_FORMAT_R16G16B16A16_UINT, 2),
    ]);
    assert_words(&results, &store_words(&UINT_DATA), "UINT store");
}

#[test]
fn a_d16_store_through_a_real_sint_uav_keeps_the_sign_extended_bits() {
    let shader = translate(&native(LOAD_STORE_D16));
    let word = 8 | VIEW_INTEGER | VIEW_SIGNED;
    let results = run(&shader, &[word, word, 0, 0], &[
        View::Buffer { data: &SINT_DATA[..2], stride: Stride::Format(DXGI_FORMAT_R16G16B16A16_SINT, 8) },
        View::TypedUav(DXGI_FORMAT_R16G16B16A16_SINT, 2),
    ]);
    assert_words(&results, &store_words(&SINT_DATA), "SINT store");
}

#[test]
fn a_d16_store_through_a_float_uav_keeps_the_finite_halves() {
    let shader = translate(&native(LOAD_STORE_D16));
    let results = run(&shader, &[8, 8, 0, 0], &[
        View::Buffer { data: &F16_DATA[..2], stride: Stride::Format(DXGI_FORMAT_R16G16B16A16_FLOAT, 8) },
        View::TypedUav(DXGI_FORMAT_R16G16B16A16_FLOAT, 2),
    ]);
    assert_words(&results, &store_words(&F16_DATA), "FLOAT store");
}

#[test]
fn without_the_integer_word_the_same_load_does_not_keep_the_halves() {
    let shader = translate(&native(LOAD2_STORE));
    let results = run(&shader, &[8, 8, 8, 0], &load_views(&UINT_DATA, DXGI_FORMAT_R16G16B16A16_UINT));
    assert!(!results.is_empty(), "no device ran the fixture");
    for (label, words) in results {
        eprintln!("{label}: float path {words:08x?}");
        assert_ne!(words, UINT_DATA, "{label}: the word must matter");
    }
}
