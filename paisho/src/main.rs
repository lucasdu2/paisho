mod attacks;
mod environments;
mod server;
mod suites;

fn main() -> std::io::Result<()> {
    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| server::DEFAULT_SOCKET_NAME.to_string());
    server::run(&name)
}
