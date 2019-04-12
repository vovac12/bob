use crate::core::bob_client::BobClient;
use crate::core::data::{
    print_vec, BobData, BobError, BobGetResult, BobKey, ClusterResult, Node, NodeDisk, VDisk,
};
use crate::core::link_manager::{LinkManager, NodeLink};

use std::sync::Arc;
use std::time::Duration;
use tokio::prelude::*;

use futures::future::*;
use futures::stream::*;

#[derive(Clone)]
pub struct Cluster {
    pub vdisks: Vec<VDisk>,
}

#[derive(Clone)]
pub struct Sprinkler {
    cluster: Cluster,
    quorum: u8,
    link_manager: Arc<LinkManager>,
}

pub struct SprinklerGetResult {
    pub data: Vec<u8>,
}
#[derive(Debug)]
pub struct SprinklerGetError {}

#[derive(Debug)]
pub struct SprinklerError {
    total_ops: u16,
    ok_ops: u16,
    quorum: u8,
}

impl std::fmt::Display for SprinklerError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "ok:{} total:{} q:{}",
            self.ok_ops, self.total_ops, self.quorum
        )
    }
}

#[derive(Debug)]
pub struct SprinklerResult {
    total_ops: u16,
    ok_ops: u16,
    quorum: u8,
}

impl std::fmt::Display for SprinklerResult {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "ok:{} total:{} q:{}",
            self.ok_ops, self.total_ops, self.quorum
        )
    }
}

impl Sprinkler {
    pub fn new() -> Sprinkler {
        let ex_cluster = Cluster {
            vdisks: vec![VDisk {
                id: 0,
                replicas: vec![
                    NodeDisk {
                        node: Node {
                            host: "127.0.0.1".to_string(),
                            port: 20000,
                        },
                        path: "/tmp/disk1".to_string(),
                    },
                    NodeDisk {
                        node: Node {
                            host: "127.0.0.1".to_string(),
                            port: 20002,
                        },
                        path: "/tmp/disk2".to_string(),
                    },
                ],
            }],
        };
        let nodes: Vec<_> = ex_cluster
            .vdisks
            .iter()
            .flat_map(|vdisk| vdisk.replicas.iter().map(|nd| nd.node.clone()))
            .collect();
        Sprinkler {
            quorum: 1,
            cluster: ex_cluster,
            link_manager: Arc::new(LinkManager::new(nodes, Duration::from_millis(3000))),
        }
    }

    pub fn get_periodic_tasks(
        &self,
        ex: tokio::runtime::TaskExecutor,
    ) -> Box<impl Future<Item = (), Error = ()>> {
        self.link_manager.get_checker_future(ex)
    }

    pub fn put_clustered(
        &self,
        key: BobKey,
        data: BobData,
    ) -> impl Future<Item = SprinklerResult, Error = SprinklerError> + 'static + Send {
        let target_nodes = self.calc_target_nodes(key);

        debug!(
            "PUT[{}]: Nodes for fan out: {:?}",
            key,
            print_vec(&target_nodes)
        );

        let reqs = Self::call_nodes(&mut self.get_connections(&target_nodes), |conn| {
            Box::new(conn.put(key, &data))
        });

        let l_quorum = self.quorum;
        Box::new(
            futures_unordered(reqs)
                .then(move |r| {
                    trace!("PUT[{}] Response from cluster {:?}", key, r);
                    ok::<_, ()>(r) // wrap all result kind to process it later
                })
                .fold(vec![], |mut acc, r| {
                    ok::<_, ()>({
                        acc.push(r);
                        acc
                    })
                })
                .then(move |acc| {
                    let res = acc.unwrap();
                    debug!("PUT[{}] cluster ans: {:?}", key, res);
                    let total_ops = res.iter().count();
                    let ok_count = res.iter().filter(|&r| r.is_ok()).count();
                    debug!(
                        "PUT[{}] total reqs: {} succ reqs: {} quorum: {}",
                        key, total_ops, ok_count, l_quorum
                    );
                    // TODO: send actuall list of vdisk it has been written on
                    if ok_count >= l_quorum as usize {
                        ok(SprinklerResult {
                            total_ops: total_ops as u16,
                            ok_ops: ok_count as u16,
                            quorum: l_quorum,
                        })
                    } else {
                        err(SprinklerError {
                            total_ops: total_ops as u16,
                            ok_ops: ok_count as u16,
                            quorum: l_quorum,
                        })
                    }
                }),
        )
    }

    pub fn get_clustered(
        &self,
        key: BobKey,
    ) -> impl Future<Item = ClusterResult<BobGetResult>, Error = BobError> + 'static + Send {
        let target_nodes = self.calc_target_nodes(key);

        debug!(
            "GET[{}]: Nodes for fan out: {:?}",
            key,
            print_vec(&target_nodes)
        );
        let reqs = Self::call_nodes(&mut self.get_connections(&target_nodes), |conn| {
            Box::new(conn.get(key))
        });

        Box::new(
            select_ok(reqs) // any result will enought
                .map(|(r, _)| r)
                .map_err(|_r| {
                    BobError::NotFound
                }),
        )
    }

    fn calc_target_nodes(&self, _key: BobKey) -> Vec<Node> {
        let target_vdisks: Vec<VDisk> = self
            .cluster
            .vdisks
            .iter()
            .filter(|disk| disk.id == 0)
            .cloned()
            .collect();

        let mut target_nodes: Vec<_> = target_vdisks
            .iter()
            .flat_map(|node_disk| node_disk.replicas.iter().map(|nd| nd.node.clone()))
            .collect();
        target_nodes.dedup();
        target_nodes
    }

    fn get_connections(&self, nodes: &[Node]) -> Vec<NodeLink> {
        nodes
            .iter()
            .map(|n| self.link_manager.clone().get_link(n))
            .collect()
    }

    fn call_nodes<F, T: 'static + Send>(
        links: &mut [NodeLink],
        mut f: F,
    ) -> Vec<Box<Future<Item = ClusterResult<T>, Error = ClusterResult<BobError>> + 'static + Send>>
    where
        F: FnMut(
            &mut BobClient,
        ) -> (Box<
            Future<Item = ClusterResult<T>, Error = ClusterResult<BobError>> + 'static + Send,
        >),
    {
        let t: Vec<_> = links
            .iter_mut()
            .map(move |nl| {
                let node = nl.node.clone();
                match &mut nl.conn {
                    Some(conn) => f(conn),
                    None => Box::new(err(ClusterResult {
                        result: BobError::Other(format!("No active connection {:?}", node)),
                        node,
                    })),
                }
            })
            .collect();
        t
    }
}
