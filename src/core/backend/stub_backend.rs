use super::prelude::*;
use crate::core::backend::core::Exist;

#[derive(Clone, Debug)]
pub struct StubBackend {}

impl BackendStorage for StubBackend {
    fn run_backend(&self) -> Run {
        future::ok(()).boxed()
    }

    fn put(&self, _operation: BackendOperation, key: BobKey, data: BobData) -> Put {
        debug!(
            "PUT[{}]: hi from backend, timestamp: {:?}",
            key,
            data.meta()
        );
        future::ok(()).boxed()
    }

    fn put_alien(&self, _operation: BackendOperation, key: BobKey, data: BobData) -> Put {
        debug!(
            "PUT[{}]: hi from backend, timestamp: {:?}",
            key,
            data.meta()
        );
        future::ok(()).boxed()
    }

    fn get(&self, _operation: BackendOperation, key: BobKey) -> Get {
        debug!("GET[{}]: hi from backend", key);
        future::ok(BobData::new(vec![0], BobMeta::stub())).boxed()
    }

    fn get_alien(&self, _operation: BackendOperation, key: BobKey) -> Get {
        debug!("GET[{}]: hi from backend", key);
        future::ok(BobData::new(vec![0], BobMeta::stub())).boxed()
    }

    fn exist(&self, _operation: BackendOperation, _keys: &[BobKey]) -> Exist {
        debug!("EXIST: hi from backend");
        future::ok(vec![]).boxed()
    }

    fn exist_alien(&self, _operation: BackendOperation, _keys: &[BobKey]) -> Exist {
        debug!("EXIST: hi from backend");
        future::ok(vec![]).boxed()
    }
}