use crate::core::backend;
use crate::core::backend::core::*;
use crate::core::backend::pearl::{
    data::*,
    metrics::*,
    stuff::{LockGuard, Stuff},
};
use crate::core::backend::policy::BackendPolicy;
use crate::core::configs::node::{NodeConfig, PearlConfig};
use crate::core::data::{BobData, BobKey, VDiskId};
use crate::core::mapper::VDiskMapper;
use pearl::{Builder, ErrorKind, Storage};

use futures03::{
    compat::Future01CompatExt,
    future::err as err03,
    task::{Spawn, SpawnExt},
    FutureExt,
};

use std::{path::PathBuf, sync::Arc};
use tokio_timer::sleep;

pub struct PearlBackend<TSpawner> {
    vdisks: Arc<Vec<PearlVDisk<TSpawner>>>,
    alien_dir: PearlVDisk<TSpawner>,
}

impl<TSpawner: Spawn + Clone + Send + 'static + Unpin + Sync> PearlBackend<TSpawner> {
    pub fn new(mapper: Arc<VDiskMapper>, config: &NodeConfig, spawner: TSpawner) -> Self {
        debug!("initializing pearl backend");
        let pearl_config = config.pearl.clone().unwrap();

        let mut result = Vec::new();

        let policy = BackendPolicy::new(config, mapper.clone());
        //init pearl storages for each vdisk
        for disk in mapper.local_disks().iter() {
            let mut vdisks: Vec<PearlVDisk<TSpawner>> = mapper
                .get_vdisks_by_disk(&disk.name)
                .iter()
                .map(|vdisk_id| {
                    PearlVDisk::new(
                        &disk.name,
                        vdisk_id.clone(),
                        policy.normal_directory(&disk.path, vdisk_id),
                        pearl_config.clone(),
                        spawner.clone(),
                    )
                })
                .collect();
            result.append(&mut vdisks);
        }

        //init alien storage
        let alien_dir = PearlVDisk::new_alien(
            &pearl_config.alien_disk(),
            policy.alien_directory(),
            pearl_config.clone(),
            spawner.clone(),
        );

        PearlBackend {
            vdisks: Arc::new(result),
            alien_dir,
        }
    }

    #[allow(dead_code)]
    pub(crate) async fn test<TRet, F>(
        &self,
        disk_name: String,
        vdisk_id: VDiskId,
        f: F,
    ) -> BackendResult<TRet>
    where
        F: Fn(&mut PearlSync) -> TRet + Send + Sync,
    {
        let vdisks = self.vdisks.clone();
        let vdisk = vdisks.iter().find(|vd| vd.equal(&disk_name, &vdisk_id));
        if let Some(disk) = vdisk {
            let d_clone = disk.clone(); // TODO remove copy of disk. add Box?
            let q = async move { d_clone.test(f).await };
            q.await
        } else {
            Err(backend::Error::StorageError(format!(
                "vdisk not found: {}",
                vdisk_id
            )))
        }
    }

    #[allow(dead_code)]
    pub(crate) async fn test_vdisk<TRet, F>(
        &self,
        disk_name: String,
        vdisk_id: VDiskId,
        f: F,
    ) -> BackendResult<TRet>
    where
        F: Fn(PearlVDisk<TSpawner>) -> Future03Result<TRet> + Send + Sync,
    {
        let vdisks = self.vdisks.clone();
        let vdisk = vdisks.iter().find(|vd| vd.equal(&disk_name, &vdisk_id));
        if let Some(disk) = vdisk {
            let d_clone = disk.clone(); // TODO remove copy of disk. add Box?
            f(d_clone).await
        } else {
            async move {
                Err(backend::Error::StorageError(format!(
                    "vdisk not found: {}",
                    vdisk_id
                )))
            }
                .await
        }
    }

    fn is_write_error(err: Option<&backend::Error>) -> bool {
        match err {
            Some(backend::Error::DuplicateKey) | Some(backend::Error::VDiskIsNotReady) => false,
            Some(_) => true,
            _ => false,
        }
    }

    fn is_read_error(err: Option<&backend::Error>) -> bool {
        match err {
            Some(backend::Error::KeyNotFound) | Some(backend::Error::VDiskIsNotReady) => false,
            Some(_) => true,
            _ => false,
        }
    }

    async fn put_common(pearl: PearlVDisk<TSpawner>, key: BobKey, data: BobData) -> PutResult {
        let result = pearl
            .write(key, Box::new(data))
            .map(|r| r.map(|_ok| BackendPutResult {}))
            .await;
        if Self::is_write_error(result.as_ref().err()) && pearl.try_reinit().await.unwrap() {
            let _ = pearl.reinit_storage().await;
        }
        result
    }

    async fn get_common(pearl: PearlVDisk<TSpawner>, key: BobKey) -> GetResult {
        let result = pearl
            .read(key)
            .map(|r| r.map(|data| BackendGetResult { data }))
            .await;
        if Self::is_read_error(result.as_ref().err()) && pearl.try_reinit().await.unwrap() {
            let _ = pearl.reinit_storage().await;
        }
        result
    }
}

impl<TSpawner: Spawn + Clone + Send + 'static + Unpin + Sync> BackendStorage
    for PearlBackend<TSpawner>
{
    fn run_backend(&self) -> RunResult {
        debug!("run pearl backend");

        let vdisks = self.vdisks.clone();
        let alien_dir = self.alien_dir.clone();
        let q = async move {
            for i in 0..vdisks.len() {
                let _ = vdisks[i]
                    .clone()
                    .prepare_storage() //TODO add Box?
                    .await;
            }

            let _ = alien_dir.prepare_storage().await;
            Ok(())
        };

        q.boxed()
    }

    fn put(&self, operation: BackendOperation, key: BobKey, data: BobData) -> Put {
        debug!("PUT[{}] to pearl backend. opeartion: {}", key, operation);

        let vdisks = self.vdisks.clone();
        Put({
            let vdisk = vdisks
                .iter()
                .find(|vd| vd.equal(&operation.disk_name_local(), &operation.vdisk_id));
            if let Some(disk) = vdisk {
                let d_clone = disk.clone();
                async move {
                    Self::put_common(d_clone, key, data) // TODO remove copy of disk. add Box?
                        .await
                        .map_err(|e| {
                            debug!("PUT[{}], error: {:?}", key, e);
                            e
                        })
                }
                    .boxed()
            } else {
                debug!(
                    "PUT[{}] to pearl backend. Cannot find storage, operation: {}",
                    key, operation
                );
                err03(backend::Error::VDiskNoFound(operation.vdisk_id)).boxed()
            }
        })
    }

    fn put_alien(&self, _operation: BackendOperation, key: BobKey, data: BobData) -> Put {
        debug!("PUT[alien][{}] to pearl backend", key);

        let alien_dir = self.alien_dir.clone();
        Put({
            async move {
                Self::put_common(alien_dir.clone(), key, data) // TODO remove copy of disk. add Box?
                    .await
                    .map_err(|e| {
                        debug!("PUT[alien][{}], error: {:?}", key, e);
                        e
                    })
            }
                .boxed()
        })
    }

    fn get(&self, operation: BackendOperation, key: BobKey) -> Get {
        debug!("Get[{}] from pearl backend. operation: {}", key, operation);

        let vdisks = self.vdisks.clone();
        Get({
            let vdisk = vdisks
                .iter()
                .find(|vd| vd.equal(&operation.disk_name_local(), &operation.vdisk_id));
            if let Some(disk) = vdisk {
                let d_clone = disk.clone();
                async move {
                    Self::get_common(d_clone, key) // TODO remove copy of disk. add Box?
                        .await
                        .map_err(|e| {
                            debug!("GET[{}], error: {:?}", key, e);
                            e
                        })
                }
                    .boxed()
            } else {
                debug!(
                    "GET[{}] to pearl backend. Cannot find storage, operation: {}",
                    key, operation
                );
                err03(backend::Error::VDiskNoFound(operation.vdisk_id)).boxed()
            }
        })
    }

    fn get_alien(&self, _operation: BackendOperation, key: BobKey) -> Get {
        debug!("Get[alien][{}] from pearl backend", key);

        let alien_dir = self.alien_dir.clone();
        Get({
            async move {
                Self::get_common(alien_dir.clone(), key) // TODO remove copy of disk. add Box?
                    .await
                    .map_err(|e| {
                        debug!("PUT[alien][{}], error: {:?}", key, e);
                        e
                    })
            }
                .boxed()
        })
    }
}

#[derive(Clone)]
pub(crate) struct PearlVDisk<TSpawner> {
    name: String,
    vdisk: Option<VDiskId>,
    disk_path: PathBuf,

    config: PearlConfig,
    spawner: TSpawner,

    pub(crate) storage: Arc<LockGuard<PearlSync>>,
}

impl<TSpawner: Spawn + Clone + Send + 'static + Unpin + Sync> PearlVDisk<TSpawner> {
    pub fn new(
        name: &str,
        vdisk: VDiskId,
        disk_path: PathBuf,
        config: PearlConfig,
        spawner: TSpawner,
    ) -> Self {
        PearlVDisk {
            name: name.to_string(),
            disk_path,
            vdisk: Some(vdisk),
            config,
            spawner,
            storage: Arc::new(LockGuard::new(PearlSync::new())),
        }
    }
    pub fn new_alien(
        name: &str,
        disk_path: PathBuf,
        config: PearlConfig,
        spawner: TSpawner,
    ) -> Self {
        PearlVDisk {
            name: name.to_string(),
            vdisk: None,
            disk_path,
            config,
            spawner,
            storage: Arc::new(LockGuard::new(PearlSync::new())),
        }
    }

    fn vdisk_print(&self) -> String {
        match &self.vdisk {
            Some(vdisk) => format!("{}", vdisk),
            None => "alien".to_string(),
        }
    }
    pub async fn update(&self, storage: Storage<PearlKey>) -> BackendResult<()> {
        trace!("try update Pearl id: {}", self.vdisk_print());

        self.storage
            .write_sync_mut(|st| {
                st.set(storage.clone());
                st.ready(); // current pearl disk is ready

                debug!(
                    "update Pearl id: {}, mark as ready, state: {}",
                    self.vdisk_print(),
                    st
                );
            })
            .await
    }

    pub fn equal(&self, name: &str, vdisk: &VDiskId) -> bool {
        self.name == name && self.vdisk.as_ref().unwrap() == vdisk
    }

    pub async fn write(&self, key: BobKey, data: Box<BobData>) -> BackendResult<()> {
        self.storage
            .read(|st| {
                if !st.is_ready() {
                    trace!(
                        "Vdisk: {} is not ready for writing, state: {}",
                        self.vdisk_print(),
                        st
                    );
                    return err03(backend::Error::VDiskIsNotReady).boxed();
                }
                let storage = st.get();
                trace!("Vdisk: {}, write key: {}", self.vdisk_print(), key);
                Self::write_disk(storage, PearlKey::new(key), data.clone()).boxed()
            })
            .await
    }

    async fn write_disk(
        storage: PearlStorage,
        key: PearlKey,
        data: Box<BobData>,
    ) -> BackendResult<()> {
        PEARL_PUT_COUNTER.count(1);
        let timer = PEARL_PUT_TIMER.start();
        storage
            .write(key, PearlData::new(data).bytes())
            .await
            .map(|r| {
                PEARL_PUT_TIMER.stop(timer);
                r
            })
            .map_err(|e| {
                PEARL_PUT_ERROR_COUNTER.count(1);
                trace!("error on write: {:?}", e);
                //TODO check duplicate
                backend::Error::StorageError(format!("{:?}", e))
            })
    }

    pub async fn read(&self, key: BobKey) -> Result<BobData, backend::Error> {
        self.storage
            .read(|st| {
                if !st.is_ready() {
                    trace!(
                        "Vdisk: {} is not ready for reading, state: {}",
                        self.vdisk_print(),
                        st
                    );
                    return err03(backend::Error::VDiskIsNotReady).boxed();
                }
                let storage = st.get();
                trace!("Vdisk: {}, read key: {}", self.vdisk_print(), key);

                let q = async move {
                    PEARL_GET_COUNTER.count(1);
                    let timer = PEARL_GET_TIMER.start();
                    storage
                        .read(PearlKey::new(key))
                        .await
                        .map(|r| {
                            PEARL_GET_TIMER.stop(timer);
                            PearlData::parse(r)
                        })
                        .map_err(|e| {
                            PEARL_GET_ERROR_COUNTER.count(1);
                            trace!("error on read: {:?}", e);
                            match e.kind() {
                                ErrorKind::RecordNotFound => backend::Error::KeyNotFound,
                                _ => backend::Error::StorageError(format!("{:?}", e)),
                            }
                        })?
                };
                q.boxed()
            })
            .await
    }

    #[allow(dead_code)]
    async fn read_disk(storage: &PearlStorage, key: PearlKey) -> BackendResult<BobData> {
        PEARL_GET_COUNTER.count(1);
        let timer = PEARL_GET_TIMER.start();
        storage
            .read(key)
            .await
            .map(|r| {
                PEARL_GET_TIMER.stop(timer);
                PearlData::parse(r)
            })
            .map_err(|e| {
                PEARL_GET_ERROR_COUNTER.count(1);
                trace!("error on read: {:?}", e);
                match e.kind() {
                    ErrorKind::RecordNotFound => backend::Error::KeyNotFound,
                    _ => backend::Error::StorageError(format!("{:?}", e)),
                }
            })?
    }

    pub async fn try_reinit(&self) -> BackendResult<bool> {
        self.storage
            .write_mut(|st| {
                if st.is_reinit() {
                    trace!(
                        "Vdisk: {} is currently reinitializing, state: {}",
                        self.vdisk_print(),
                        st
                    );
                    return err03(backend::Error::VDiskIsNotReady).boxed();
                }
                st.init();
                trace!("Vdisk: {} set as reinit, state: {}", self.vdisk_print(), st);
                let storage = st.get();
                trace!("Vdisk: {} close old Pearl", self.vdisk_print());
                let q = async move {
                    let result = storage.close().await;
                    if let Err(e) = result {
                        error!("can't close pearl storage: {:?}", e);
                        return Ok(true); // we can't do anything
                    }
                    Ok(true)
                };

                q.boxed()
            })
            .await
    }

    pub async fn reinit_storage(self) -> BackendResult<()> {
        debug!("Vdisk: {} try reinit Pearl", self.vdisk_print());
        let mut spawner = self.spawner.clone();
        async move {
            debug!("Vdisk: {} start reinit Pearl", self.vdisk_print());
            let _ = spawner
                .spawn(self.prepare_storage().map(|_r| ()))
                .map_err(|e| {
                    error!("can't start reinit thread: {:?}", e);
                    panic!("can't start reinit thread: {:?}", e);
                });
        }
            .await;

        Ok(())
    }

    pub async fn prepare_storage(self) -> BackendResult<()> {
        let repeat = true;
        let path = &self.disk_path;
        let config = self.config.clone();
        // let spawner = self.spawner.clone();

        let delay = config.fail_retry_timeout();

        let mut need_delay = false;
        while repeat {
            if need_delay {
                let _ = sleep(delay).compat().boxed().await;
            }
            need_delay = true;

            if let Err(e) = Stuff::check_or_create_directory(path) {
                error!("cannot check path: {:?}, error: {}", path, e);
                continue;
            }

            if let Err(e) = Stuff::drop_pearl_lock_file(path) {
                error!("cannot delete lock file: {:?}, error: {}", path, e);
                continue;
            }

            let storage = Self::init_pearl_by_path(path, &config);
            if let Err(e) = storage {
                error!("cannot build pearl by path: {:?}, error: {:?}", path, e);
                continue;
            }
            let mut st = storage.unwrap();
            if let Err(e) = st.init().await {
                error!("cannot init pearl by path: {:?}, error: {:?}", path, e);
                continue;
            }
            if let Err(e) = self.update(st).await {
                error!("cannot update storage by path: {:?}, error: {:?}", path, e);
                //TODO drop storage  .Part 2: i think we should panic here
                continue;
            }
            debug!("Vdisk: {} Pearl is ready for work", self.vdisk_print());
            return Ok(());
        }
        Err(backend::Error::StorageError("stub".to_string()))
    }

    fn init_pearl_by_path(path: &PathBuf, config: &PearlConfig) -> BackendResult<PearlStorage> {
        let mut builder = Builder::new().work_dir(path.clone());

        builder = match &config.blob_file_name_prefix {
            Some(blob_file_name_prefix) => builder.blob_file_name_prefix(blob_file_name_prefix),
            _ => builder.blob_file_name_prefix("bob"),
        };
        builder = match config.max_data_in_blob {
            Some(max_data_in_blob) => builder.max_data_in_blob(max_data_in_blob),
            _ => builder,
        };
        builder = match config.max_blob_size {
            Some(max_blob_size) => builder.max_blob_size(max_blob_size),
            _ => panic!("'max_blob_size' is not set in pearl config"),
        };

        let storage = builder
            .build()
            .map_err(|e| backend::Error::StorageError(format!("{:?}", e)));
        if let Err(e) = storage {
            error!("cannot build pearl by path: {:?}, error: {}", path, e);
            return Err(backend::Error::StorageError(format!(
                "cannot build pearl by path: {:?}, error: {}",
                path, e
            )));
        }
        trace!("Pearl is created by path: {:?}", path);
        storage
    }

    #[allow(dead_code)]
    pub(crate) async fn test<TRet, F>(&self, f: F) -> BackendResult<TRet>
    where
        F: Fn(&mut PearlSync) -> TRet + Send + Sync,
    {
        self.storage.write_sync_mut(|st| f(st)).await
    }
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) enum PearlState {
    Normal,       // pearl is started and working
    Initializing, // pearl restarting
}

#[derive(Clone)]
pub(crate) struct PearlSync {
    pub(crate) storage: Option<PearlStorage>,
    state: PearlState,

    pub(crate) start_time_test: u8,
}
impl PearlSync {
    pub(crate) fn new() -> Self {
        PearlSync {
            storage: None,
            state: PearlState::Initializing,
            start_time_test: 0,
        }
    }
    pub(crate) fn ready(&mut self) {
        self.set_state(PearlState::Normal);
    }
    pub(crate) fn init(&mut self) {
        self.set_state(PearlState::Initializing);
    }
    pub(crate) fn is_ready(&self) -> bool {
        self.get_state() == PearlState::Normal
    }
    pub(crate) fn is_reinit(&self) -> bool {
        self.get_state() == PearlState::Initializing
    }

    pub(crate) fn set_state(&mut self, state: PearlState) {
        self.state = state;
    }

    pub(crate) fn get_state(&self) -> PearlState {
        self.state.clone()
    }

    pub(crate) fn set(&mut self, storage: PearlStorage) {
        self.storage = Some(storage);
        self.start_time_test += 1;
    }
    pub(crate) fn get(&self) -> PearlStorage {
        self.storage.clone().unwrap()
    }
}

impl std::fmt::Display for PearlSync {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "#{:?}", self.state)
    }
}
