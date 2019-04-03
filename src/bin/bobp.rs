extern crate clap;

use clap::{App, Arg};
use futures::future::{loop_fn, Loop};

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::{thread, time};

use tokio::runtime::current_thread::Runtime;

use futures::{Future, Poll};
use std::net::SocketAddr;
use tokio::net::tcp::{ConnectFuture, TcpStream};
use tower::MakeService;
use tower_grpc::Request;
use tower_h2::client;
use tower_service::Service;

use bob::api::grpc::client::BobApi;
use bob::api::grpc::{Blob, BlobKey, PutRequest};
//use stopwatch::{Stopwatch};

#[derive(Debug, Clone)]
struct NetConfig {
    port: u16,
    target: String,
}

impl NetConfig {
    pub fn get_uri(&self) -> http::Uri {
        format!("http://{}:{}", self.target, self.port)
            .parse()
            .unwrap()
    }

    pub fn get_connector(&self) -> Dst {
        Dst::new(self)
    }
}

#[derive(Debug, Clone, Copy)]
struct TaskConfig {
    low_idx: u64,
    high_idx: u64,
    payload_size: u64,
}

struct Stat {
    put_total: AtomicU64,
}

struct Dst {
    addr: SocketAddr,
}
impl Dst {
    pub fn new(net_conf: &NetConfig) -> Dst {
        Dst {
            addr: SocketAddr::new(net_conf.target.parse().unwrap(), net_conf.port),
        }
    }
}
impl Service<()> for Dst {
    type Response = TcpStream;
    type Error = ::std::io::Error;
    type Future = ConnectFuture;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, _: ()) -> Self::Future {
        TcpStream::connect(&self.addr)
    }
}

fn stat_worker(stop_token: Arc<AtomicBool>, period_ms: u64, stat: Arc<Stat>) {
    let pause = time::Duration::from_millis(period_ms);
    let mut last_put_count = stat.put_total.load(Ordering::Relaxed);

    while !stop_token.load(Ordering::Relaxed) {
        thread::sleep(pause);
        let cur_put_count = stat.put_total.load(Ordering::Relaxed);
        let put_count_spd = (cur_put_count - last_put_count) / (period_ms / 1000);
        last_put_count = cur_put_count;
        println!("put: {:5} rps", put_count_spd);
    }
}

fn bench_worker(net_conf: NetConfig, task_conf: TaskConfig, stat: Arc<Stat>) {
    let uri = std::sync::Arc::new(net_conf.get_uri());
    let mut rt = Runtime::new().unwrap();

    let h2_settings = Default::default();
    let mut make_client = client::Connect::new(net_conf.get_connector(), h2_settings, rt.handle());
    let conn = rt.block_on(make_client.make_service(())).unwrap();
    let p_conn = tower_add_origin::Builder::new()
        .uri(uri.as_ref())
        .build(conn)
        .unwrap();
    let mut client = BobApi::new(p_conn.clone());
    rt.block_on(loop_fn((stat, task_conf.low_idx), |(lstat, i)| {
        client
            .put(Request::new(PutRequest {
                key: Some(BlobKey { key: i }),
                data: Some(Blob {
                    data: vec![0; task_conf.payload_size as usize],
                }),
                options: None,
            }))
            .and_then(move |_| {
                lstat.clone().put_total.fetch_add(1, Ordering::SeqCst);
                if i == task_conf.high_idx {
                    Ok(Loop::Break((lstat, i)))
                } else {
                    Ok(Loop::Continue((lstat, i + 1)))
                }
            })
    }))
    .unwrap();

    //     //let sw = Stopwatch::start_new();
    //     rt.block_on(pur_req).unwrap();
    //     stat.put_total.fetch_add(1, Ordering::SeqCst);
    //     //println!("Thing took {}ms", sw.elapsed_ms());
    // }
}

fn main() {
    let matches = App::new("Bob benchmark tool")
        .arg(
            Arg::with_name("host")
                .help("ip or hostname of bob")
                .takes_value(true)
                .short("h")
                .long("host")
                .default_value("127.0.0.1"),
        )
        .arg(
            Arg::with_name("port")
                .help("port of bob")
                .takes_value(true)
                .short("p")
                .long("port")
                .default_value("20000"),
        )
        .arg(
            Arg::with_name("payload")
                .help("payload size")
                .takes_value(true)
                .short("l")
                .long("payload")
                .default_value("100000"),
        )
        .arg(
            Arg::with_name("threads")
                .help("worker thread count")
                .takes_value(true)
                .short("t")
                .long("threads")
                .default_value("1"),
        )
        .arg(
            Arg::with_name("count")
                .help("count of records to proceed")
                .takes_value(true)
                .short("c")
                .long("count")
                .default_value("1000000"),
        )
        .get_matches();

    let net_conf = NetConfig {
        port: matches
            .value_of("port")
            .unwrap_or_default()
            .parse()
            .unwrap(),
        target: matches.value_of("host").unwrap_or_default().to_string(),
    };

    let task_conf = TaskConfig {
        low_idx: 1,
        high_idx: matches
            .value_of("count")
            .unwrap_or_default()
            .parse()
            .unwrap(),
        payload_size: matches
            .value_of("payload")
            .unwrap_or_default()
            .parse()
            .unwrap(),
    };

    let workers_count: u64 = matches
        .value_of("threads")
        .unwrap_or_default()
        .parse()
        .unwrap();

    println!("Bob will be benchmarked now");
    println!(
        "target: {}:{} workers_count: {} payload size: {} total records: {}",
        net_conf.target,
        net_conf.port,
        workers_count,
        task_conf.payload_size,
        task_conf.high_idx - task_conf.low_idx + 1
    );

    let stat = Arc::new(Stat {
        put_total: AtomicU64::new(0),
    });

    let stop_token: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    let stat_cln = stat.clone();

    let stat_thread = {
        let stop_token = stop_token.clone();
        thread::spawn(move || {
            stat_worker(stop_token, 1000, stat_cln);
        })
    };
    let mut workers = vec![];
    let task_size = (task_conf.high_idx - task_conf.low_idx) / workers_count;
    for i in 0..workers_count {
        let nc = net_conf.clone();
        let stat_inner = stat.clone();
        let tc = TaskConfig {
            low_idx: task_conf.low_idx + task_size * i,
            high_idx: task_conf.low_idx + task_size * (i + 1),
            payload_size: task_conf.payload_size,
        };
        workers.push(thread::spawn(move || {
            bench_worker(nc, tc, stat_inner);
        }));
    }

    workers.drain(..).for_each(|w| w.join().unwrap());

    stop_token.store(true, Ordering::Relaxed);

    stat_thread.join().unwrap();
}