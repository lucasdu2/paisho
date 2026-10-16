mod attacks;
mod environments;
mod results;
mod server;
mod suites;

/// Usage: `paisho [socket-name] [logdir]`, defaulting to `paisho.sock` and `runs`.
fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let name = args
        .next()
        .unwrap_or_else(|| server::DEFAULT_SOCKET_NAME.to_string());
    let logdir = args
        .next()
        .unwrap_or_else(|| server::DEFAULT_LOGDIR.to_string());
    server::run(&name, logdir.into())
}
