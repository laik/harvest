use common::{Result, GLOBAL_BUFFER_SIZE};
use harvest::Harvest;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct ServerOptions {
    // short and long flags (-n, --namespace) will be deduced from the field's name
    #[structopt(short, env = "NAMESPACE", default_value = "", long)]
    namespace: String,

    // // short and long flags (-s, --api-server) will be deduced from the field's name
    #[structopt(short = "s", env = "API_SERVER", default_value = "", long)]
    api_server: String,

    // short and long flags (-d, --docker-dir) will be deduced from the field's name
    #[structopt(short = "d", long)]
    docker_dir: String,

    // short and long flags (-h, --node) will be deduced from the field's name
    #[structopt(short = "h", env = "HOSTNAME", default_value = "", long)]
    host: String,

    // short and long flags (-b, --buffer_size) will be deduced from the field's name
    #[structopt(short = "b", env = "BUFFER_SIZE", default_value = "100000", long)]
    buffer_size: usize,
}
// cargo run -- --namespace default --docker_dir /var/log/container --api-server http://localhost:9999/ --host node1

fn main() -> Result<()> {
    let opt = ServerOptions::from_args();
    println!("start args {:?}", opt);
    unsafe {
        GLOBAL_BUFFER_SIZE = opt.buffer_size;
    }
    Harvest::new(&opt.namespace, &opt.docker_dir, &opt.api_server, &opt.host).start()
}
