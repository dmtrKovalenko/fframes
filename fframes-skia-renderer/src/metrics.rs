use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    Arc,
};

// Metrics structures
#[derive(Debug)]
pub(crate) struct ThreadMetrics {
    pub(crate) channel_wait_time: AtomicU64, // Timewaiting for messages in microseconds
    pub(crate) total_time: AtomicU64,        // Total running time in microseconds
    pub(crate) items_processed: AtomicU64,
    pub(crate) thread_count: AtomicUsize,
}

impl ThreadMetrics {
    fn new(thread_count: usize) -> Self {
        Self {
            channel_wait_time: AtomicU64::default(),
            total_time: AtomicU64::default(),
            items_processed: AtomicU64::default(),
            thread_count: AtomicUsize::new(thread_count),
        }
    }
}

pub(crate) struct PipelineMetrics {
    pub(crate) generator_metrics: Arc<ThreadMetrics>,
    pub(crate) renderer_metrics: Arc<ThreadMetrics>,
    pub(crate) encoder_metrics: Arc<ThreadMetrics>,
}

impl PipelineMetrics {
    pub(crate) fn new(generator_threads: usize) -> Self {
        Self {
            generator_metrics: Arc::new(ThreadMetrics::new(generator_threads)),
            renderer_metrics: Arc::new(ThreadMetrics::new(1)),
            encoder_metrics: Arc::new(ThreadMetrics::new(1)),
        }
    }

    pub(crate) fn print_stats(&self) {
        println!("\nPipeline Thread Channel Wait Times (averaged per thread):");
        self.print_thread_stats("Frame Generator", &self.generator_metrics);
        self.print_thread_stats("Renderer", &self.renderer_metrics);
        self.print_thread_stats("Encoder", &self.encoder_metrics);
    }

    pub(crate) fn print_thread_stats(&self, name: &str, metrics: &ThreadMetrics) {
        let total = metrics.total_time.load(Ordering::Relaxed);
        let thread_count = metrics.thread_count.load(Ordering::Relaxed);

        if total > 0 && thread_count > 0 {
            let wait_time = metrics.channel_wait_time.load(Ordering::Relaxed);
            let avg_total = total / thread_count as u64;
            let avg_wait = wait_time / thread_count as u64;

            println!(
                "{} ({} threads): {:.2}s time waiting for messages ({} frames done in averaging {:.2}s)",
                name,
                thread_count,
                avg_wait as f64 / 1_000_000.,
                metrics.items_processed.load(Ordering::Relaxed),
                avg_total as f64 / 1_000_000.
            );
        }
    }
}
