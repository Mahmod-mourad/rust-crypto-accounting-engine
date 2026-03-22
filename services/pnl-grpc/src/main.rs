use std::net::SocketAddr;

use anyhow::Result;
use sqlx::postgres::PgPoolOptions;
use tonic::transport::Server;
use tracing::info;

use pnl_grpc::pnl::pnl_service_server::PnlServiceServer;
use pnl_grpc::service::PnlServiceImpl;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let _telemetry = observability::init("pnl-grpc");

    // pnl-consumer owns the pnl_db; point DATABASE_URL at it.
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set (point at pnl_db)");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let addr: SocketAddr = std::env::var("GRPC_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50051".to_string())
        .parse()?;

    let svc = PnlServiceImpl::new(pool);

    info!(addr = %addr, "pnl-grpc server starting");

    Server::builder()
        .add_service(PnlServiceServer::new(svc))
        .serve(addr)
        .await?;

    Ok(())
}
