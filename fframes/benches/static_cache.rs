use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use fframes::Svgr;
use fframes::usvgr::{self, fontdb};
use svgr::{Context, PixmapPool, SvgrCache, render, tiny_skia};

/// Simple SVG with small static paths (below caching threshold)
fn bench_static_paths(c: &mut Criterion) {
    let mut group = c.benchmark_group("static_paths_small");

    let fontdb = fontdb::Database::new();
    let pixmap_pool = PixmapPool::new();
    let usvg_options = usvgr::Options::default();
    let mut converter_cache = usvgr::Cache::default();

    // Small rects - below caching threshold (100x100 = 10000 < 40000)
    let svg: Svgr = fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000">
            <rect x="0" y="0" width="100" height="100" fill="red"/>
            <rect x="100" y="0" width="100" height="100" fill="green"/>
            <rect x="200" y="0" width="100" height="100" fill="blue"/>
            <rect x="300" y="0" width="100" height="100" fill="yellow"/>
            <rect x="0" y="100" width="100" height="100" fill="cyan"/>
            <rect x="100" y="100" width="100" height="100" fill="magenta"/>
            <rect x="200" y="100" width="100" height="100" fill="orange"/>
            <rect x="300" y="100" width="100" height="100" fill="purple"/>
            <rect x="0" y="200" width="100" height="100" fill="pink"/>
            <rect x="100" y="200" width="100" height="100" fill="brown"/>
            <rect x="200" y="200" width="100" height="100" fill="gray"/>
            <rect x="300" y="200" width="100" height="100" fill="black"/>
        </svg>
    );

    let tree = svg
        .into_svg_tree(&usvg_options, &mut converter_cache, &fontdb)
        .unwrap();
    let size = tree.size();
    let pixmap_for_ctx = tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();
    let ctx = Context::new_from_pixmap(&pixmap_for_ctx);

    group.bench_function("with_static_cache", |b| {
        let mut cache = SvgrCache::new(100);
        let mut pixmap = tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();

        b.iter(|| {
            pixmap.fill(tiny_skia::Color::TRANSPARENT);
            render(
                black_box(&tree),
                tiny_skia::Transform::default(),
                &mut pixmap.as_mut(),
                &mut cache,
                &pixmap_pool,
                &ctx,
            );
        });
    });

    group.bench_function("no_cache", |b| {
        let mut cache = SvgrCache::none();
        let mut pixmap = tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();

        b.iter(|| {
            pixmap.fill(tiny_skia::Color::TRANSPARENT);
            render(
                black_box(&tree),
                tiny_skia::Transform::default(),
                &mut pixmap.as_mut(),
                &mut cache,
                &pixmap_pool,
                &ctx,
            );
        });
    });

    group.finish();
}

/// SVG with large static paths (above caching threshold)
fn bench_large_static_paths(c: &mut Criterion) {
    let mut group = c.benchmark_group("static_paths_large");

    let fontdb = fontdb::Database::new();
    let pixmap_pool = PixmapPool::new();
    let usvg_options = usvgr::Options::default();
    let mut converter_cache = usvgr::Cache::default();

    // Large rects - above caching threshold (300x300 = 90000 > 40000)
    let svg: Svgr = fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080">
            <rect x="0" y="0" width="300" height="300" fill="red"/>
            <rect x="320" y="0" width="300" height="300" fill="green"/>
            <rect x="640" y="0" width="300" height="300" fill="blue"/>
            <rect x="960" y="0" width="300" height="300" fill="yellow"/>
            <rect x="0" y="320" width="300" height="300" fill="cyan"/>
            <rect x="320" y="320" width="300" height="300" fill="magenta"/>
            <rect x="640" y="320" width="300" height="300" fill="orange"/>
            <rect x="960" y="320" width="300" height="300" fill="purple"/>
        </svg>
    );

    let tree = svg
        .into_svg_tree(&usvg_options, &mut converter_cache, &fontdb)
        .unwrap();
    let size = tree.size();
    let pixmap_for_ctx = tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();
    let ctx = Context::new_from_pixmap(&pixmap_for_ctx);

    group.bench_function("with_static_cache", |b| {
        let mut cache = SvgrCache::new(100);
        let mut pixmap = tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();

        b.iter(|| {
            pixmap.fill(tiny_skia::Color::TRANSPARENT);
            render(
                black_box(&tree),
                tiny_skia::Transform::default(),
                &mut pixmap.as_mut(),
                &mut cache,
                &pixmap_pool,
                &ctx,
            );
        });
    });

    group.bench_function("no_cache", |b| {
        let mut cache = SvgrCache::none();
        let mut pixmap = tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();

        b.iter(|| {
            pixmap.fill(tiny_skia::Color::TRANSPARENT);
            render(
                black_box(&tree),
                tiny_skia::Transform::default(),
                &mut pixmap.as_mut(),
                &mut cache,
                &pixmap_pool,
                &ctx,
            );
        });
    });

    group.finish();
}

/// Large static groups without isolation - NOW CACHED with new implementation
fn bench_large_static_groups(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_static_groups");

    let fontdb = fontdb::Database::new();
    let pixmap_pool = PixmapPool::new();
    let usvg_options = usvgr::Options::default();
    let mut converter_cache = usvgr::Cache::default();

    // Large static groups (above 10000 pixel threshold) - NOW CACHED
    let svg: Svgr = fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080">
            <g>
                <rect x="0" y="0" width="400" height="400" fill="red"/>
                <rect x="400" y="0" width="400" height="400" fill="green"/>
                <rect x="800" y="0" width="400" height="400" fill="blue"/>
                <rect x="1200" y="0" width="400" height="400" fill="yellow"/>
                <rect x="0" y="400" width="400" height="400" fill="cyan"/>
                <rect x="400" y="400" width="400" height="400" fill="magenta"/>
                <rect x="800" y="400" width="400" height="400" fill="orange"/>
                <rect x="1200" y="400" width="400" height="400" fill="purple"/>
            </g>
        </svg>
    );

    let tree = svg
        .into_svg_tree(&usvg_options, &mut converter_cache, &fontdb)
        .unwrap();
    let size = tree.size();
    let pixmap_for_ctx = tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();
    let ctx = Context::new_from_pixmap(&pixmap_for_ctx);

    group.bench_function("with_cache", |b| {
        let mut cache = SvgrCache::new(100);
        let mut pixmap = tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();

        b.iter(|| {
            pixmap.fill(tiny_skia::Color::TRANSPARENT);
            render(
                black_box(&tree),
                tiny_skia::Transform::default(),
                &mut pixmap.as_mut(),
                &mut cache,
                &pixmap_pool,
                &ctx,
            );
        });
    });

    group.bench_function("no_cache", |b| {
        let mut cache = SvgrCache::none();
        let mut pixmap = tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();

        b.iter(|| {
            pixmap.fill(tiny_skia::Color::TRANSPARENT);
            render(
                black_box(&tree),
                tiny_skia::Transform::default(),
                &mut pixmap.as_mut(),
                &mut cache,
                &pixmap_pool,
                &ctx,
            );
        });
    });

    group.finish();
}

/// Multi-frame rendering of large static groups - Shows the real benefit
fn bench_multi_frame_static_groups(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_frame_static_groups");

    let fontdb = fontdb::Database::new();
    let pixmap_pool = PixmapPool::new();
    let usvg_options = usvgr::Options::default();
    let mut converter_cache = usvgr::Cache::default();

    // Large static groups - now cacheable
    let svg: Svgr = fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080">
            <g>
                <rect x="0" y="0" width="400" height="400" fill="red"/>
                <rect x="400" y="0" width="400" height="400" fill="green"/>
                <rect x="800" y="0" width="400" height="400" fill="blue"/>
                <rect x="1200" y="0" width="400" height="400" fill="yellow"/>
                <rect x="0" y="400" width="400" height="400" fill="cyan"/>
                <rect x="400" y="400" width="400" height="400" fill="magenta"/>
                <rect x="800" y="400" width="400" height="400" fill="orange"/>
                <rect x="1200" y="400" width="400" height="400" fill="purple"/>
            </g>
        </svg>
    );

    let tree = svg
        .into_svg_tree(&usvg_options, &mut converter_cache, &fontdb)
        .unwrap();
    let size = tree.size();
    let pixmap_for_ctx = tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();
    let ctx = Context::new_from_pixmap(&pixmap_for_ctx);

    let frame_count = 30;

    group.bench_function(BenchmarkId::new("with_cache", frame_count), |b| {
        b.iter(|| {
            let mut cache = SvgrCache::new(100);
            let mut pixmap =
                tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();

            for _ in 0..frame_count {
                pixmap.fill(tiny_skia::Color::TRANSPARENT);
                render(
                    black_box(&tree),
                    tiny_skia::Transform::default(),
                    &mut pixmap.as_mut(),
                    &mut cache,
                    &pixmap_pool,
                    &ctx,
                );
            }
        });
    });

    group.bench_function(BenchmarkId::new("no_cache", frame_count), |b| {
        b.iter(|| {
            let mut cache = SvgrCache::none();
            let mut pixmap =
                tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();

            for _ in 0..frame_count {
                pixmap.fill(tiny_skia::Color::TRANSPARENT);
                render(
                    black_box(&tree),
                    tiny_skia::Transform::default(),
                    &mut pixmap.as_mut(),
                    &mut cache,
                    &pixmap_pool,
                    &ctx,
                );
            }
        });
    });

    group.finish();
}

/// SVG with isolated groups (opacity < 1) - CACHING SHOULD WORK
fn bench_isolated_groups_opacity(c: &mut Criterion) {
    let mut group = c.benchmark_group("isolated_groups_opacity");

    let fontdb = fontdb::Database::new();
    let pixmap_pool = PixmapPool::new();
    let usvg_options = usvgr::Options::default();
    let mut converter_cache = usvgr::Cache::default();

    // Groups with opacity < 1 trigger isolation and caching
    let svg: Svgr = fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000">
            <g opacity="0.9">
                <rect x="0" y="0" width="100" height="100" fill="red"/>
                <rect x="100" y="0" width="100" height="100" fill="green"/>
                <rect x="200" y="0" width="100" height="100" fill="blue"/>
                <rect x="300" y="0" width="100" height="100" fill="yellow"/>
            </g>
            <g opacity="0.8">
                <rect x="0" y="100" width="100" height="100" fill="cyan"/>
                <rect x="100" y="100" width="100" height="100" fill="magenta"/>
                <rect x="200" y="100" width="100" height="100" fill="orange"/>
                <rect x="300" y="100" width="100" height="100" fill="purple"/>
            </g>
            <g opacity="0.7">
                <rect x="0" y="200" width="100" height="100" fill="pink"/>
                <rect x="100" y="200" width="100" height="100" fill="brown"/>
                <rect x="200" y="200" width="100" height="100" fill="gray"/>
                <rect x="300" y="200" width="100" height="100" fill="black"/>
            </g>
        </svg>
    );

    let tree = svg
        .into_svg_tree(&usvg_options, &mut converter_cache, &fontdb)
        .unwrap();
    let size = tree.size();
    let pixmap_for_ctx = tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();
    let ctx = Context::new_from_pixmap(&pixmap_for_ctx);

    group.bench_function("with_cache", |b| {
        let mut cache = SvgrCache::new(100);
        let mut pixmap = tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();

        b.iter(|| {
            pixmap.fill(tiny_skia::Color::TRANSPARENT);
            render(
                black_box(&tree),
                tiny_skia::Transform::default(),
                &mut pixmap.as_mut(),
                &mut cache,
                &pixmap_pool,
                &ctx,
            );
        });
    });

    group.bench_function("no_cache", |b| {
        let mut cache = SvgrCache::none();
        let mut pixmap = tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();

        b.iter(|| {
            pixmap.fill(tiny_skia::Color::TRANSPARENT);
            render(
                black_box(&tree),
                tiny_skia::Transform::default(),
                &mut pixmap.as_mut(),
                &mut cache,
                &pixmap_pool,
                &ctx,
            );
        });
    });

    group.finish();
}

/// SVG with filters - CACHING SHOULD HAVE BIG IMPACT
fn bench_filtered_groups(c: &mut Criterion) {
    let mut group = c.benchmark_group("filtered_groups");

    let fontdb = fontdb::Database::new();
    let pixmap_pool = PixmapPool::new();
    let usvg_options = usvgr::Options::default();
    let mut converter_cache = usvgr::Cache::default();

    // Groups with blur filter - expensive to render, should benefit from cache
    let svg: Svgr = fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000">
            <defs>
                <filter id="blur1">
                    <feGaussianBlur stdDeviation="2"/>
                </filter>
                <filter id="blur2">
                    <feGaussianBlur stdDeviation="4"/>
                </filter>
            </defs>
            <g filter="url(#blur1)">
                <rect x="0" y="0" width="200" height="200" fill="red"/>
                <rect x="200" y="0" width="200" height="200" fill="green"/>
            </g>
            <g filter="url(#blur2)">
                <rect x="0" y="200" width="200" height="200" fill="blue"/>
                <rect x="200" y="200" width="200" height="200" fill="yellow"/>
            </g>
        </svg>
    );

    let tree = svg
        .into_svg_tree(&usvg_options, &mut converter_cache, &fontdb)
        .unwrap();
    let size = tree.size();
    let pixmap_for_ctx = tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();
    let ctx = Context::new_from_pixmap(&pixmap_for_ctx);

    group.bench_function("with_cache", |b| {
        let mut cache = SvgrCache::new(100);
        let mut pixmap = tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();

        b.iter(|| {
            pixmap.fill(tiny_skia::Color::TRANSPARENT);
            render(
                black_box(&tree),
                tiny_skia::Transform::default(),
                &mut pixmap.as_mut(),
                &mut cache,
                &pixmap_pool,
                &ctx,
            );
        });
    });

    group.bench_function("no_cache", |b| {
        let mut cache = SvgrCache::none();
        let mut pixmap = tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();

        b.iter(|| {
            pixmap.fill(tiny_skia::Color::TRANSPARENT);
            render(
                black_box(&tree),
                tiny_skia::Transform::default(),
                &mut pixmap.as_mut(),
                &mut cache,
                &pixmap_pool,
                &ctx,
            );
        });
    });

    group.finish();
}

/// Multi-frame rendering simulation with isolated groups
fn bench_multi_frame_isolated(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_frame_isolated");

    let fontdb = fontdb::Database::new();
    let pixmap_pool = PixmapPool::new();
    let usvg_options = usvgr::Options::default();
    let mut converter_cache = usvgr::Cache::default();

    let svg: Svgr = fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080">
            <defs>
                <filter id="shadow">
                    <feDropShadow dx="2" dy="2" stdDeviation="3"/>
                </filter>
            </defs>
            <g opacity="0.95">
                <rect x="0" y="0" width="1920" height="1080" fill="#1a1a2e"/>
            </g>
            <g filter="url(#shadow)">
                <circle cx="200" cy="540" r="80" fill="#e94560"/>
                <circle cx="400" cy="540" r="80" fill="#0f3460"/>
                <circle cx="600" cy="540" r="80" fill="#16213e"/>
            </g>
            <g opacity="0.9">
                <rect x="800" y="400" width="300" height="200" fill="#533483" rx="10"/>
                <rect x="1120" y="400" width="300" height="200" fill="#e94560" rx="10"/>
                <rect x="1440" y="400" width="300" height="200" fill="#0f3460" rx="10"/>
            </g>
        </svg>
    );

    let tree = svg
        .into_svg_tree(&usvg_options, &mut converter_cache, &fontdb)
        .unwrap();
    let size = tree.size();
    let pixmap_for_ctx = tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();
    let ctx = Context::new_from_pixmap(&pixmap_for_ctx);

    let frame_count = 30;

    group.bench_function(BenchmarkId::new("with_cache", frame_count), |b| {
        b.iter(|| {
            let mut cache = SvgrCache::new(100);
            let mut pixmap =
                tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();

            for _ in 0..frame_count {
                pixmap.fill(tiny_skia::Color::TRANSPARENT);
                render(
                    black_box(&tree),
                    tiny_skia::Transform::default(),
                    &mut pixmap.as_mut(),
                    &mut cache,
                    &pixmap_pool,
                    &ctx,
                );
            }
        });
    });

    group.bench_function(BenchmarkId::new("no_cache", frame_count), |b| {
        b.iter(|| {
            let mut cache = SvgrCache::none();
            let mut pixmap =
                tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();

            for _ in 0..frame_count {
                pixmap.fill(tiny_skia::Color::TRANSPARENT);
                render(
                    black_box(&tree),
                    tiny_skia::Transform::default(),
                    &mut pixmap.as_mut(),
                    &mut cache,
                    &pixmap_pool,
                    &ctx,
                );
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_static_paths,
    bench_large_static_paths,
    bench_large_static_groups,
    bench_multi_frame_static_groups,
    bench_isolated_groups_opacity,
    bench_filtered_groups,
    bench_multi_frame_isolated
);
criterion_main!(benches);
