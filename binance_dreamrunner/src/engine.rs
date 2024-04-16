use std::collections::VecDeque;
use crate::utils::*;
use binance_lib::*;
use log::*;
use serde::de::DeserializeOwned;
use std::time::SystemTime;
use time_series::{trunc, Candle, Time};
use crate::dreamrunner::Dreamrunner;
use crate::utils::{Source, Signal};

#[derive(Clone)]
pub struct Engine {
  pub client: Client,
  pub recv_window: u64,
  pub base_asset: String,
  pub quote_asset: String,
  pub ticker: String,
  pub equity_pct: f64,
  pub active_order: ActiveOrder,
  pub assets: Assets,
  /// Last N candles from current candle.
  /// 0th index is current candle, Nth index is oldest candle.
  pub candles: VecDeque<Candle>,
  pub dreamrunner: Dreamrunner
}

impl Engine {
  #[allow(dead_code)]
  pub fn new(
    client: Client,
    recv_window: u64,
    base_asset: String,
    quote_asset: String,
    ticker: String,
    equity_pct: f64,
    wma_period: usize,
    dreamrunner: Dreamrunner
  ) -> Self {
    let active_order = ActiveOrder::new();
    let candles = VecDeque::with_capacity(wma_period + 1);
    Self {
      client,
      recv_window,
      base_asset,
      quote_asset,
      ticker,
      equity_pct,
      active_order,
      assets: Assets::default(),
      candles,
      dreamrunner
    }
  }

  #[allow(dead_code)]
  pub async fn exchange_info(&self, symbol: String) -> DreamrunnerResult<ExchangeInformation> {
    let req = ExchangeInfo::request(symbol);
    self.client
        .get::<ExchangeInformation>(API::Spot(Spot::ExchangeInfo), Some(req)).await
  }

  /// Place a trade
  pub async fn trade<T: DeserializeOwned>(&self, trade: BinanceTrade) -> DreamrunnerResult<T> {
    let req = trade.request();
    self.client.post_signed::<T>(API::Spot(Spot::Order), req).await
  }
  
  pub async fn trade_or_reset<T: DeserializeOwned>(&mut self, trade: BinanceTrade) -> DreamrunnerResult<T> {
    match self.trade::<T>(trade.clone()).await {
      Ok(res) => Ok(res),
      Err(e) => {
        let order_type = ActiveOrder::client_order_id_suffix(&trade.client_order_id);
        error!(
            "🛑 Error entering {} for {}: {:?}",
            trade.side.fmt_binance(),
            order_type,
            e
        );
        self.reset_active_order().await?;
        Err(e)
      }
    }
  }
  
  fn trade_qty(&self, side: Side, price: f64) -> DreamrunnerResult<f64> {
    let assets = self.assets();
    info!(
        "{}, Free: {}, Locked: {}  |  {}, Free: {}, Locked: {}",
        self.quote_asset,
        assets.free_quote,
        assets.locked_quote,
        self.base_asset,
        assets.free_base,
        assets.locked_base
    );
    let long_qty = assets.free_quote / price * (self.equity_pct / 100_f64);
    let short_qty = assets.free_base * (self.equity_pct / 100_f64);

    Ok(match side {
      Side::Long => trunc!(long_qty, 2),
      Side::Short => trunc!(short_qty, 2)
    })
  }

  fn long_order(&mut self, price: f64, time: Time) -> DreamrunnerResult<OrderBuilder> {
    let long_qty = self.trade_qty(Side::Long, price)?;
    let limit = trunc!(price, 2);
    let timestamp = time.to_unix_ms();
    let entry = BinanceTrade::new(
      self.ticker.to_string(),
      format!("{}-{}", timestamp, "ENTRY"),
      Side::Long,
      OrderType::Limit,
      long_qty,
      Some(limit),
      None,
      None,
      Some(10000),
    );
    Ok(OrderBuilder {
      entry,
    })
  }

  fn short_order(&mut self, price: f64, time: Time) -> DreamrunnerResult<OrderBuilder> {
    let short_qty = self.trade_qty(Side::Short, price)?;
    let limit = trunc!(price, 2);
    let timestamp = time.to_unix_ms();
    let entry = BinanceTrade::new(
      self.ticker.to_string(),
      format!("{}-{}", timestamp, "ENTRY"),
      Side::Short,
      OrderType::Limit,
      short_qty,
      Some(limit),
      None,
      None,
      Some(10000),
    );
    Ok(OrderBuilder {
      entry,
    })
  }

  pub async fn handle_signal(&mut self, signal: Signal) -> DreamrunnerResult<()> {
    match signal {
      Signal::Long((price, time)) => {
        let order = self.long_order(price, time)?;
        self.active_order.add_entry(order.entry.clone());
        self.trade_or_reset::<LimitOrderResponse>(order.entry).await?;
        info!("✅ Long");
        Ok(())
      },
      Signal::Short((price, time)) => {
        let order = self.short_order(price, time)?;
        self.active_order.add_entry(order.entry.clone());
        self.trade_or_reset::<LimitOrderResponse>(order.entry).await?;
        info!("❌ Short");
        Ok(())
      },
      Signal::None => Ok(())
    }
  }
  
  pub async fn process_candle(&mut self, candle: Candle) -> DreamrunnerResult<()> {
    // push to 0th index of VecDeque "candle"
    self.candles.push_front(candle);
    
    if self.active_order.entry.is_none() {
      let signal = self.dreamrunner.signal(self.candles.clone())?;
      if Signal::None != signal {
        info!("{}", signal.print());
        self.handle_signal(signal).await?;
      }
    }
    Ok(())
  }

  pub async fn reset_active_order(&mut self) -> DreamrunnerResult<Vec<OrderCanceled>> {
    self.active_order.reset();
    self.cancel_all_open_orders().await
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
    debug!("Request time: {:?}ms", dur);
    if let Err(e) = res {
      let now = AccountInfo::get_timestamp()?;
      let req_time = builder
        .btree
        .get("timestamp")
        .unwrap()
        .parse::<u64>()
        .unwrap();
      // difference between now and req_time
      let diff = now - req_time;
      error!("🛑 Failed to get account info in {}ms: {:?}", diff, e);
      return Err(e);
    }
    res
  }

  pub async fn update_assets(&mut self) -> DreamrunnerResult<()> {
    let account_info = self.account_info().await?;
    self.assets = account_info.account_assets(&self.quote_asset, &self.base_asset)?;
    Ok(())
  }

  /// Get all assets
  /// Not available on testnet
  #[allow(dead_code)]
  pub async fn all_assets(&self) -> DreamrunnerResult<Vec<CoinInfo>> {
    let req = AllAssets::request(Some(5000));
    self.client
        .get_signed::<Vec<CoinInfo>>(API::Savings(Sapi::AllCoins), Some(req)).await
  }

  /// Get price of the ticker
  pub async fn price(&self) -> DreamrunnerResult<f64> {
    let req = Price::request(self.ticker.to_string());
    let res = self
      .client
      .get::<PriceResponse>(API::Spot(Spot::Price), Some(req)).await?;
    res.price.parse::<f64>().map_err(DreamrunnerError::ParseFloat)
  }

  /// Get historical orders for a single symbol
  #[allow(dead_code)]
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
  #[allow(dead_code)]
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

  pub fn update_active_order(&mut self, event: OrderTradeEvent) -> DreamrunnerResult<()> {
    let id = ActiveOrder::client_order_id_suffix(&event.new_client_order_id);
    match &*id {
      "ENTRY" => {
        self.active_order.entry = Some(PendingOrActiveOrder::Active(
          TradeInfo::from_order_trade_event(&event)?,
        ));
      }
      _ => debug!("Unknown order id: {}", id),
    }
    debug!("{:#?}", event);
    Ok(())
  }

  fn trade_pnl(&self, entry: &TradeInfo, exit: &TradeInfo) -> DreamrunnerResult<f64> {
    Ok(trunc!(
        match entry.side {
            Side::Long => {
                (exit.price - entry.price) / entry.price * 100_f64
            }
            Side::Short => {
                (entry.price - exit.price) / entry.price * 100_f64
            }
        },
        5
    ))
  }

  pub async fn check_active_order(&mut self) -> DreamrunnerResult<()> {
    let copy = self.active_order.clone();
    if let Some(entry) = &copy.entry {
      match entry {
        PendingOrActiveOrder::Active(entry) => {
          // do nothing, order is active
          if entry.status == OrderStatus::Filled {
            info!("Order filled: {:#?}", entry);
            self.reset_active_order().await?;
          }
        }
        PendingOrActiveOrder::Pending(entry) => {}
      }
    }
    Ok(())
  }

  pub async fn equalize_assets(&self) -> DreamrunnerResult<()> {
    info!("Equalizing assets");
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

    // buy BTC
    if quote_diff > 0_f64 && quote_diff > min_notional {
      let timestamp = BinanceTrade::get_timestamp()?;
      let client_order_id = format!("{}-{}", timestamp, "EQUALIZE_QUOTE");
      let long_qty = trunc!(quote_diff, 2);
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

  pub fn assets(&self) -> Assets {
    self.assets.clone()
  }

  pub fn log_assets(&self) {
    let assets = &self.assets;
    info!(
        "Account Assets  |  {}, Free: {}, Locked: {}  |  {}, Free: {}, Locked: {}",
        self.quote_asset,
        assets.free_quote,
        assets.locked_quote,
        self.base_asset,
        assets.free_base,
        assets.locked_base
    );
  }
}
