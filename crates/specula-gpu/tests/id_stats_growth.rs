//! The ID buffer's per-id stats buffer grows with the highest id: an ID render whose highest id
//! crosses the size the last buffer was made for must get a big enough one. It once kept a
//! buffer that was an alignment short (its memory's size, rounded up, was compared instead of
//! the size the buffer was made with): the stats clear and readback ran one id's stride past its
//! end (review, 2026-10-29: VUID-vkCmdFillBuffer-size-00027 and vkCmdCopyBuffer-size-00115/00116
//! in a Sepulcra capture, 213 then 214 ids: 5,992 bytes into 5,964). A first render at an id
//! count that isn't a round size, then the next few, under validation, must be clean.
//! Skips cleanly without Vulkan.

mod common;

use common::*;
use specula_gpu::{AdapterPreference, DrawItem, FrameDesc};

#[test]
fn growing_id_counts_never_overrun_the_stats_buffer() {
    let test = "id_stats_growth";
    // Every adapter (the Intel iGPU rounds buffer memory up further than the discrete GPU: that
    // is where it showed), each start on a fresh device, so the first stats buffer is made at
    // exactly that size.
    let adapters = match specula_gpu::enumerate_adapters() {
        Ok(a) if !a.is_empty() => a,
        _ => {
            eprintln!("{test}: SKIPPED: no Vulkan adapters");
            return;
        }
    };
    for (a, first) in adapters.iter().flat_map(|a| [213u32, 97, 301].map(|f| (a.index, f))) {
        let Some(mut gpu) = gpu_or_skip(test, AdapterPreference::Index(a)) else { return };
        if !gpu.ids_count_blended() {
            eprintln!("{test}: SKIPPED: no fragment atomics (no stats buffer)");
            return;
        }
        let (cv, ci) = cube();
        let cube = gpu.upload_mesh(&cv, &ci).unwrap();
        let target = gpu.create_target(32, 32).unwrap();
        let view_proj = mul(&perspective_vk_reverse_z(1.0, 1.0, 0.05), &look_at_rh([0.0, 0.0, 4.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
        let draw = [DrawItem::new(cube, IDENTITY, [0.5, 0.5, 0.5, 1.0])];
        let frame = FrameDesc { view_proj, draws: &draw, reverse_z: true, ..Default::default() };
        // One draw with id `n`: the highest id is `n`, so the stats buffer needs n strides.
        for n in first..first + 4 {
            gpu.render_with_ids(target, &frame, &[n]).unwrap();
            let cov = gpu.id_coverage(target, n).unwrap();
            assert!(cov.per_id[n as usize - 1].pixels > 0, "{test}: id {n} counted");
            let msgs: Vec<_> = gpu.take_validation_messages().into_iter().filter(|m| m.severity >= specula_gpu::Severity::Warning).collect();
            assert!(msgs.is_empty(), "{test}: adapter {a}, first {first}, highest id {n}: {msgs:#?}");
        }
        gpu.destroy_target(target);
        let errors = validation_errors(test, gpu);
        assert!(errors.is_empty(), "{test}: {errors:#?}");
    }
}
