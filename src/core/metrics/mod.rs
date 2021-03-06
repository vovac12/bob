use super::prelude::*;

mod exporter;

/// Counts number of PUT requests, processed by Grinder
pub const GRINDER_PUT_COUNTER: &str = "grinder.put_count";
/// Counts number of PUT requests return error, processed by Grinder
pub const GRINDER_PUT_ERROR_COUNT_COUNTER: &str = "grinder.put_error_count";
/// Measures processing time of the PUT request
pub const GRINDER_PUT_TIMER: &str = "grinder.put_timer";

/// Counts number of GET requests, processed by Grinder
pub const GRINDER_GET_COUNTER: &str = "grinder.get_count";
/// Counts number of GET requests return error, processed by Grinder
pub const GRINDER_GET_ERROR_COUNT_COUNTER: &str = "grinder.get_error_count";
/// Measures processing time of the GET request
pub const GRINDER_GET_TIMER: &str = "grinder.get_timer";

/// Counts number of EXIST requests, processed by Grinder
pub const GRINDER_EXIST_COUNTER: &str = "grinder.exist_count";
/// Counts number of EXIST requests return error, processed by Grinder
pub const GRINDER_EXIST_ERROR_COUNT_COUNTER: &str = "grinder.exist_error_count";
/// Measures processing time of the EXIST request
pub const GRINDER_EXIST_TIMER: &str = "grinder.exist_timer";

/// Counts number of PUT requests, processed by Client
pub const CLIENT_PUT_COUNTER: &str = "client.put_count";
/// Counts number of PUT requests return error, processed by Client
pub const CLIENT_PUT_ERROR_COUNT_COUNTER: &str = "client.put_error_count";
/// Measures processing time of the PUT request
pub const CLIENT_PUT_TIMER: &str = "client.put_timer";

/// Counts number of GET requests, processed by Client
pub const CLIENT_GET_COUNTER: &str = "client.get_count";
/// Counts number of GET requests return error, processed by Client
pub const CLIENT_GET_ERROR_COUNT_COUNTER: &str = "client.get_error_count";
/// Measures processing time of the GET request
pub const CLIENT_GET_TIMER: &str = "client.get_timer";

/// Counts number of EXIST requests, processed by Client
pub const CLIENT_EXIST_COUNTER: &str = "client.exist_count";
/// Counts number of EXIST requests return error, processed by Client
pub const CLIENT_EXIST_ERROR_COUNT_COUNTER: &str = "client.exist_error_count";
/// Measures processing time of the EXIST request
pub const CLIENT_EXIST_TIMER: &str = "client.exist_timer";

/// Observes number of connected nodes
pub const AVAILABLE_NODES_COUNT: &str = "link_manager.nodes_number";

/// Observes if bob has started already
pub const BACKEND_STATE: &str = "backend.backend_state";
/// Count blobs (without aliens)
pub const BLOBS_COUNT: &str = "backend.blob_count";
/// Count alien blobs
pub const ALIEN_BLOBS_COUNT: &str = "backend.alien_count";
/// Count memory occupied by indices
pub const INDEX_MEMORY: &str = "backend.index_memory";

/// Type to measure time of requests processing
pub type Timer = Instant;

/// Structure contains put/get metrics for `BobClient`
#[derive(Debug, Clone)]
pub struct BobClient {
    prefix: String,
}

impl BobClient {
    fn new(prefix: String) -> Self {
        BobClient { prefix }
    }

    pub(crate) fn put_count(&self) {
        counter!(self.prefix.clone() + ".put_count", 1);
    }

    pub(crate) fn start_timer() -> Timer {
        Instant::now()
    }

    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn put_timer_stop(&self, timer: Timer) {
        timing!(
            self.prefix.clone() + ".put_timer",
            timer.elapsed().as_nanos() as u64
        );
    }

    pub(crate) fn put_error_count(&self) {
        counter!(self.prefix.clone() + ".put_error_count", 1);
    }

    pub(crate) fn get_count(&self) {
        counter!(self.prefix.clone() + ".get_count", 1);
    }

    pub(crate) fn get_timer_stop(&self, timer: Timer) {
        timing!(
            self.prefix.clone() + ".get_timer",
            timer.elapsed().as_nanos() as u64
        );
    }

    pub(crate) fn get_error_count(&self) {
        counter!(self.prefix.clone() + ".get_error_count", 1);
    }

    pub(crate) fn exist_count(&self) {
        counter!(self.prefix.clone() + ".exist_count", 1);
    }

    pub(crate) fn exist_error_count(&self) {
        counter!(self.prefix.clone() + ".exist_error_count", 1);
    }

    pub(crate) fn exist_timer_stop(&self, timer: Timer) {
        timing!(
            self.prefix.clone() + ".exist_timer",
            timer.elapsed().as_nanos() as u64
        );
    }
}

#[derive(Debug, Clone)]
struct MetricsContainer {
    duration: Duration,
    prefix: String,
}

impl MetricsContainer {
    pub(crate) fn new(duration: Duration, prefix: String) -> Self {
        MetricsContainer { duration, prefix }
    }
}

/// A trait for generic metrics builders
pub trait ContainerBuilder {
    /// Initializes `BobClient` container with given name
    fn get_metrics(&self, name: &str) -> BobClient;
}

impl ContainerBuilder for MetricsContainer {
    fn get_metrics(&self, name: &str) -> BobClient {
        let prefix = self.prefix.clone() + "." + name;
        BobClient::new(prefix)
    }
}

/// initializes bob counters with given config and address of the local node
pub fn init_counters(
    node_config: &NodeConfig,
    local_address: &str,
) -> Arc<dyn ContainerBuilder + Send + Sync> {
    let prefix = local_address;
    exporter::GraphiteBuilder::new()
        .set_address(node_config.metrics().graphite().to_string())
        .set_interval(Duration::from_secs(1))
        .install()
        .expect("Can't install metrics");
    let container = MetricsContainer::new(Duration::from_secs(1), prefix.to_string());
    info!(
        "metrics container initialized with update interval: {}ms",
        container.duration.as_millis()
    );
    let metrics = Arc::new(container);
    init_grinder();
    init_bob_client();
    init_backend();
    init_link_manager();
    init_pearl();
    metrics
}

fn init_grinder() {
    counter!(GRINDER_GET_COUNTER, 0);
    counter!(GRINDER_PUT_COUNTER, 0);
    counter!(GRINDER_EXIST_COUNTER, 0);
    counter!(GRINDER_GET_ERROR_COUNT_COUNTER, 0);
    counter!(GRINDER_PUT_ERROR_COUNT_COUNTER, 0);
    counter!(GRINDER_EXIST_ERROR_COUNT_COUNTER, 0);
}

fn init_backend() {
    counter!(BACKEND_STATE, 0);
    counter!(BLOBS_COUNT, 0);
    counter!(ALIEN_BLOBS_COUNT, 0);
}

fn init_link_manager() {
    counter!(AVAILABLE_NODES_COUNT, 0);
}

fn init_bob_client() {
    counter!(CLIENT_GET_COUNTER, 0);
    counter!(CLIENT_PUT_COUNTER, 0);
    counter!(CLIENT_EXIST_COUNTER, 0);
    counter!(CLIENT_GET_ERROR_COUNT_COUNTER, 0);
    counter!(CLIENT_PUT_ERROR_COUNT_COUNTER, 0);
    counter!(CLIENT_EXIST_ERROR_COUNT_COUNTER, 0);
}
