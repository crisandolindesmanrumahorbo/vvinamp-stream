use spotify_streaming::db::Database;
use spotify_streaming::server::Server;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::oneshot;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    let db_pool = Database::new_pool("postgres://rust_101_user:password@localhost/vvinamp").await;

    let server = Server::new(db_pool);
    // Start server
    let server_handle = tokio::spawn(async move { server.start(shutdown_rx).await });

    // Shutdown
    gracefully_shutdown(shutdown_tx, server_handle).await;

    Ok(())
}

async fn gracefully_shutdown(
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
    server_handle: tokio::task::JoinHandle<Result<(), anyhow::Error>>,
) {
    // Wait for shutdown signal
    let mut signal_terminate = signal(SignalKind::terminate()).unwrap();
    let mut signal_interrupt = signal(SignalKind::interrupt()).unwrap();
    tokio::select! {
        _ = signal_terminate.recv() => {
            println!("Shutdown signal received");
        },
        _ = signal_interrupt.recv() => {
            println!("SIGINT received");
        }
    }

    // Trigger graceful shutdown
    let _ = shutdown_tx.send(());
    let _ = server_handle.await;

    println!("Shutdown completed");
}
