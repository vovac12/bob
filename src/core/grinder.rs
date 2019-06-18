use crate::core::backend::backend::{Backend, BackendGetResult, BackendPutResult, BackendError};
use crate::core::configs::node::NodeConfig;
use crate::core::data::VDiskMapper;
use crate::core::data::{BobData, BobKey, BobOptions};
use crate::core::link_manager::LinkManager;
use crate::core::cluster::{get_cluster, Cluster};
use futures03::task::Spawn;

use std::sync::Arc;

#[derive(Debug)]
pub enum BobError {
    Cluster(BackendError),
    Local(BackendError),

    NotFound,
    Other,
}

impl BobError {
    pub fn error(&self) -> BobError {
        match self {
            BobError::Cluster(err) => self.match_error(err, false),
            BobError::Local(err) => self.match_error(err, true),
            _ => panic!("dont use anything except Local and Cluster types"),
        }
    }

    fn match_error(&self, err: &BackendError, _is_local: bool) -> BobError {
        match err {
            BackendError::NotFound => BobError::NotFound,
            _ => BobError::Other,
        }
    }
    fn is_cluster(&self) -> bool {
        match *self {
            BobError::Cluster(_) => true,
            BobError::Local(_) => false,
            _ => panic!("dont use anything except Local and Cluster types"),
        }
    }
    fn is_local(&self) -> bool {
        !self.is_cluster()
    }
}

impl std::fmt::Display for BobError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let dest = if self.is_local() {"local"} else {"cluster"};
        write!(f, "dest: {}, error: {}", dest, self)
    }
}

pub struct Grinder {
    pub backend: Backend,
    mapper: VDiskMapper,

    link_manager: Arc<LinkManager>,
    cluster: Arc<dyn Cluster + Send + Sync>,

}

impl Grinder {
    pub fn new(mapper: VDiskMapper, config: &NodeConfig) -> Grinder {
        let link = Arc::new(LinkManager::new(
            mapper.nodes(),
            config.check_interval(),
            config.timeout(),
        ));

        Grinder {
            backend: Backend::new(&mapper, config),
            mapper: mapper.clone(),
            link_manager: link.clone(),
            cluster: get_cluster(link, &mapper, config),
        }
    }
    pub async fn put(
        &self,
        key: BobKey,
        data: BobData,
        opts: BobOptions,
    ) -> Result<BackendPutResult, BobError> 
    {
        if opts.contains(BobOptions::FORCE_NODE) {
            let op = self.mapper.get_operation(key);
            debug!(
                "PUT[{}] flag FORCE_NODE is on - will handle it by local node. Put params: {}",
                key, op
            );
            self.backend
                .put(&op, key, data)
                .0
                .await
                .map_err(|err| BobError::Local(err))
        } else {
            debug!("PUT[{}] will route to cluster", key);
            self.cluster
                .put_clustered(key, data)
                .0
                .await
                .map_err(|err| BobError::Cluster(err))
        }
    }

    pub async fn get(
        &self,
        key: BobKey,
        opts: BobOptions,
    ) -> Result<BackendGetResult, BobError> 
    {
        if opts.contains(BobOptions::FORCE_NODE) {
            let op = self.mapper.get_operation(key);
            debug!(
                "GET[{}] flag FORCE_NODE is on - will handle it by local node. Get params: {}",
                key, op
            );
            self.backend
                .get(&op, key)
                .0
                .await
                .map_err(|err| BobError::Local(err))
        } else {
            debug!("GET[{}] will route to cluster", key);
            self.cluster
                .get_clustered(key)
                .0
                .await
                .map_err(|err| BobError::Cluster(err))
        }
    }

    pub async fn get_periodic_tasks<S>(&self, ex: tokio::runtime::TaskExecutor, spawner: S) -> Result<(), ()> 
        where S: Spawn + Clone + Send + 'static + Unpin + Sync,
    {
        self.link_manager.get_checker_future(ex, spawner).await
    }
}
