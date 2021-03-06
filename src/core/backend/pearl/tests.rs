use super::prelude::*;

use crate::core::backend::pearl::core::Pearl as PearlBackend;
use std::fs::remove_dir_all;

static DISK_NAME: &str = "disk1";
static PEARL_PATH: &str = "/tmp/d1/";
const KEY_ID: u64 = 1;
const TIMESTAMP: u64 = 1;

fn drop_pearl() {
    let path = PathBuf::from(PEARL_PATH);
    if path.exists() {
        remove_dir_all(path).unwrap();
    }
}

async fn create_backend(node_config: &str, cluster_config: &str) -> PearlBackend {
    let cluster = ClusterConfig::get_from_string(cluster_config).unwrap();
    debug!("cluster: {:?}", cluster);
    let node = NodeConfig::get_from_string(node_config, &cluster).unwrap();
    debug!("node: {:?}", node);

    let mapper = Arc::new(Virtual::new(&node, &cluster).await);
    debug!("mapper: {:?}", mapper);
    PearlBackend::new(mapper, &node)
}

async fn backend() -> PearlBackend {
    let node_config = "
log_config: logger.yaml
name: local_node
quorum: 1
operation_timeout: 3sec
check_interval: 5000ms
cluster_policy: quorum             # quorum
backend_type: pearl                # in_memory, stub, pearl
cleanup_interval: 1d
pearl:                             # used only for 'backend_type: pearl'
  max_blob_size: 10000000          # size in bytes. required for 'pearl'
  max_data_in_blob: 10000          # optional
  blob_file_name_prefix: bob       # optional
  fail_retry_timeout: 100ms
  alien_disk: disk1                # required for 'pearl'
  settings:                        # describes how create and manage bob directories. required for 'pearl'
    root_dir_name: bob             # root dir for bob storage. required for 'pearl'
    alien_root_dir_name: alien     # root dir for alien storage in 'alien_disk'. required for 'pearl'
    timestamp_period: 1d           # period when new pearl directory created. required for 'pearl'
    create_pearl_wait_delay: 100ms
";
    let cluster_config = "
nodes:
    - name: local_node
      address: 127.0.0.1:20000
      disks:
        - name: disk1
          path: /tmp/d1
vdisks:
    - id: 0
      replicas:
        - node: local_node
          disk: disk1
";
    debug!("node_config: {}", node_config);
    debug!("cluster_config: {}", cluster_config);
    create_backend(node_config, cluster_config).await
}

#[tokio::test]
async fn test_write_multiple_read() {
    test_utils::init_logger();
    drop_pearl();
    let vdisk_id = 0;
    let backend = backend().await;
    backend.run_backend().await.unwrap();
    let path = DiskPath::new(DISK_NAME.to_owned(), "".to_owned());
    let operation = Operation::new_local(vdisk_id, path);
    let data = BobData::new(vec![], BobMeta::new(TIMESTAMP));
    let write = backend.put(operation.clone(), KEY_ID, data).await;
    assert!(write.is_ok());

    let mut read = backend.get(operation.clone(), KEY_ID).await;
    assert_eq!(TIMESTAMP, read.unwrap().meta().timestamp());
    read = backend.get(operation.clone(), KEY_ID).await;
    assert_eq!(TIMESTAMP, read.unwrap().meta().timestamp());

    let res = backend.get(operation.clone(), KEY_ID).await;
    assert_eq!(TIMESTAMP, res.unwrap().meta().timestamp());
    let res = backend.get(operation, KEY_ID).await;
    assert_eq!(TIMESTAMP, res.unwrap().meta().timestamp());
    drop_pearl();
}
