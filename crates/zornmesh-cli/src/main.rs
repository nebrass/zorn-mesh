fn main() {
    std::process::exit(zornmesh_cli::run(std::env::args().skip(1)));
}
