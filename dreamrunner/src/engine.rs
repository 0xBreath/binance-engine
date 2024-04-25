#![allow(dead_code)]

use lib::*;
use log::*;
use serde::de::DeserializeOwned;
use std::time::SystemTime;
use crossbeam::channel::Receiver;
use lib::trade::*;
use time_series::{trunc, Candle, Time, Signal, RollingCandles, Kagi, KagiDirection};
use crate::dreamrunner::Dreamrunner;

pub struct Engine {
  pub client: Client,
  pub rx: Receiver<WebSocketEvent>,
  pub disable_trading: bool,
  pub base_asset: String,
  pub quote_asset: String,
  pub ticker: String,
  pub interval: Interval,
  pub min_notional: f64,
  pub equity_pct: f64,
  pub active_order: ActiveOrder,
  pub assets: Assets,
  /// Last N candles from current candle.
  /// 0th index is current candle, Nth index is oldest candle.
  pub candles: RollingCandles,
  pub kagi: Kagi,
  pub dreamrunner: Dreamrunner
}

impl Engine {
  #[allow(clippy::too_many_arguments)]
  pub fn new(
    client: Client,
    rx: Receiver<WebSocketEvent>,
    disable_trading: bool,
    base_asset: String,
    quote_asset: String,
    ticker: String,
    interval: Interval,
    min_notional: f64,
    equity_pct: f64,
    wma_period: usize,
    dreamrunner: Dreamrunner,
  ) -> Self {
    Self {
      client: client.clone(),
      rx,
      disable_trading,
      base_asset,
      quote_asset,
      ticker,
      interval,
      min_notional,
      equity_pct,
      active_order: ActiveOrder::new(),
      assets: Assets::default(),
      candles: RollingCandles::new(wma_period + 1),
      kagi: Kagi {
        direction: KagiDirection::Up,
        line: 0.0
      },
      dreamrunner
    }
  }

  pub async fn ignition(&mut self) -> DreamrunnerResult<()> {
    if !self.disable_trading {
      // cancel all open orders to start with a clean slate
      self.cancel_all_open_orders().await?;
      // equalize base and quote assets to 50/50
      self.equalize_assets().await?;
    }
    // get initial asset balances
    self.update_assets().await?;
    self.log_assets();
    // load one less than required rolling period.
    // if we fetch the entire period, the most recent candle could be old.
    // for example: 15m candles, closed at 1:00pm, we fetch at 1:14pm, we trade using old data.
    // so we fetch one less than the rolling period and wait for the next candle to close to ensure we trade immediately.
    self.load_recent_candles(Some((self.candles.capacity - 1) as u16)).await?;

    info!("🚀 Starting Dreamrunner!");
    while let Ok(event) = self.rx.recv() {
      match event {
        WebSocketEvent::Kline(kline) => {
          // only accept if this candle is at the end of the bar period
          if kline.kline.is_final_bar {
            info!("{:#?}", kline.kline.info()?);
            self.process_candle(kline.kline.to_candle()?).await?;
          }
        }
        WebSocketEvent::AccountUpdate(account_update) => {
          let assets = account_update.assets(&self.quote_asset, &self.base_asset)?;
          info!(
            "Account Update, {}: {}, {}: {}",
            self.quote_asset, assets.free_quote, self.base_asset, assets.free_base
          );
        }
        WebSocketEvent::OrderTrade(event) => {
          let entry_price = trunc!(event.price.parse::<f64>()?, 2);
          info!(
                "{},  {},  {} @ {}, {}",
                event.symbol,
                event.new_client_order_id,
                event.side,
                entry_price,
                event.order_status,
              );
          // update state
          self.update_active_order(event)?;
          // create or cancel orders depending on state
          self.check_active_order().await?;
        }
        _ => (),
      };
    }
    warn!("🟡 Shutting down engine");
    Ok(())
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
      Time::now().to_unix_ms(),
      None,
      None
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
      Time::now().to_unix_ms(),
      None,
      None
    );
    Ok(OrderBuilder {
      entry,
    })
  }

  pub async fn handle_signal(&mut self, signal: Signal) -> DreamrunnerResult<()> {
    if !self.disable_trading {
      return Ok(());
    }
    match signal {
      Signal::Long((price, time)) => {
        let order = self.long_order(price, time)?;
        self.active_order.add_entry(order.entry.clone());
        self.trade_or_reset::<LimitOrderResponse>(order.entry).await?;
        Ok(())
      },
      Signal::Short((price, time)) => {
        let order = self.short_order(price, time)?;
        self.active_order.add_entry(order.entry.clone());
        self.trade_or_reset::<LimitOrderResponse>(order.entry).await?;
        Ok(())
      },
      Signal::None => Ok(())
    }
  }

  pub async fn process_candle(&mut self, candle: Candle) -> DreamrunnerResult<()> {
    // pushes to front of VecDeque and pops the back if at capacity
    self.candles.push(candle);
    
    match &self.active_order.entry {
      None => {
        let signal = self.dreamrunner.signal(&mut self.kagi, &self.candles)?;
        if Signal::None != signal {
          info!("{}", signal.print());
          if !self.disable_trading {
            self.update_assets().await?;
            self.handle_signal(signal).await?;
          }
        }
      }
      Some(entry) => {
        // cancel order if not completely filled within 10 minutes
        match entry {
          PendingOrActiveOrder::Pending(order) => {
            let placed_at = Time::from_unix_ms(order.timestamp);
            if Time::now().diff_minutes(&placed_at)? > 10 {
              self.reset_active_order().await?;
            }
          }
          PendingOrActiveOrder::Active(order) => {
            if order.status == OrderStatus::PartiallyFilled || order.status == OrderStatus::New {
              let placed_at = Time::from_unix_ms(order.event_time);
              if Time::now().diff_minutes(&placed_at)? > 10 {
                self.reset_active_order().await?;
              }
            }
          }
        }
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
    orders.sort_by(|a, b| b.update_time.cmp(&a.update_time));
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
        PendingOrActiveOrder::Pending(_) => {}
      }
    }
    Ok(())
  }

  pub async fn equalize_assets(&self) -> DreamrunnerResult<()> {
    if self.disable_trading {
      return Ok(());
    }
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

    // buy base asset
    if quote_diff > 0_f64 && quote_diff > self.min_notional {
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
        Time::now().to_unix_ms(),
        None,
        None
      );
      if let Err(e) = self.trade::<LimitOrderResponse>(buy_base).await {
        error!("🛑 Error equalizing quote asset with error: {:?}", e);
        return Err(e);
      }
    }

    // sell base asset
    if base_diff > 0_f64 && base_diff > self.min_notional {
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
        Time::now().to_unix_ms(),
        None,
        None
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

  pub async fn klines(&self, limit: Option<u16>, start_time: Option<i64>, end_time: Option<i64>) -> DreamrunnerResult<Vec<Kline>> {
    let req = Klines::request(self.ticker.to_string(), self.interval.as_str(), limit, start_time, end_time);
    let mut klines = self.client
      .get::<Vec<serde_json::Value>>(API::Spot(Spot::Klines), Some(req)).await?
      .into_iter()
      .flat_map(Kline::try_from)
      .collect::<Vec<Kline>>();
    klines.sort_by(|a, b| b.open_time.cmp(&a.open_time));
    Ok(klines)
  }

  pub async fn load_recent_candles(&mut self, limit: Option<u16>) -> DreamrunnerResult<()> {
    let klines = self.klines(limit, None, None).await?;
    for kline in klines {
      self.candles.push(kline.to_candle());
    }
    Ok(())
  }
}
