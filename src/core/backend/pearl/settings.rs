// use super::core::BackendOperation;
use crate::core::{configs::node::NodeConfig, data::VDiskId, mapper::VDiskMapper};

use std::{path::PathBuf, sync::Arc};

pub(crate) struct Settings {
    bob_prefix_path: String,
    alien_folder: String,
}

impl Settings {
    pub(crate) fn new(config: &NodeConfig, mapper: Arc<VDiskMapper>) -> Self {
        let pearl_config = config.pearl.clone().unwrap();

        let alien_folder = format!(
            "{}/{}/",
            mapper
                .get_disk_by_name(&pearl_config.alien_disk())
                .expect("cannot find alien disk in config")
                .path,
            pearl_config.settings().alien_root_dir_name()
        );

        Settings {
            bob_prefix_path: pearl_config.settings().root_dir_name(),
            alien_folder: alien_folder,
        }
    }

    pub(crate) fn normal_directory(&self, disk_path: &str, vdisk_id: &VDiskId) -> PathBuf {
        let mut vdisk_path = PathBuf::from(format!("{}/{}/", disk_path, self.bob_prefix_path));
        vdisk_path.push(format!("{}/", vdisk_id));
        vdisk_path
    }

    pub(crate) fn alien_directory(&self) -> PathBuf {
        PathBuf::from(self.alien_folder.clone())
    }
}
