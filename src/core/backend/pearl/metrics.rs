use super::prelude::*;

metrics! {
    PEARL: Proxy = "pearl" => {
        pub PEARL_PUT_COUNTER: Counter = "put_count";
        pub PEARL_PUT_ERROR_COUNTER: Counter = "put_error_count";
        pub PEARL_PUT_TIMER: Timer = "put_timer";

        pub PEARL_GET_COUNTER: Counter = "get_count";
        pub PEARL_GET_ERROR_COUNTER: Counter = "get_error_count";
        pub PEARL_GET_TIMER: Timer = "get_timer";
    }
}

pub fn init_pearl(prefix: &str, metrics: &Arc<dyn MetricsContainerBuilder + Send + Sync>) {
    let bucket = metrics.init_bucket(prefix);
    PEARL.target(bucket.clone());
}
