use std::collections::VecDeque;

use async_trait::async_trait;
use metrics::counter;
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::PgPool;
use tonic::{Request, Response, Status};
use tracing::instrument;

use crate::pnl::{
    pnl_service_server::PnlService, GetPnlSummaryRequest, GetPnlSummaryResponse, ListAssetsRequest,
    ListAssetsResponse, Lot as ProtoLot,
};

// ─── DB row ───────────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct PortfolioRow {
    asset:              String,
    lots:               serde_json::Value,
    total_quantity:     Decimal,
    total_realized_pnl: Decimal,
}

/// Shape of one entry in the `lots` JSONB array stored by pnl-consumer.
#[derive(Deserialize)]
struct LotJson {
    quantity:      Decimal,
    cost_per_unit: Decimal,
}

// ─── Service ──────────────────────────────────────────────────────────────────

pub struct PnlServiceImpl {
    pool: PgPool,
}

impl PnlServiceImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PnlService for PnlServiceImpl {
    /// Return realized PnL and open lots for the requested asset.
    #[instrument(skip(self), fields(asset = %request.get_ref().asset))]
    async fn get_pnl_summary(
        &self,
        request: Request<GetPnlSummaryRequest>,
    ) -> Result<Response<GetPnlSummaryResponse>, Status> {
        let asset = &request.into_inner().asset;

        if asset.is_empty() {
            return Err(Status::invalid_argument("asset must not be empty"));
        }

        let row = sqlx::query_as::<_, PortfolioRow>(
            "SELECT asset, lots, total_quantity, total_realized_pnl \
             FROM portfolio_state \
             WHERE asset = $1",
        )
        .bind(asset)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

        let row = row.ok_or_else(|| {
            Status::not_found(format!("no portfolio state found for asset '{asset}'"))
        })?;

        let lots: VecDeque<LotJson> = serde_json::from_value(row.lots)
            .map_err(|e| Status::internal(format!("deserialize lots: {e}")))?;

        let open_lots = lots
            .into_iter()
            .map(|l| ProtoLot {
                quantity:      l.quantity.to_string(),
                cost_per_unit: l.cost_per_unit.to_string(),
            })
            .collect();

        counter!("pnl_grpc_requests_total", "method" => "get_pnl_summary", "status" => "ok")
            .increment(1);
        Ok(Response::new(GetPnlSummaryResponse {
            asset:              row.asset,
            total_realized_pnl: row.total_realized_pnl.to_string(),
            total_quantity:     row.total_quantity.to_string(),
            open_lots,
        }))
    }

    /// Return all assets that have portfolio state (alphabetically sorted).
    #[instrument(skip(self))]
    async fn list_assets(
        &self,
        _request: Request<ListAssetsRequest>,
    ) -> Result<Response<ListAssetsResponse>, Status> {
        let assets = sqlx::query_scalar::<_, String>(
            "SELECT asset FROM portfolio_state ORDER BY asset",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            counter!("pnl_grpc_requests_total", "method" => "list_assets", "status" => "error")
                .increment(1);
            Status::internal(e.to_string())
        })?;

        counter!("pnl_grpc_requests_total", "method" => "list_assets", "status" => "ok")
            .increment(1);
        Ok(Response::new(ListAssetsResponse { assets }))
    }
}
