use crate::*;
use log::*;
use serde::de::DeserializeOwned;
use std::time::SystemTime;
use time_series::trunc;

#[derive(Clone)]
pub struct Account {
    pub client: Client,
    pub recv_window: u64,
    pub base_asset: String,
    pub quote_asset: String,
    pub ticker: String,
}

impl Account {
    #[allow(dead_code)]
    pub fn new(
        client: Client,
        recv_window: u64,
        base_asset: String,
        quote_asset: String,
        ticker: String,
    ) -> Self {
        Self {
            client,
            recv_window,
            base_asset,
            quote_asset,
            ticker,
        }
    }

    #[allow(dead_code)]
    pub async fn exchange_info(&self, symbol: String) -> DreamrunnerResult<ExchangeInformation> {
        let req = ExchangeInfo::request(symbol);
        self.client
            .get::<ExchangeInformation>(API::Spot(Spot::ExchangeInfo), Some(req)).await
    }

    /// Get account info which includes token balances
    pub async fn account_info(&self) -> DreamrunnerResult<AccountInfoResponse> {
        let builder = AccountInfo::request(None);
        let req = builder.request;
        let pre = SystemTime::now();
        let res = self
            .client
            .get_signed::<AccountInfoResponse>(API::Spot(Spot::Account), Some(req)).await;
        let dur = SystemTime::now().duration_since(pre).unwrap().as_millis();
        info!("Request time: {:?}ms", dur);
        if let Err(e) = res {
            let now = AccountInfo::get_timestamp()?;
            let req_time = builder
                .btree
                .get("timestamp")
                .ok_or(DreamrunnerError::Custom(
                    "Timestamp not found in request".to_string(),
                ))?
                .parse::<u64>()?;
            // difference between now and req_time
            let diff = now - req_time;
            error!("🛑 Failed to get account info in {}ms: {:?}", diff, e);
            return Err(e);
        }
        res
    }

    /// Get all assets
    /// Not available on testnet
    pub async fn all_assets(&self) -> DreamrunnerResult<Vec<CoinInfo>> {
        let req = AllAssets::request(Some(5000));
        self.client
            .get_signed::<Vec<CoinInfo>>(API::Savings(Sapi::AllCoins), Some(req)).await
    }

    /// Get price of a single symbol
    pub async fn price(&self) -> DreamrunnerResult<f64> {
        let req = Price::request(self.ticker.to_string());
        let res = self
            .client
            .get::<PriceResponse>(API::Spot(Spot::Price), Some(req)).await?;
        Ok(res.price.parse::<f64>()?)
    }

    /// Get historical orders for a single symbol
    pub async fn all_orders(&self, symbol: String) -> DreamrunnerResult<Vec<HistoricalOrder>> {
        let req = AllOrders::request(symbol, Some(5000));
        let mut orders = self
            .client
            .get_signed::<Vec<HistoricalOrder>>(API::Spot(Spot::AllOrders), Some(req)).await?;
        // order by time
        orders.sort_by(|a, b| a.update_time.partial_cmp(&b.update_time).unwrap());
        Ok(orders)
    }

    /// Get last open trade for a single symbol
    /// Returns Some if there is an open trade, None otherwise
    pub async fn open_orders(&self, symbol: String) -> DreamrunnerResult<Vec<HistoricalOrder>> {
        let req = AllOrders::request(symbol, Some(5000));
        let orders = self
            .client
            .get_signed::<Vec<HistoricalOrder>>(API::Spot(Spot::AllOrders), Some(req)).await?;
        // filter out orders that are not filled or canceled
        let open_orders = orders
            .into_iter()
            .filter(|order| order.status == "NEW")
            .collect::<Vec<HistoricalOrder>>();
        Ok(open_orders)
    }

    /// Cancel all open orders for a single symbol
    pub async fn cancel_all_open_orders(&self) -> DreamrunnerResult<Vec<OrderCanceled>> {
        info!("Canceling all active orders");
        let req = CancelOrders::request(self.ticker.clone(), Some(10000));
        let res = self
            .client
            .delete_signed::<Vec<OrderCanceled>>(API::Spot(Spot::OpenOrders), Some(req)).await;
        if let Err(e) = &res {
            if let DreamrunnerError::Binance(err) = &e {
                return if err.code != -2011 {
                    error!("🛑 Failed to cancel all active orders: {:?}", e);
                    Err(DreamrunnerError::Binance(err.clone()))
                } else {
                    debug!("No open orders to cancel");
                    Ok(vec![])
                };
            }
        }
        res
    }

    pub async fn cancel_order(&self, order_id: u64) -> DreamrunnerResult<OrderCanceled> {
        debug!("Canceling order {}", order_id);
        let req = CancelOrder::request(order_id, self.ticker.to_string(), Some(10000));
        let res = self
            .client
            .delete_signed::<OrderCanceled>(API::Spot(Spot::Order), Some(req)).await;
        if let Err(e) = &res {
            if let DreamrunnerError::Binance(err) = &e {
                if err.code != -2011 {
                    error!("🛑 Failed to cancel order: {:?}", e);
                    return Err(DreamrunnerError::Binance(err.clone()));
                } else {
                    debug!("No order to cancel");
                }
            }
        }
        res
    }

    pub async fn trade<T: DeserializeOwned>(&self, trade: BinanceTrade) -> DreamrunnerResult<T> {
        let req = trade.request();
        self.client.post_signed::<T>(API::Spot(Spot::Order), req).await
    }

    pub async fn equalize_account_assets(&self) -> DreamrunnerResult<()> {
        info!("Equalizing account assets");
        let account_info = self.account_info().await?;
        let assets = account_info.account_assets(&self.quote_asset, &self.base_asset)?;
        let price = self.price().await?;

        // USDT
        let quote_balance = assets.free_quote / price;
        // BTC
        let base_balance = assets.free_base;

        let sum = quote_balance + base_balance;
        let equal = trunc!(sum / 2_f64, 2);
        let quote_diff = trunc!(quote_balance - equal, 2);
        let base_diff = trunc!(base_balance - equal, 2);
        let min_notional = 0.001;
        info!("sum: {}", sum);
        info!("equal: {}", equal);
        info!("quote_diff: {}", quote_diff);
        info!("base_diff: {}", base_diff);

        // buy BTC
        if quote_diff > 0_f64 && quote_diff > min_notional {
            let timestamp = BinanceTrade::get_timestamp()?;
            let client_order_id = format!("{}-{}", timestamp, "EQUALIZE_QUOTE");
            let long_qty = trunc!(quote_diff, 2);
            info!("long_qty: {}", long_qty);
            info!(
                "Quote asset too high = {} {}, 50/50 = {} {}, buy base asset = {} {}",
                quote_balance * price,
                self.quote_asset,
                equal * price,
                self.quote_asset,
                long_qty,
                self.base_asset
            );
            let buy_base = BinanceTrade::new(
                self.ticker.to_string(),
                client_order_id,
                Side::Long,
                OrderType::Limit,
                long_qty,
                Some(price),
                None,
                None,
                None,
            );
            if let Err(e) = self.trade::<LimitOrderResponse>(buy_base).await {
                error!("🛑 Error equalizing quote asset with error: {:?}", e);
                return Err(e);
            }
        }

        // sell BTC
        if base_diff > 0_f64 && base_diff > min_notional {
            let timestamp = BinanceTrade::get_timestamp()?;
            let client_order_id = format!("{}-{}", timestamp, "EQUALIZE_BASE");
            let short_qty = trunc!(base_diff, 2);
            info!(
                "Base asset too high = {} {}, 50/50 = {} {}, sell base asset = {} {}",
                base_balance, self.base_asset, equal, self.base_asset, short_qty, self.base_asset
            );
            let sell_base = BinanceTrade::new(
                self.ticker.to_string(),
                client_order_id,
                Side::Short,
                OrderType::Limit,
                short_qty,
                Some(price),
                None,
                None,
                None,
            );
            if let Err(e) = self.trade::<LimitOrderResponse>(sell_base).await {
                error!("🛑 Error equalizing base asset with error: {:?}", e);
                return Err(e);
            }
        }

        Ok(())
    }
}
