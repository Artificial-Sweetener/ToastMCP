mod assets;
mod mcp;
mod notify;

fn main() {
    if let Err(err) = mcp::run() {
        eprintln!("toastmcp error: {err:?}");
        std::process::exit(1);
    }
}
