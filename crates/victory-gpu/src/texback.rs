//! Textures shaders write that the title's own code then reads on the CPU.
//! On the console the CPU and the GPU share memory, so what a compute pass
//! stores into a texture is in title memory for the CPU to read. Here it stays
//! in the texture's host copy. The title's ocean is one: its wave passes write
//! a 256x256 height map (R32_FLOAT) and a displacement map (R32G32_FLOAT), and
//! its CPU code samples both, which is what boats float on; it read memory
//! nothing had written, and boats sank (the upstream fork's read watch, run
//! rw1, found about 30 places in the title reading those two kinds).
//!
//! For textures of the kinds in `VICTORY_TEXTURE_BACK` (comma-separated
//! `<width>x<height>f<format>`), a list in which shaders write one copies it
//! into a read-back buffer of ours when it closes, and once the list has run
//! the texels are tiled into the texture's title memory in the console's
//! layout, as the console's GPU would have left them.
//!
//! The switch is off unless asked for, because upstream enables it by default
//! and it has not yet been proved on this machine (the boat/sea check). The
//! variable is read once, at the first texture a list writes:
//!
//! - unset, empty or whitespace: no kind, nothing is recorded;
//! - `1`: the kinds the upstream trace found, `DEFAULT`;
//! - `0`: no kind: the switch is off explicitly (and overrides a list given
//!   with it, which cannot be read as on);
//! - anything else: that comma-separated list of kinds, entries that do not
//!   parse are skipped.
//!
//! shortcut: by kind, not by texture: textures of those kinds the title never
//! reads are copied back too (about a megabyte per such list). The copy shows
//! what the list's last write left; a title read in between sees the frame
//! before, as it would before the console's GPU has run.
//!
//! Ported from upstream f5c03f3b (fxceless/victory-fork). The subresource
//! order follows D3D12's own: a volume's Z slices are part of each mip's
//! footprint, not array layers.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use windows::Win32::Graphics::Direct3D12::*;

use crate::object::This;

/// A texture kind copied back: width, height, DXGI format.
pub(crate) type Kind = (u64, u32, u32);

/// Until UAV mip/slice selections are carried through recording, a whole-resource
/// readback is only valid for a single mip of a single array layer.
pub(crate) fn whole_resource_is_one_subresource(mips: u32, layers: u32) -> bool {
    mips == 1 && layers == 1
}

/// The kinds `VICTORY_TEXTURE_BACK=1` asks for: the two the title's ocean
/// wave passes write (the height and displacement maps its CPU code samples)
/// and the two other water formats the upstream read watch found.
const DEFAULT: &str = "256x256f41,256x256f16,64x64f16,128x128f36";

/// The kinds copied back: (width, height, format).
fn kinds() -> &'static [Kind] {
    static KINDS: OnceLock<Vec<Kind>> = OnceLock::new();
    KINDS.get_or_init(|| {
        let kinds = parse_kinds(&spec());
        if !kinds.is_empty() {
            tracing::info!(?kinds, "textures of these kinds that shaders write are copied back into title memory (VICTORY_TEXTURE_BACK)");
        }
        kinds
    })
}

/// `VICTORY_TEXTURE_BACK`'s value read as the spec of `kinds`: unset or
/// empty for none, `0` for none, `1` for `DEFAULT`, else the list itself.
fn spec() -> String {
    match std::env::var("VICTORY_TEXTURE_BACK").as_deref().map(str::trim) {
        Ok("") | Ok("0") | Err(_) => String::new(),
        Ok("1") => DEFAULT.to_owned(),
        Ok(list) => list.to_owned(),
    }
}

/// A list of `<width>x<height>f<format>` kinds, entries that do not parse
/// skipped.
fn parse_kinds(list: &str) -> Vec<Kind> {
    list.split(',')
        .filter_map(|k| {
            let (size, format) = k.trim().split_once('f')?;
            let (w, h) = size.split_once('x')?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?, format.trim().parse().ok()?))
        })
        .collect()
}

/// Whether `kind` is one of `kinds`.
fn matches(kinds: &[Kind], kind: Kind) -> bool {
    kinds.contains(&kind)
}

/// Whether title texture `resource` is of a kind copied back.
pub(crate) fn wanted(resource: This) -> bool {
    let kinds = kinds();
    !kinds.is_empty() && crate::resource::texture_kind(resource).is_some_and(|kind| matches(kinds, kind))
}

/// An outstanding write to a specific allocation. Mapping and resource
/// changes invalidate only the writes that could touch the replaced memory.
struct Ticket {
    owner: usize,
    at: usize,
    end: usize,
    valid: AtomicBool,
}

static TICKETS: Mutex<Vec<Weak<Ticket>>> = Mutex::new(Vec::new());
static TRACKING: AtomicBool = AtomicBool::new(false);

fn track(owner: usize, at: usize, end: usize) -> Arc<Ticket> {
    let ticket = Arc::new(Ticket { owner, at, end, valid: AtomicBool::new(true) });
    let mut tickets = TICKETS.lock().unwrap_or_else(|e| e.into_inner());
    tickets.retain(|weak| weak.strong_count() != 0);
    tickets.push(Arc::downgrade(&ticket));
    TRACKING.store(true, Ordering::Relaxed);
    ticket
}

fn invalidate(matches: impl Fn(&Ticket) -> bool) {
    if !TRACKING.load(Ordering::Relaxed) {
        return;
    }
    TICKETS.lock().unwrap_or_else(|e| e.into_inner()).retain(|weak| {
        let Some(ticket) = weak.upgrade() else { return false };
        if matches(&ticket) {
            ticket.valid.store(false, Ordering::Release);
        }
        true
    });
}

/// The title released texture `owner`: its host copy may go away, and the
/// memory it was placed in belong to someone else.
pub(crate) fn released(owner: usize) {
    invalidate(|ticket| ticket.owner == owner);
}

/// A texture was handed out at `at`: the allocation that was there is gone.
#[cfg(test)]
fn replaced(at: usize) {
    invalidate(|ticket| ticket.at == at);
}

/// The runtime remapped `[at, at + len)` of title memory: what a copy shows
/// from the range is no longer what the GPU wrote.
pub(crate) fn remapped(at: usize, len: usize) {
    let end = at.saturating_add(len);
    invalidate(|ticket| at < ticket.end && ticket.at < end);
}

/// A texture the list's shaders write and that is copied back when it
/// closes: the host copy to read, its title layout and allocation, and the
/// ticket that says the allocation is still the one the copy was recorded
/// against.
pub(crate) struct Target {
    pub(crate) copy: Arc<crate::resource::HostTexture>,
    layout: victory_formats::texture::Layout,
    ticket: Arc<Ticket>,
}

impl Target {
    pub(crate) fn new(resource: This) -> Option<Self> {
        // Keep the host copy independently of the title's resource lifetime.
        let copy = crate::resource::host_copy(resource)?;
        let _mapping = crate::watch::title_memory_reading();
        let (layout, at, size) = crate::resource::title_texture(resource)?;
        let end = at.checked_add(size).filter(|&end| at != 0 && end > at)?;
        let ticket = track(resource as usize, at, end);
        Some(Self { copy, layout, ticket })
    }

    pub(crate) fn owner(&self) -> usize {
        self.ticket.owner
    }
}

/// The subresources a read-back copies and where their rows sit in the
/// read-back buffer.
struct Texels {
    footprints: Vec<D3D12_PLACED_SUBRESOURCE_FOOTPRINT>,
    rows: Vec<u32>,
    mips: u32,
    /// Whether the copies are of a volume's mips (whose Z slices are inside
    /// each footprint), not of array layers.
    volume: bool,
    layout: victory_formats::texture::Layout,
}

impl Texels {
    /// The mip and array slice subresource `i` is.
    fn subresource(&self, i: u32) -> (u32, u32) {
        mip_and_slice(i, self.mips, self.volume)
    }

    /// Tiles every copied subresource of `mapped` into `tiled` (the title
    /// allocation's bytes): the elements of the copied subresources are
    /// replaced, every other byte of `tiled` is left as it is.
    fn tile_into(
        &self,
        tiled: &mut [u8],
        mapped: &[u8],
    ) -> Result<(), (usize, victory_formats::texture::Error)> {
        for (i, (footprint, &rows)) in self.footprints.iter().zip(&self.rows).enumerate() {
            let f = footprint.Footprint;
            let pitch = f.RowPitch as usize;
            let rows = rows as usize;
            let depth = f.Depth.max(1) as usize;
            let start = footprint.Offset as usize;
            let Some(len) = pitch.checked_mul(rows).and_then(|len| len.checked_mul(depth)) else {
                return Err((i, victory_formats::texture::Error::TooSmall));
            };
            let Some(linear) = start.checked_add(len).and_then(|end| mapped.get(start..end)) else {
                return Err((i, victory_formats::texture::Error::TooSmall));
            };
            let (mip, slice) = self.subresource(i as u32);
            self.layout
                .tile(tiled, mip, slice, linear, pitch, pitch.saturating_mul(rows))
                .map_err(|e| (i, e))?;
        }
        Ok(())
    }
}

/// Subresources a resource description has: one per mip and array layer. A
/// volume's Z slices are part of each mip's footprint, not array layers.
fn subresource_count(desc: &D3D12_RESOURCE_DESC) -> u32 {
    let layers = if desc.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D {
        1
    } else {
        u32::from(desc.DepthOrArraySize.max(1))
    };
    u32::from(desc.MipLevels.max(1)) * layers
}

/// The mip and array slice a subresource index names: subresources go mip by
/// mip within a slice. A volume has no slice beyond its mips.
fn mip_and_slice(i: u32, mips: u32, volume: bool) -> (u32, u32) {
    if volume {
        (i, 0)
    } else {
        (i % mips.max(1), i / mips.max(1))
    }
}

/// The read-back resource and the allocation the pixels belong to.
pub(crate) struct Back {
    buffer: ID3D12Resource,
    /// The bytes of `buffer` the copies use.
    bytes: u64,
    texels: Texels,
    ticket: Arc<Ticket>,
}

impl Back {
    pub(crate) fn range(&self) -> (usize, usize) {
        (self.ticket.at, self.ticket.end)
    }
}

/// Records a host copy from a texture already in COPY_SOURCE.
pub(crate) fn record(list: &ID3D12GraphicsCommandList, target: Target) -> Option<Back> {
    if !target.ticket.valid.load(Ordering::Acquire) {
        return None;
    }
    let host = crate::host::host()?;
    let texture = &target.copy.texture;
    // SAFETY: a live texture kept through the command list's state tracking.
    let desc = unsafe { texture.GetDesc() };
    let mips = u32::from(desc.MipLevels.max(1));
    let volume = desc.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D;
    let subresources = subresource_count(&desc);
    let n = subresources as usize;
    let mut footprints = vec![D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default(); n];
    let mut rows = vec![0u32; n];
    let mut total = 0u64;
    // SAFETY: out arrays of `n` entries.
    unsafe {
        host.device.GetCopyableFootprints(
            &desc,
            0,
            subresources,
            0,
            Some(footprints.as_mut_ptr()),
            Some(rows.as_mut_ptr()),
            None,
            Some(&mut total),
        )
    };
    // The last footprint's advertised extent can omit row-end padding. The
    // tiler reads a pitched slice, so the mapped buffer must include it.
    for (footprint, &rows) in footprints.iter().zip(&rows) {
        let format = footprint.Footprint;
        let pitched = u64::from(format.RowPitch)
            .checked_mul(u64::from(rows))?
            .checked_mul(u64::from(format.Depth.max(1)))?;
        total = total.max(footprint.Offset.checked_add(pitched)?);
    }
    let buffer = host.read_back_buffer(total)?;
    for (i, footprint) in footprints.iter().enumerate() {
        let to = D3D12_TEXTURE_COPY_LOCATION {
            // SAFETY: a borrowed pointer the location never releases.
            pResource: unsafe { std::mem::transmute_copy(&buffer) },
            Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { PlacedFootprint: *footprint },
        };
        let from = D3D12_TEXTURE_COPY_LOCATION {
            // SAFETY: as above.
            pResource: unsafe { std::mem::transmute_copy(texture) },
            Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { SubresourceIndex: i as u32 },
        };
        // SAFETY: an open list; the texture is in the copy-source state.
        unsafe { list.CopyTextureRegion(&to, 0, 0, 0, &from, None) };
    }
    let texels = Texels { footprints, rows, mips, volume, layout: target.layout };
    Some(Back { buffer, bytes: total, texels, ticket: target.ticket })
}

/// Tiles a copy's texels into `tiled` if its allocation is still the one the
/// copy was recorded against; false (and `tiled` untouched) otherwise.
/// Without a device, so that the invalidation can be tested.
fn land_bytes(ticket: &Ticket, tiled: &mut [u8], texels: &Texels, mapped: &[u8]) -> bool {
    if !ticket.valid.load(Ordering::Acquire) {
        return false;
    }
    match texels.tile_into(tiled, mapped) {
        Ok(()) => true,
        Err((i, e)) => {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| tracing::warn!(?e, subresource = i, "a texture copied back could not be tiled"));
            false
        }
    }
}

/// Once the list has run, tile its texels back into the same title allocation.
pub(crate) fn land(back: &Back) {
    static COPIED: AtomicU64 = AtomicU64::new(0);
    static STALE: AtomicU64 = AtomicU64::new(0);
    let _mapping = crate::watch::title_memory_reading();
    let _ticket_guard = TICKETS.lock().unwrap_or_else(|e| e.into_inner());
    if !back.ticket.valid.load(Ordering::Acquire) {
        let skipped = STALE.fetch_add(1, Ordering::Relaxed) + 1;
        if skipped.is_power_of_two() {
            tracing::info!(skipped, "skipping texture readbacks whose title allocation changed");
        }
        return;
    }
    let (at, end) = back.range();
    let size = end - at;
    let mut tiled = vec![0u8; size];
    // Keep title bytes the subresources do not cover (and any the title's
    // own code wrote since the copy was recorded).
    if !crate::host::read_into(at as *const u8, &mut tiled) {
        return;
    }
    let mut data = std::ptr::null_mut();
    // SAFETY: our read-back buffer, written by the list that has run.
    unsafe {
        if back.buffer.Map(0, None, Some(&mut data)).is_err() || data.is_null() {
            return;
        }
    }
    // SAFETY: the buffer's `bytes` are mapped by the call above.
    let mapped = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), back.bytes as usize) };
    let landed = land_bytes(&back.ticket, &mut tiled, &back.texels, mapped);
    // SAFETY: mapped above.
    unsafe { back.buffer.Unmap(0, None) };
    if !landed {
        return;
    }
    crate::host::copy_mapped(at as *mut u8, tiled.as_ptr(), size, true);
    let copied = COPIED.fetch_add(1, Ordering::Relaxed) + 1;
    if copied == 1 || copied.is_multiple_of(128) {
        tracing::info!(copied, bytes = size, "texture readback landed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use victory_formats::texture::{Dimension, Swizzle, TextureDesc};

    #[test]
    fn whole_readback_rejects_untracked_mips_and_array_slices() {
        assert!(whole_resource_is_one_subresource(1, 1));
        for (mips, layers) in [(0, 1), (1, 0), (2, 1), (1, 2), (3, 4)] {
            assert!(!whole_resource_is_one_subresource(mips, layers));
        }
    }

    #[test]
    fn a_new_alias_starting_inside_the_back_invalidates_its_ticket() {
        let first = track(901, 0x990000, 0x992000);
        let far = track(902, 0x994000, 0x995000);
        remapped(0x991000, 0x100);
        assert!(!first.valid.load(Ordering::Acquire));
        assert!(far.valid.load(Ordering::Acquire));
    }

    fn desc(dimension: D3D12_RESOURCE_DIMENSION, mips: u16, layers: u16) -> D3D12_RESOURCE_DESC {
        D3D12_RESOURCE_DESC {
            Dimension: dimension,
            MipLevels: mips,
            DepthOrArraySize: layers,
            ..Default::default()
        }
    }

    #[test]
    fn subresources_are_one_per_mip_and_array_layer() {
        assert_eq!(subresource_count(&desc(D3D12_RESOURCE_DIMENSION_TEXTURE2D, 1, 1)), 1);
        assert_eq!(subresource_count(&desc(D3D12_RESOURCE_DIMENSION_TEXTURE2D, 4, 3)), 12);
        assert_eq!(subresource_count(&desc(D3D12_RESOURCE_DIMENSION_TEXTURE2D, 6, 8)), 48);
        // A volume's Z slices live inside each mip's footprint.
        assert_eq!(subresource_count(&desc(D3D12_RESOURCE_DIMENSION_TEXTURE3D, 4, 8)), 4);
        assert_eq!(subresource_count(&desc(D3D12_RESOURCE_DIMENSION_TEXTURE3D, 1, 16)), 1);
    }

    #[test]
    fn a_volume_is_not_an_array_of_subresources() {
        let volume = desc(D3D12_RESOURCE_DIMENSION_TEXTURE3D, 4, 8);
        let array = desc(D3D12_RESOURCE_DIMENSION_TEXTURE2D, 4, 8);
        assert_eq!(subresource_count(&volume), 4);
        assert_eq!(subresource_count(&array), 32);
        // Every volume subresource is its own mip, none a later slice.
        for i in 0..subresource_count(&volume) {
            assert_eq!(mip_and_slice(i, 4, true), (i, 0));
        }
        // The array's are mip by mip within a slice.
        assert_eq!(mip_and_slice(0, 4, false), (0, 0));
        assert_eq!(mip_and_slice(3, 4, false), (3, 0));
        assert_eq!(mip_and_slice(4, 4, false), (0, 1));
        assert_eq!(mip_and_slice(31, 4, false), (3, 7));
    }

    #[test]
    fn kinds_are_matched_by_size_and_format() {
        let kinds = parse_kinds(DEFAULT);
        assert_eq!(kinds.len(), 4);
        assert!(matches(&kinds, (256, 256, 41)));
        assert!(matches(&kinds, (256, 256, 16)));
        assert!(matches(&kinds, (64, 64, 16)));
        assert!(matches(&kinds, (128, 128, 36)));
        assert!(!matches(&kinds, (256, 256, 36)));
        assert!(!matches(&kinds, (128, 128, 41)));
        // Entries that do not parse are skipped, not fatal.
        let odd = parse_kinds(" 256x256f41 , nonsense, 32x32, 8x8f10");
        assert_eq!(odd, vec![(256, 256, 41), (8, 8, 10)]);
        // An empty list never matches a kind.
        let none = parse_kinds("");
        assert!(none.is_empty());
        assert!(!matches(&none, (256, 256, 41)));
        assert!(!matches(&[], (64, 64, 16)));
    }

    #[test]
    fn released_owner_and_replaced_allocation_invalidate_only_their_backs() {
        // The ticket table is global to the process: each test's addresses
        // stay its own.
        let first = track(191, 0xc000, 0xc100);
        let next = track(192, 0xc200, 0xc300);
        let far = track(193, 0xc400, 0xc500);

        replaced(0xc200);
        assert!(first.valid.load(Ordering::Acquire));
        assert!(!next.valid.load(Ordering::Acquire));
        assert!(far.valid.load(Ordering::Acquire));

        released(191);
        assert!(!first.valid.load(Ordering::Acquire));
        assert!(far.valid.load(Ordering::Acquire));

        released(193);
        assert!(!far.valid.load(Ordering::Acquire));
        // A later write to the same allocation is its own ticket.
        assert!(track(194, 0xc200, 0xc300).valid.load(Ordering::Acquire));
    }

    #[test]
    fn remapping_invalidates_overlapping_backs_and_keeps_the_rest() {
        let first = track(291, 0x1000, 0x2000);
        let next = track(292, 0x2000, 0x3000);
        let far = track(293, 0x4000, 0x5000);

        // A remap that ends before the first allocation.
        remapped(0x0f00, 0x100);
        assert!(first.valid.load(Ordering::Acquire));
        assert!(next.valid.load(Ordering::Acquire));

        // One that touches only the first, ending where the next begins.
        remapped(0x1f00, 0x100);
        assert!(!first.valid.load(Ordering::Acquire));
        assert!(next.valid.load(Ordering::Acquire));
        assert!(far.valid.load(Ordering::Acquire));

        // A zero-length remap touches nothing.
        remapped(0x4000, 0);
        assert!(far.valid.load(Ordering::Acquire));
        // One that starts at the allocation's last byte does.
        let last = track(294, 0xf000, 0xf100);
        remapped(0xf0ff, 0x1);
        assert!(!last.valid.load(Ordering::Acquire));
        assert!(far.valid.load(Ordering::Acquire));
    }

    /// A linear R32_FLOAT texture's layout, as the console lays one out.
    fn layout_of(
        width: u32,
        height: u32,
        mips: u32,
        slices: u32,
        dimension: Dimension,
    ) -> victory_formats::texture::Layout {
        victory_formats::texture::Texture::new(&TextureDesc {
            dimension,
            width,
            height,
            depth_or_array: slices,
            mips,
            format: 41,
            samples: 1,
            swizzle: Swizzle::LINEAR,
        })
        .unwrap()
        .planes[0]
        .layout
        .clone()
    }

    fn footprint(offset: u64, row_pitch: u32, rows: u32, depth: u32) -> D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
        D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
            Offset: offset,
            Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                Format: Default::default(),
                Width: 0,
                Height: rows,
                Depth: depth,
                RowPitch: row_pitch,
            },
        }
    }

    /// One footprint per subresource, tightly packed in the read-back buffer,
    /// with the row pitch and rows of the mip the subresource is of (what
    /// `GetCopyableFootprints` gives).
    fn texels(
        layout: &victory_formats::texture::Layout,
        mips: u32,
        slices: u32,
    ) -> (Texels, Vec<u8>) {
        use victory_formats::texture::Dimension;
        let volume = layout.desc.dimension == Dimension::Tex3d;
        let n = (mips * slices) as usize;
        let mut footprints = Vec::with_capacity(n);
        let mut rows = Vec::with_capacity(n);
        let mut data = Vec::new();
        for i in 0..n {
            let (mip, _) = mip_and_slice(i as u32, mips, volume);
            let m = &layout.mips[mip as usize];
            let (row_pitch, height) =
                (m.pitch * layout.desc.element_bytes as u32, m.height);
            footprints.push(footprint(data.len() as u64, row_pitch, height, 1));
            rows.push(height);
            data.resize(data.len() + (row_pitch * height) as usize, 0);
        }
        let texels = Texels { footprints, rows, mips, volume, layout: layout.clone() };
        (texels, data)
    }

    #[test]
    fn pixels_land_only_while_the_ticket_is_valid() {
        let layout = layout_of(4, 4, 1, 1, Dimension::Tex2d);
        let (texels, mut data) = texels(&layout, 1, 1);
        data.iter_mut().for_each(|b| *b = 0xab);
        let size = layout.size as usize;

        let stale = track(95, 0x8000, 0x8000 + size);
        let mut tiled = vec![0x11u8; size];
        released(95);
        assert!(!land_bytes(&stale, &mut tiled, &texels, &data));
        assert!(tiled.iter().all(|&b| b == 0x11), "an invalidated copy left title memory alone");

        let live = track(96, 0x9000, 0x9000 + size);
        assert!(land_bytes(&live, &mut tiled, &texels, &data));
        // A linear mip's rows sit its own pitch apart; a copy carries the
        // texels of each row and nothing else.
        let mip = &layout.mips[0];
        let (row, pitch) = (mip.width as usize * 4, mip.pitch as usize * 4);
        let mut expected = vec![0x11u8; layout.size as usize];
        for y in 0..mip.height as usize {
            expected[y * pitch..y * pitch + row].fill(0xab);
        }
        assert!(tiled == expected, "a valid copy landed its pixels, and only those");
    }

    #[test]
    fn every_subresource_of_a_mip_chain_and_its_slices_is_tiled() {
        let layout = layout_of(4, 4, 2, 2, Dimension::Tex2d);
        let (texels, mut data) = texels(&layout, 2, 2);
        // Each subresource's rows carry its own byte value.
        for (i, footprint) in texels.footprints.iter().enumerate() {
            let start = footprint.Offset as usize;
            let len = (footprint.Footprint.RowPitch * footprint.Footprint.Height) as usize;
            data[start..start + len].iter_mut().for_each(|b| *b = 0x10 + i as u8);
        }
        let mut tiled = vec![0u8; layout.size as usize];
        let ticket = track(97, 0xa000, 0xa000 + layout.size as usize);
        assert!(land_bytes(&ticket, &mut tiled, &texels, &data));

        for (i, mip) in layout.mips.iter().enumerate() {
            for slice in 0..2usize {
                let at = slice * layout.slice_size as usize + mip.offset as usize;
                assert_eq!(
                    tiled[at],
                    0x10 + i as u8 + (slice * 2) as u8,
                    "mip {i} slice {slice} is its own copy"
                );
            }
        }
    }

    #[test]
    fn a_newer_cpu_write_outside_the_copied_subresources_survives_landing() {
        // Two mips of a two-slice array: the read-back covers mip 0 only, as
        // if the GPU's write had stopped there.
        let layout = layout_of(4, 4, 2, 2, Dimension::Tex2d);
        let (mut texels, mut data) = texels(&layout, 2, 2);
        texels.footprints.truncate(1);
        texels.rows.truncate(1);
        texels.mips = 2;
        let covered = (texels.footprints[0].Footprint.RowPitch * texels.footprints[0].Footprint.Height) as usize;
        data.truncate(covered);
        data.iter_mut().for_each(|b| *b = 0x5a);

        let ticket = track(98, 0xb000, 0xb000 + layout.size as usize);
        // What the title's own code wrote into the allocation after the copy
        // was recorded (the second mip and everything of the second slice).
        let mut tiled = vec![0xcc; layout.size as usize];
        assert!(land_bytes(&ticket, &mut tiled, &texels, &data));

        let mip0 = layout.mips[0].offset as usize;
        assert_eq!(tiled[mip0], 0x5a, "the copied subresource landed");
        for (i, mip) in layout.mips.iter().enumerate().skip(1) {
            for slice in 0..2usize {
                let at = slice * layout.slice_size as usize + mip.offset as usize;
                assert_eq!(tiled[at], 0xcc, "mip {i} slice {slice} kept the newer CPU bytes");
            }
        }
        let second = layout.slice_size as usize + mip0;
        assert_eq!(tiled[second], 0xcc, "the second slice kept the newer CPU bytes");
    }
}
