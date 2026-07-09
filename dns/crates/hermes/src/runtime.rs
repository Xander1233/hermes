use anyhow::Context;
use hermes_config::NetworkConfig;
use tokio::runtime::Runtime;

const WORKERS_PER_CORE: usize = 0;

pub fn build(network: &NetworkConfig) -> anyhow::Result<Runtime> {
    let worker_threads = match network.workers {
        WORKERS_PER_CORE => std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
        configured => configured,
    };

    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads.max(1))
        .enable_all()
        .build()
        .context("building the Tokio runtime")
}
