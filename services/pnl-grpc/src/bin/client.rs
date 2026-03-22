//! Example gRPC client — demonstrates service-to-service PnL queries.
//!
//! Usage:
//!   cargo run --bin pnl-client -- [server-addr] [asset]
//!
//! Defaults:
//!   server-addr : http://127.0.0.1:50051
//!   asset       : BTC

use pnl_grpc::pnl::{
    pnl_service_client::PnlServiceClient, GetPnlSummaryRequest, ListAssetsRequest,
};
use tonic::transport::Channel;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let addr  = args.next().unwrap_or_else(|| "http://127.0.0.1:50051".into());
    let asset = args.next().unwrap_or_else(|| "BTC".into());

    println!("Connecting to pnl-grpc server at {addr}");

    let channel = Channel::from_shared(addr)?.connect().await?;
    let mut client = PnlServiceClient::new(channel);

    // ── ListAssets ────────────────────────────────────────────────────────────
    let list_resp = client
        .list_assets(ListAssetsRequest {})
        .await?
        .into_inner();

    println!("\nTracked assets: {:?}", list_resp.assets);

    // ── GetPnlSummary ─────────────────────────────────────────────────────────
    println!("\nFetching PnL summary for '{asset}'…");

    let summary = client
        .get_pnl_summary(GetPnlSummaryRequest { asset: asset.clone() })
        .await?
        .into_inner();

    println!("Asset             : {}", summary.asset);
    println!("Total quantity    : {}", summary.total_quantity);
    println!("Total realized PnL: {}", summary.total_realized_pnl);
    println!("Open lots ({}):", summary.open_lots.len());
    for (i, lot) in summary.open_lots.iter().enumerate() {
        println!("  [{i}] qty={} cost_per_unit={}", lot.quantity, lot.cost_per_unit);
    }

    Ok(())
}
