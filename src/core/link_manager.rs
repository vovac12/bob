use super::prelude::*;

#[derive(Debug)]
pub(crate) struct LinkManager {
    nodes: Arc<Vec<Node>>,
    check_interval: Duration,
}

pub(crate) type ClusterCallOutput<T> = Result<NodeOutput<T>, NodeOutput<BackendError>>;
pub(crate) type ClusterCallFuture<'a, T> =
    Pin<Box<dyn Future<Output = ClusterCallOutput<T>> + Send + 'a>>;

impl LinkManager {
    pub(crate) fn new(nodes: Vec<Node>, check_interval: Duration) -> LinkManager {
        LinkManager {
            nodes: Arc::new(nodes),
            check_interval,
        }
    }

    async fn checker_task(factory: Factory, nodes: Arc<Vec<Node>>, period: Duration) {
        let mut interval = interval(period);
        loop {
            interval.tick().await;
            for node in nodes.iter() {
                node.check(&factory).await.expect("check");
            }
        }
    }

    pub(crate) fn spawn_checker(&self, factory: Factory) {
        let nodes = self.nodes.clone();
        tokio::spawn(Self::checker_task(factory, nodes, self.check_interval));
    }

    pub(crate) async fn call_nodes<'a, F, T>(nodes: &[Node], f: F) -> Vec<ClusterCallOutput<T>>
    where
        F: FnMut(&'_ BobClient) -> ClusterCallFuture<'_, T> + Send + Clone,
        T: Send,
    {
        let futures: FuturesUnordered<_> = nodes
            .iter()
            .map(|node| Self::call_node(node, f.clone()))
            .collect();
        futures.collect().await
    }

    pub(crate) async fn call_node<'a, F, T>(node: &Node, mut f: F) -> ClusterCallOutput<T>
    where
        F: FnMut(&'_ BobClient) -> ClusterCallFuture<'_, T> + Send + Clone,
        T: Send,
    {
        match node.get_connection().await {
            Some(conn) => f(&conn).await,
            None => Err(NodeOutput::new(
                node.name().to_owned(),
                BackendError::Failed(format!("No active connection {:?}", node)),
            )),
        }
    }

    pub(crate) async fn exist_on_nodes(
        nodes: &[Node],
        keys: &[BobKey],
    ) -> Vec<Result<NodeOutput<Vec<bool>>, NodeOutput<BackendError>>> {
        Self::call_nodes(nodes, |client| {
            Box::pin(client.exist(keys.to_vec(), GetOptions::new_all()))
        })
        .await
    }
}