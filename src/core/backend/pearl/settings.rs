use super::prelude::*;
use tokio::fs::DirEntry;

#[derive(Debug)]
pub(crate) struct Settings {
    bob_prefix_path: String,
    alien_folder: PathBuf,
    timestamp_period: Duration,
    config: PearlConfig,
    mapper: Arc<Virtual>,
}

impl Settings {
    pub(crate) fn new(config: &NodeConfig, mapper: Arc<Virtual>) -> Self {
        let config = config.pearl().clone();
        let disk_path = mapper
            .get_disk(&config.alien_disk())
            .expect("cannot find alien disk in config")
            .path();
        let alien_folder =
            format!("{}/{}/", disk_path, config.settings().alien_root_dir_name()).into();

        Self {
            bob_prefix_path: config.settings().root_dir_name().to_owned(),
            alien_folder,
            timestamp_period: config.settings().timestamp_period(),
            mapper,
            config,
        }
    }

    pub(crate) fn config(&self) -> &PearlConfig {
        &self.config
    }

    pub(crate) fn read_group_from_disk(self: Arc<Self>, config: &NodeConfig) -> Vec<Group> {
        let mut result = vec![];
        for disk in self.mapper.local_disks() {
            let vdisks = self.mapper.get_vdisks_by_disk(disk.name());
            let iter = vdisks.iter().map(|&vdisk_id| {
                let path = self.normal_path(disk.path(), vdisk_id);
                Group::new(
                    self.clone(),
                    vdisk_id,
                    config.name().to_owned(),
                    disk.name().to_owned(),
                    path,
                    config.name().to_owned(),
                )
            });
            result.extend(iter);
        }
        result
    }

    pub(crate) async fn read_alien_directory(
        self: Arc<Self>,
        config: &NodeConfig,
    ) -> Result<Vec<Group>> {
        let mut result = vec![];
        let node_names = Self::get_all_subdirectories(&self.alien_folder).await?;
        for node in node_names {
            if let Ok((node, node_name)) = self.try_parse_node_name(node) {
                let vdisks = Self::get_all_subdirectories(&node.path()).await?;

                for vdisk_id in vdisks {
                    if let Ok((entry, vdisk_id)) = self.try_parse_vdisk_id(vdisk_id) {
                        if self.mapper.is_vdisk_on_node(&node_name, vdisk_id) {
                            let disk_name = config.pearl().alien_disk().to_owned();
                            let group = Group::new(
                                self.clone(),
                                vdisk_id,
                                node_name.clone(),
                                disk_name,
                                entry.path(),
                                node_name.clone(),
                            );
                            result.push(group);
                        } else {
                            warn!(
                                "potentionally invalid state. Node: {} doesnt hold vdisk: {}",
                                node_name, vdisk_id
                            );
                        }
                    }
                }
            }
        }
        Ok(result)
    }

    pub(crate) async fn create_group(
        self: Arc<Self>,
        operation: &Operation,
        node_name: &str,
    ) -> Result<Group> {
        let remote_node_name = operation.remote_node_name().unwrap();
        let path = self.alien_path(operation.vdisk_id(), remote_node_name);

        Stuff::check_or_create_directory(&path)
            .await
            .with_context(|| "check or create directory failed")?;

        let group = Group::new(
            self.clone(),
            operation.vdisk_id(),
            remote_node_name.to_owned(),
            self.config.alien_disk().to_owned(),
            path,
            node_name.to_owned(),
        );
        Ok(group)
    }

    pub async fn get_all_subdirectories(path: &Path) -> Result<Vec<DirEntry>> {
        Stuff::check_or_create_directory(path)
            .await
            .with_context(|| "check or create directory failed".to_owned())?;

        let mut dir = tokio::fs::read_dir(path)
            .await
            .with_context(|| format!("couldn't process path: {:?} ", path))?;
        let mut directories = vec![];
        while let Some(entry) = dir.next().await {
            let (entry, metadata) = Self::try_read_path(entry)
                .await
                .with_context(|| "try read path failed")?;
            if metadata.is_dir() {
                directories.push(entry);
            }
        }
        Ok(directories)
    }

    fn try_parse_node_name(&self, entry: DirEntry) -> BackendResult<(DirEntry, String)> {
        let file_name = entry.file_name().into_string().map_err(|e| {
            error!("cannot parse file name: {:?}, {:?}", entry, e);
            Error::failed(format!("cannot parse file name: {:?}", entry))
        })?;
        if self
            .mapper
            .nodes()
            .values()
            .any(|node| node.name() == file_name)
        {
            Ok((entry, file_name))
        } else {
            let msg = format!("cannot find node with name: {:?}", file_name);
            error!("{}", msg);
            Err(Error::failed(msg))
        }
    }

    fn try_parse_vdisk_id(&self, entry: DirEntry) -> BackendResult<(DirEntry, VDiskID)> {
        let file_name = entry.file_name().into_string().map_err(|_| {
            let msg = format!("cannot parse file name: {:?}", entry);
            error!("{}", msg);
            Error::failed(msg)
        })?;
        let vdisk_id: VDiskID = file_name.parse().map_err(|_| {
            let msg = format!("cannot parse file name: {:?}", entry);
            error!("{}", msg);
            Error::failed(msg)
        })?;

        let vdisk = self
            .mapper
            .get_vdisks_ids()
            .into_iter()
            .find(|vdisk| *vdisk == vdisk_id);
        vdisk.map(|id| (entry, id)).ok_or({
            let msg = format!("cannot find vdisk with id: {:?}", vdisk_id);
            error!("{}", msg);
            Error::failed(msg)
        })
    }

    async fn try_read_path(entry: IOResult<DirEntry>) -> BackendResult<(DirEntry, Metadata)> {
        if let Ok(entry) = entry {
            if let Ok(metadata) = entry.metadata().await {
                Ok((entry, metadata))
            } else {
                let msg = format!("Couldn't get metadata for {:?}", entry.path());
                error!("{}", msg);
                Err(Error::failed(msg))
            }
        } else {
            let msg = format!("couldn't read entry: {:?} ", entry);
            error!("{}", msg);
            Err(Error::failed(msg))
        }
    }

    fn normal_path(&self, disk_path: &str, vdisk_id: VDiskID) -> PathBuf {
        let mut vdisk_path = PathBuf::from(format!("{}/{}/", disk_path, self.bob_prefix_path));
        vdisk_path.push(format!("{}/", vdisk_id));
        vdisk_path
    }

    fn alien_path(&self, vdisk_id: VDiskID, node_name: &str) -> PathBuf {
        let mut vdisk_path = self.alien_folder.clone();
        vdisk_path.push(format!("{}/{}/", node_name, vdisk_id));
        vdisk_path
    }

    #[inline]
    pub fn timestamp_period(&self) -> Duration {
        self.timestamp_period
    }

    #[inline]
    pub fn timestamp_period_as_secs(&self) -> u64 {
        self.timestamp_period.as_secs()
    }

    #[inline]
    pub fn get_actual_timestamp_start(&self) -> u64 {
        Stuff::get_start_timestamp_by_std_time(self.timestamp_period, SystemTime::now())
    }

    #[inline]
    pub(crate) fn choose_most_recent_data(records: Vec<BobData>) -> Option<BobData> {
        records
            .into_iter()
            .max_by(|x, y| x.meta().timestamp().cmp(&y.meta().timestamp()))
    }
}
