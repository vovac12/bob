use crate::core::backend::core::*;
use crate::core::data::{BobData, BobKey, BobMeta, VDiskId};
use futures03::{future::ok, FutureExt};

#[derive(Clone)]
pub struct StubBackend {}

impl BackendStorage for StubBackend {
    fn run_backend(&self) -> RunResult {
        async move {
            Ok(())
        }.boxed()
    }
    
    fn put(&self, _disk_name: String, _vdisk: VDiskId, key: BobKey, data: BobData) -> Put {
        debug!("PUT[{}]: hi from backend, timestamp: {}", key, data.meta);
        Put(ok(BackendPutResult {}).boxed())
    }

    fn put_alien(&self, _vdisk: VDiskId, key: BobKey, data: BobData) -> Put {
        debug!("PUT[{}]: hi from backend, timestamp: {}", key, data.meta);
        Put(ok(BackendPutResult {}).boxed())
    }

    fn get(&self, _disk_name: String, _vdisk: VDiskId, key: BobKey) -> Get {
        debug!("GET[{}]: hi from backend", key);
        Get(ok(BackendGetResult {
            data: BobData::new(vec![0], BobMeta::new_stub()),
        })
        .boxed())
    }

    fn get_alien(&self, _vdisk: VDiskId, key: BobKey) -> Get {
        debug!("GET[{}]: hi from backend", key);
        Get(ok(BackendGetResult {
            data: BobData::new(vec![0], BobMeta::new_stub()),
        })
        .boxed())
    }
}
