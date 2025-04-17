use criterion::{Criterion, black_box, criterion_group, criterion_main};
use fframes::pix_fmt::{fill_yuv420_from_rgba_pixmap_accelerated, fill_yuv420_from_rgba_pixmap_base};

fn create_test_data(width: i32, height: i32) -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
    let size = (width * height * 4) as usize;
    let frame_size = (height * width) as usize;

    let rgba_pixels = (0..size).map(|_| rand::random::<u8>()).collect();

    let y_pixels = vec![0u8; frame_size];
    let cb_pixels = vec![0u8; frame_size / 4];
    let cr_pixels = vec![0u8; frame_size / 4];

    (rgba_pixels, y_pixels, cb_pixels, cr_pixels)
}

fn benchmark_yuv420_conversion(c: &mut Criterion) {
    let mut group = c.benchmark_group("YUV420 Conversion");

    for &(width, height) in &[(320, 240), (640, 480), (1280, 720), (1920, 1080)] {
        let (rgba_pixels, mut y_pixels, mut cb_pixels, mut cr_pixels) =
            create_test_data(width, height);

        group.bench_function(format!("base_{}x{}", width, height), |b| {
            b.iter(|| {
                fill_yuv420_from_rgba_pixmap_base(
                    black_box(width),
                    black_box(height),
                    black_box(width),     // y_linesize
                    black_box(width / 2), // cb_linesize
                    black_box(width / 2), // cr_linesize
                    black_box(&rgba_pixels),
                    black_box(y_pixels.as_mut_ptr()),
                    black_box(cb_pixels.as_mut_ptr()),
                    black_box(cr_pixels.as_mut_ptr()),
                )
            })
        });

        #[cfg(target_arch = "aarch64")]
        {
            group.bench_function(format!("neon_{}x{}", width, height), |b| {
                b.iter(|| {
                    unsafe {
                        fill_yuv420_from_rgba_pixmap_accelerated(
                            black_box(width),
                            black_box(height),
                            black_box(width),     // y_linesize
                            black_box(width / 2), // cb_linesize
                            black_box(width / 2), // cr_linesize
                            black_box(&rgba_pixels),
                            black_box(y_pixels.as_mut_ptr()),
                            black_box(cb_pixels.as_mut_ptr()),
                            black_box(cr_pixels.as_mut_ptr()),
                        )
                    }
                })
            });
        }
    }

    group.finish();
}

criterion_group!(benches, benchmark_yuv420_conversion);
criterion_main!(benches);
