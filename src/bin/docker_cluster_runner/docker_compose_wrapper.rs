use serde::{Serialize, Serializer};
use std::collections::HashMap;
use std::error::Error;
use std::net::Ipv4Addr;
use std::process::{Child, Command};

#[derive(Serialize, new)]
pub struct DockerCompose {
    version: String,
    services: HashMap<String, DockerService>,
    networks: HashMap<String, DockerComposeNetwork>,
}

impl DockerCompose {
    pub fn up(&self, base_dir: &str) -> Result<Child, Box<dyn Error>> {
        let filename = self.save_docker_compose(base_dir)?;
        let mut command = Command::new("docker-compose");
        command.arg("-f").arg(&filename).arg("up").arg("--build");
        command.spawn().map_err(|e| e.into())
    }

    pub fn down(&self, base_dir: &str) -> Result<Child, Box<dyn Error>> {
        let filename = self.save_docker_compose(base_dir)?;
        let mut command = Command::new("docker-compose");
        command.arg("-f").arg(&filename).arg("down");
        command.spawn().map_err(|e| e.into())
    }

    fn save_docker_compose(&self, base_dir: &str) -> Result<String, Box<dyn Error>> {
        let str_compose = serde_yaml::to_string(self)?;
        let filename = format!("{}/docker-compose.yml", base_dir);
        std::fs::write(&filename, str_compose)?;
        Ok(filename)
    }
}

#[derive(Serialize, new)]
pub struct DockerService {
    build: DockerBuild,
    volumes: Vec<VolumeMapping>,
    command: String,
    networks: HashMap<String, DockerNetwork>,
    ports: Vec<DockerPort>,
}

#[derive(Serialize, new, Clone)]
pub struct DockerBuild {
    context: String,
    dockerfile: String,
}

#[derive(new, Clone)]
pub struct VolumeMapping {
    host_dir: String,
    docker_dir: String,
}

impl Serialize for VolumeMapping {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}:{}", self.host_dir, self.docker_dir))
    }
}

#[derive(Serialize, Clone, Copy, new)]
pub struct DockerNetwork {
    ipv4_address: Ipv4Addr,
}

#[derive(Serialize, new)]
pub struct DockerComposeNetwork {
    driver: String,
    ipam: IPAMConfiguration,
}

#[derive(Serialize, new)]
pub struct IPAMConfiguration {
    config: Vec<SubnetConfiguration>,
}

#[derive(Serialize, new)]
pub struct SubnetConfiguration {
    subnet: String,
}

#[derive(new)]
pub struct DockerPort {
    host_port: u32,
    docker_port: u32,
}

impl Serialize for DockerPort {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}:{}", self.docker_port, self.host_port))
    }
}
