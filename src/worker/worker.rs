use std::net::SocketAddr;

enum WorkerState {
    UP,
    DOWN,
}
struct Worker {
    socket_addr: SocketAddr,
    status: WorkerState,
}