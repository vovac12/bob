extern crate itertools;

use itertools::Itertools;
use std::fs;


pub trait Validatable {
    fn validate(&self) -> bool;
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeDisk {
    pub path: String,
    pub name: String,
}
 
impl Validatable for NodeDisk {
     fn validate(&self) -> bool {
        let result = !self.path.is_empty() && !self.name.is_empty()
            && self.path != "~" && self.name != "~";
        if !result {
            error!("disk is invalid: {} {}", self.name, self.path);
        }
        result
    }
 }

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    pub address: String,
    pub disks: Vec<NodeDisk>,
}

impl Validatable for Node {
    fn validate(&self) -> bool {
        let result = !self.address.is_empty() && !self.name.is_empty()
            && self.address != "~" && self.name != "~"
            && self.disks.iter().all(|x| x.validate())
            && self.disks.iter()
            .group_by(|x| x.name.clone())
            .into_iter()
            .map(|(_, group)| group.count())
            .filter(|x| *x > 1)
            .count() == 0;
        if !result {
            error!("node is invalid: {} {}", self.name, self.address);
        }
        result
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Replica {
    pub node: String,
    pub disk: String,
}

impl PartialEq for Replica {
    fn eq(&self, other: &Replica) -> bool {
        self.node == other.node && self.disk == other.disk
    }
}

impl Validatable for Replica {
    fn validate(&self) -> bool {
        let result = !self.node.is_empty() && !self.disk.is_empty()
            && self.node != "~" && self.disk != "~";
        if !result {
            error!("replica is invalid: {} {}", self.node, self.disk);
        }
        result
    }
 }

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct VDisk {
    pub id: i32,
    pub replicas: Vec<Replica>,
}

impl Validatable for VDisk {
    fn validate(&self) -> bool {
        let result = self.replicas.iter().all(|x| x.validate())
            && self.replicas.iter()
            .group_by(|x| x.clone())
            .into_iter()
            .map(|(_, group)| group.count())
            .filter(|x| *x > 1)
            .count() == 0;
        if !result {
            error!("vdisk is invalid: {}", self.id);
        }
        result
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Cluster {
    pub nodes: Vec<Node>,
    pub vdisks: Vec<VDisk>
}

impl Validatable for Cluster {
    fn validate(&self) -> bool {
        if self.nodes.len() == 0
            || self.vdisks.len() == 0
            || self.nodes.iter().any(|x| !x.validate()) 
            || self.vdisks.iter().any(|x| !x.validate()) {
            return false;
        }

        if self.vdisks.iter()
                .group_by(|x| x.id)
                .into_iter()
                .map(|(_, group)| group.count())
                .filter(|x| *x > 1)
                .count() != 0 {
            error!("config contains duplicates vdisks ids");
            return false;
        }

        if self.nodes.iter()
                .group_by(|x| x.name.clone())
                .into_iter()
                .map(|(_, group)| group.count())
                .filter(|x| *x > 1)
                .count() != 0 {
            error!("config contains duplicates nodes names");
            return false;
        }

        for vdisk in self.vdisks.iter() {
            for replica in vdisk.replicas.iter() {
                match self.nodes.iter().find(|x|x.name==replica.node) {
                    Some(node) => {
                        if node.disks.iter().find(|x|x.name==replica.disk) == None {
                            error!("cannot find in node: {}, disk with name: {} for vdisk: {}", replica.node, replica.disk, vdisk.id);
                            return false;
                        }
                    },
                    None    => {
                        error!("cannot find node: {} for vdisk: {}", replica.node, vdisk.id);
                        return false;
                    },
                }
            }
        }

        true
    }
}

use crate::core::data::VDisk as DataVDisk;
use crate::core::data::NodeDisk as DataNodeDisk;
use crate::core::data::Node as DataNode;

use std::collections::HashMap;

pub trait BobConfig {
    fn get_cluster_config(&self, filename: &String) -> Option<Vec<DataVDisk>>;
    
    fn read_config(&self, filename: &String) ->Option<Cluster>;
    fn parse_config(&self, config: &String) -> Option<Cluster>;
    fn convert_to_data(&self, cluster: &Cluster) -> Vec<DataVDisk>;    
}

pub struct YamlConfig { }

impl BobConfig for YamlConfig {
    fn read_config(&self, filename: &String) -> Option<Cluster> {
        let result:Result<String,_> = fs::read_to_string(filename);
        match result {
            Ok(config) => return self.parse_config(&config),
            Err(e) => {
                error!("error on file opening: {}", e);
                return None;
            }
        }
    }
    fn parse_config(&self, config: &String) -> Option<Cluster> {
        let result:Result<Cluster, _> = serde_yaml::from_str(config);
        match result {
            Ok(cluster) => return Some(cluster),
            Err(e) => {
                error!("error on yaml parsing: {}", e);
                return None;
            }
        }
    }
    fn convert_to_data(&self, cluster: &Cluster) -> Vec<DataVDisk>  {
        let mut node_map = HashMap::new();
        for node in cluster.nodes.iter() {
            let mut disk_map = HashMap::new();
            for disk in node.disks.iter() {
                disk_map.insert(disk.name.clone(), disk.path.clone());
            }
            node_map.insert(node.name.clone(), (node.address.split(":").collect::<Vec<&str>>(), disk_map));
        }

        let mut result: Vec<DataVDisk> = Vec::with_capacity(cluster.vdisks.len());
        for vdisk in cluster.vdisks.iter() {
            let mut disk = DataVDisk{
                id: vdisk.id as u32,
                replicas: Vec::with_capacity(vdisk.replicas.len())
            };
            for replica in vdisk.replicas.iter() {
                let finded_node = node_map.get(&replica.node).unwrap();
                let node_disk = DataNodeDisk {
                    path: finded_node.1.get(&replica.disk).unwrap().to_string(),
                    node: DataNode {
                        host: finded_node.0[0].to_string(),
                        port: finded_node.0[1].parse().unwrap(),
                    },
                };
                disk.replicas.push(node_disk);
            }
            result.push(disk);
        }
        result
    }
    fn get_cluster_config(&self, filename: &String) -> Option<Vec<DataVDisk>> {
        let file: Option<Cluster> = self.read_config(filename);
        match file {
            Some(config) => {
                if !config.validate(){
                    error!("config is not valid");
                    return None;
                }
                return Some(self.convert_to_data(&config))
            },
            _ => return None,
        }
    }
}