use crate::api::grpc::{BlobMeta, PutOptions};
use crate::core::{
    backend::Error,
    bob_client::{BobClient, BobClientFactory},
};
use std::sync::{Arc, Mutex};

impl PutOptions {
    pub(crate) fn new_client() -> Self {
        PutOptions {
            remote_nodes: vec![],
            force_node: true,
            overwrite: false,
        }
    }
}

#[derive(Debug)]
pub struct ClusterResult<T> {
    pub node: Node,
    pub result: T,
}

impl<T: std::fmt::Display> std::fmt::Display for ClusterResult<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "node: {}, result: {}", self.node, self.result)
    }
}

#[derive(Clone, Debug)]
pub struct BobData {
    pub data: Vec<u8>,
    pub meta: BobMeta,
}

impl BobData {
    pub fn new(data: Vec<u8>, meta: BobMeta) -> Self {
        BobData { data, meta }
    }
}

#[derive(Debug, Clone)]
pub struct BobMeta {
    pub timestamp: u32,
}
impl BobMeta {
    pub fn new(data: BlobMeta) -> Self {
        BobMeta {
            timestamp: data.timestamp,
        }
    }

    pub fn new_value(timestamp: u32) -> Self {
        BobMeta { timestamp }
    }

    pub fn new_stub() -> Self {
        BobMeta { timestamp: 1 }
    }
}

impl std::fmt::Display for BobMeta {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.timestamp)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BobKey {
    pub key: u64,
}

impl BobKey {
    pub fn new(key: u64) -> Self {
        BobKey { key }
    }
}

impl std::fmt::Display for BobKey {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.key)
    }
}

bitflags! {
    #[derive(Default)]
    pub struct BobOptions: u8 {
        const FORCE_NODE = 0x01;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VDiskId {
    id: u32,
}

impl VDiskId {
    pub fn new(id: u32) -> VDiskId {
        VDiskId { id }
    }
}

impl std::fmt::Display for VDiskId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

#[derive(Debug, Clone)]
pub struct VDisk {
    pub id: VDiskId,
    pub replicas: Vec<NodeDisk>,
}
impl std::fmt::Display for VDisk {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "#{}-{}",
            self.id,
            self.replicas
                .iter()
                .map(|nd| nd.to_string())
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct DiskPath {
    pub name: String,
    pub path: String,
}

impl DiskPath {
    pub fn new(name: &str, path: &str) -> DiskPath {
        DiskPath {
            name: name.to_string().clone(),
            path: path.to_string().clone(),
        }
    }
}

impl std::fmt::Display for DiskPath {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "#{}-{}", self.name, self.path)
    }
}

#[derive(Debug, Clone)]
pub struct NodeDisk {
    pub node: Node,
    pub path: String,
    pub name: String,
}

impl std::fmt::Display for NodeDisk {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}/{}-{}", self.node, self.name, self.path)
    }
}

impl PartialEq for NodeDisk {
    fn eq(&self, other: &NodeDisk) -> bool {
        self.node == other.node && self.path == other.path && self.name == other.name
    }
}

pub fn print_vec<T: std::fmt::Display>(coll: &[T]) -> String {
    coll.iter()
        .map(|vd| vd.to_string())
        .collect::<Vec<_>>()
        .join(",")
}


#[derive(Clone)]
pub struct Node {
    pub name: String,
    pub host: String,
    pub port: u16,

    conn: Arc<Mutex<Option<BobClient>>>,
}

impl Node {
    pub fn new(name: &str, host: &str, port: u16) -> Self {
        Node {
            name: name.to_string(),
            host: host.to_string(),
            port,
            conn: Arc::new(Mutex::new(None)),
        }
    }
    pub fn get_uri(&self) -> http::Uri {
        format!("http://{}:{}", self.host, self.port)
            .parse()
            .unwrap()
    }

    pub(crate) fn counter_display(&self) -> String {
        format!("{}:{}", self.host.replace(".", "_"), self.port)
    }

    pub(crate) fn set_connection(&self, client: BobClient) {
        *self.conn.lock().unwrap() = Some(client);
    }

    pub(crate) fn clear_connection(&self) {
        *self.conn.lock().unwrap() = None;
    }

    pub(crate) fn get_connection(&self) -> Option<BobClient> {
        self.conn.lock().unwrap().clone()
    }

    pub(crate) async fn check(self, client_fatory: BobClientFactory) -> Result<(), ()> {
        match self.get_connection() {
            Some(mut conn) => {
                conn.ping()
                    .await
                    .map(|_| debug!("All good with pinging node {:?}", self))
                    .map_err(|_| {
                        debug!("Got broken connection to node {:?}", self);
                        self.clear_connection();
                    })?;
                Ok(())
            }
            None => {
                debug!("will connect to {:?}", self);
                client_fatory
                    .produce(self.clone())
                    .await
                    .map(move |client| {
                        self.set_connection(client);
                    })
            }
        }
    }
}

impl std::fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}={}:{}", self.name, self.host, self.port)
    }
}

impl std::hash::Hash for Node {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.host.hash(state);
        self.port.hash(state);
    }
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}={}:{}", self.name, self.host, self.port)
    }
}

impl PartialEq for Node {
    fn eq(&self, other: &Node) -> bool {
        self.host == other.host && self.port == other.port
    }
}

impl Eq for Node {}