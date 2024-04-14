use crate::error::*;
use crate::utils::*;
use apca::api::v2::account::{Account, Get as GetAccount};
use apca::api::v2::asset::Symbol;
use apca::api::v2::order::*;
use apca::api::v2::orders::{Get as GetOrders, OrdersReq, Status as OrdersStatus};
use apca::api::v2::position::{Get as GetPosition, Position};
use apca::api::v2::updates::OrderUpdate;
use apca::data::v2::bars::{BarsReqInit, Get as GetBars, TimeFrame};
use apca::Client;
use ephemeris::PLPLSystem;
use log::*;
use num_decimal::Num;
use time_series::{f64_to_num, num_to_f64, num_unwrap_f64, precise_round, Candle, Time};

#[derive(Debug, Clone)]
pub struct ActiveOrder {
    pub entry: Option<Order>,
    pub take_profit: Option<Order>,
    pub take_profit_handler: TakeProfitHandler,
    pub stop_loss: Option<Order>,
    pub stop_loss_handler: StopLossHandler,
}

impl ActiveOrder {
    pub fn new(take_profit_handler: TakeProfitHandler, stop_loss_handler: StopLossHandler) -> Self {
        Self {
            entry: None,
            take_profit: None,
            take_profit_handler,
            stop_loss: None,
            stop_loss_handler,
        }
    }

    pub fn add_entry(&mut self, entry: Order) {
        self.entry = Some(entry);
    }

    pub fn add_exits(&mut self, take_profit: Order, stop_loss: Order) {
        self.take_profit = Some(take_profit);
        self.stop_loss = Some(stop_loss);
    }

    pub fn reset(&mut self) {
        self.entry = None;
        self.take_profit = None;
        self.stop_loss = None;
    }
}

pub struct Engine {
    pub client: Client,
    pub ticker: String,
    pub plpl_system: PLPLSystem,
    pub trailing_take_profit: ExitType,
    pub stop_loss: ExitType,
    pub active_order: ActiveOrder,
}

impl Engine {
    pub fn new(
        client: Client,
        ticker: String,
        plpl_system: PLPLSystem,
        trailing_take_profit: ExitType,
        stop_loss: ExitType,
    ) -> Self {
        let take_profit_handler = TakeProfitHandler::new(trailing_take_profit.clone());
        let stop_loss_handler = StopLossHandler::new(stop_loss.clone());
        let active_order = ActiveOrder::new(take_profit_handler, stop_loss_handler);
        Self {
            client,
            ticker,
            plpl_system,
            trailing_take_profit,
            stop_loss,
            active_order,
        }
    }

    async fn account(&self) -> Result<Account> {
        let res = self.client.issue::<GetAccount>(&()).await;
        trace!("Get account: {:?}", res);
        res.map_err(AlpacaError::ApcaGetAccount)
    }

    // TODO: fix this
    pub async fn candle(&self) -> Result<Candle> {
        let start = chrono::Utc::now() - chrono::Duration::minutes(3);
        let end = chrono::Utc::now();
        let request = BarsReqInit {
            limit: Some(1),
            ..Default::default()
        }
        .init(&self.ticker, start, end, TimeFrame::OneMinute);

        let res = match self.client.issue::<GetBars>(&request).await {
            Ok(res) => res,
            Err(e) => {
                error!("Failed to get bars: {:?}", e);
                return Err(AlpacaError::from(e));
            }
        };
        let bars = res.bars;
        if bars.len() != 1 {
            return Err(AlpacaError::BarsEmpty);
        }
        let bar = bars[0].clone();
        Ok(Candle {
            date: Time::from_datetime(bar.time),
            open: num_to_f64!(bar.open)?,
            high: num_to_f64!(bar.high)?,
            low: num_to_f64!(bar.low)?,
            close: num_to_f64!(bar.close)?,
            volume: None,
        })
    }

    async fn position(&self) -> Option<Position> {
        let symbol = Symbol::Sym(self.ticker.to_string());
        match self.client.issue::<GetPosition>(&symbol).await {
            Ok(res) => Some(res),
            Err(e) => {
                warn!("No position {e}");
                None
            }
        }
    }

    /// Cancel open orders for this ticker
    pub async fn cancel_open_orders(&self) -> Result<()> {
        info!("Canceling open orders");
        let request = OrdersReq {
            status: OrdersStatus::Open,
            ..Default::default()
        };
        let orders = self.client.issue::<GetOrders>(&request).await?;
        for order in orders {
            match self.client.issue::<Delete>(&order.id).await {
                Ok(_) => debug!("Canceled open order: {}", order.client_order_id),
                Err(e) => {
                    error!("Failed to cancel open order: {:?}", e);
                }
            }
        }
        Ok(())
    }

    pub async fn equalize_assets(&self) -> Result<()> {
        info!("Equalizing assets");
        // get quote balance for ticker (cash, USD, USDT, etc)
        let account = self.account().await?;
        // get base balance for ticker
        let ticker_position = self.position().await;
        match ticker_position {
            Some(ticker_position) => {
                let qty = num_to_f64!(ticker_position.quantity)?;

                // both converted to base units (i.e. BTC units for BTC/USD)
                let base_balance = num_unwrap_f64!(ticker_position.market_value)?;
                let price = base_balance / qty;
                let quote_balance = num_to_f64!(account.cash)? / price;

                let sum = quote_balance + base_balance;
                let equal = precise_round!(sum / 2_f64, 5);
                let quote_diff = precise_round!(quote_balance - equal, 5);
                let base_diff = precise_round!(base_balance - equal, 5);
                let min_notional = 0.001;

                // buy base asset
                if quote_diff > 0_f64 && quote_diff > min_notional {
                    let long_qty = precise_round!(quote_diff, 5);
                    let long = OrderReqInit {
                        type_: Type::Limit,
                        limit_price: Some(f64_to_num!(price)),
                        client_order_id: Some(format!(
                            "{}-{}",
                            chrono::Utc::now().naive_utc().timestamp(),
                            "EQUALIZE_LONG"
                        )),
                        time_in_force: TimeInForce::UntilCanceled,
                        ..Default::default()
                    }
                    .init(
                        &self.ticker,
                        Side::Buy,
                        Amount::quantity(f64_to_num!(long_qty)),
                    );
                    return match self.client.issue::<Post>(&long).await {
                        Ok(_) => {
                            info!(
                                "Quote asset too high = {}, 50/50 = {}, buy base asset = {}",
                                quote_balance * price,
                                equal * price,
                                long_qty,
                            );
                            Ok(())
                        }
                        Err(e) => {
                            error!("Failed to buy base asset to equalize: {:?}", e);
                            Err(AlpacaError::from(e))
                        }
                    };
                }
                // sell base asset
                else if base_diff > 0_f64 && base_diff > min_notional {
                    let short_qty = precise_round!(base_diff, 5);
                    let short = OrderReqInit {
                        type_: Type::Limit,
                        limit_price: Some(f64_to_num!(price)),
                        client_order_id: Some(format!(
                            "{}-{}",
                            chrono::Utc::now().naive_utc().timestamp(),
                            "EQUALIZE_SHORT"
                        )),
                        time_in_force: TimeInForce::UntilCanceled,
                        ..Default::default()
                    }
                    .init(
                        &self.ticker,
                        Side::Sell,
                        Amount::quantity(f64_to_num!(short_qty)),
                    );
                    return match self.client.issue::<Post>(&short).await {
                        Ok(_) => {
                            info!(
                                "Base asset too high = {}, 50/50 = {}, sell base asset = {}",
                                base_balance, equal, short_qty,
                            );
                            Ok(())
                        }
                        Err(e) => {
                            error!("Failed to sell base asset to equalize: {:?}", e);
                            Err(AlpacaError::from(e))
                        }
                    };
                }
            }
            None => {
                let cash = num_to_f64!(account.cash)?;
                let long_notional = precise_round!(cash / 2.0, 2);
                let long = OrderReqInit {
                    type_: Type::Market,
                    client_order_id: Some(format!(
                        "{}-{}",
                        chrono::Utc::now().naive_utc().timestamp(),
                        "EQUALIZE_LONG"
                    )),
                    time_in_force: TimeInForce::UntilCanceled,
                    ..Default::default()
                }
                .init(
                    &self.ticker,
                    Side::Buy,
                    Amount::notional(f64_to_num!(long_notional)),
                );
                return match self.client.issue::<Post>(&long).await {
                    Ok(_) => {
                        info!(
                            "Quote asset too high = {}, 50/50 = {}, buy base asset = {}",
                            cash,
                            cash - long_notional,
                            long_notional,
                        );
                        Ok(())
                    }
                    Err(e) => {
                        error!("Failed to buy base asset to equalize: {:?}", e);
                        Err(AlpacaError::from(e))
                    }
                };
            }
        }
        Ok(())
    }

    fn take_profit_pnl(&self, entry: &Option<Order>, take_profit: &Option<Order>) -> Result<f64> {
        match (entry, take_profit) {
            (Some(entry), Some(take_profit)) => {
                let entry_price = num_unwrap_f64!(entry.clone().average_fill_price)?;
                let take_profit_price = num_unwrap_f64!(take_profit.clone().average_fill_price)?;
                let pnl = precise_round!(
                    match entry.side {
                        Side::Buy => {
                            (take_profit_price - entry_price) / entry_price * 100_f64
                        }
                        Side::Sell => {
                            (entry_price - take_profit_price) / entry_price * 100_f64
                        }
                    },
                    5
                );
                Ok(pnl)
            }
            _ => Err(AlpacaError::NumUnwrap),
        }
    }

    fn stop_loss_pnl(&self, entry: &Option<Order>, stop_loss: &Option<Order>) -> Result<f64> {
        match (entry, stop_loss) {
            (Some(entry), Some(stop_loss)) => {
                let entry_price = num_unwrap_f64!(entry.clone().average_fill_price)?;
                let stop_loss_price = num_unwrap_f64!(stop_loss.clone().average_fill_price)?;
                let pnl = precise_round!(
                    match entry.side {
                        Side::Buy => {
                            (stop_loss_price - entry_price) / entry_price * 100_f64
                        }
                        Side::Sell => {
                            (entry_price - stop_loss_price) / entry_price * 100_f64
                        }
                    },
                    5
                );
                Ok(pnl)
            }
            _ => Err(AlpacaError::NumUnwrap),
        }
    }

    pub async fn process_candle(
        &mut self,
        prev_candle: &Candle,
        candle: &Candle,
        timestamp: String,
    ) -> Result<()> {
        let plpl = self.plpl_system.closest_plpl(candle)?;
        if self.plpl_system.long_signal(prev_candle, candle, plpl) {
            self.handle_signal(candle, timestamp, Side::Buy).await?;
        } else if self.plpl_system.short_signal(prev_candle, candle, plpl) {
            self.handle_signal(candle, timestamp, Side::Sell).await?;
        }

        Ok(())
    }

    async fn handle_signal(
        &mut self,
        candle: &Candle,
        timestamp: String,
        side: Side,
    ) -> Result<()> {
        if self.active_order.entry.is_none() {
            let account = self.account().await?;
            let cash = num_to_f64!(account.cash)?;
            info!("Cash: {}", cash);
            let entry = self
                .create_entry_order(candle, timestamp, side, cash)
                .await?;
            self.active_order.add_entry(entry);

            match side {
                Side::Buy => info!("🟢 Long"),
                Side::Sell => info!("🔴Short"),
            };
        }
        Ok(())
    }

    pub fn update_active_order(&mut self, order: OrderUpdate) -> Result<()> {
        let id = order_id_suffix(&order.order);
        match &*id {
            "ENTRY" => {
                self.active_order.entry = Some(order.order);
            }
            "TAKE_PROFIT" => {
                self.active_order.take_profit = Some(order.order);
            }
            "STOP_LOSS" => {
                self.active_order.stop_loss = Some(order.order);
            }
            _ => debug!("Unknown order id: {}", id),
        }
        self.log_active_order()?;
        Ok(())
    }

    pub fn log_active_order(&self) -> Result<()> {
        let id = match &self.active_order.entry {
            Some(entry) => order_id_prefix(entry),
            None => "None".to_string(),
        };
        let entry_price = match &self.active_order.entry {
            Some(entry) => match &entry.limit_price {
                None => "None".to_string(),
                Some(price) => num_to_f64!(price)?.to_string(),
            },
            None => "None".to_string(),
        };
        let entry_status = match &self.active_order.entry {
            Some(entry) => status_to_string(entry.status),
            None => "None".to_string(),
        };
        let tp_price = match &self.active_order.take_profit {
            None => "None".to_string(),
            Some(tp) => match &tp.average_fill_price {
                None => "None".to_string(),
                Some(price) => num_to_f64!(price)?.to_string(),
            },
        };
        let tp_status = match &self.active_order.take_profit {
            Some(tp) => status_to_string(tp.status),
            None => "None".to_string(),
        };
        let sl_price = match &self.active_order.stop_loss {
            None => "None".to_string(),
            Some(sl) => match &sl.average_fill_price {
                None => "None".to_string(),
                Some(price) => num_to_f64!(price)?.to_string(),
            },
        };
        let sl_status = match &self.active_order.stop_loss {
            Some(sl) => status_to_string(sl.status),
            None => "None".to_string(),
        };
        info!(
            "Active Order, ID: {}, Entry: {} @ {}, TP: {} @ {}, SL: {} @ {}",
            id, entry_status, entry_price, tp_status, tp_price, sl_status, sl_price
        );
        Ok(())
    }

    pub async fn check_active_order(&mut self) -> Result<()> {
        let copy = self.active_order.clone();
        match (&copy.entry, &copy.take_profit, &copy.stop_loss) {
            (Some(entry), None, None) => {
                if entry.status == Status::Filled {
                    let take_profit = self.create_take_profit_order().await?;
                    let stop_loss = self.create_stop_loss_order().await?;
                    self.active_order.add_exits(take_profit, stop_loss);
                }
            }
            // OCO is done in websocket trade update handling function
            // so just validate that active order is not in a bad state
            (Some(_), Some(tp), Some(sl)) => {
                if tp.status == Status::Filled && sl.status == Status::New {
                    // take profit filled, so cancel stop loss
                    self.client.issue::<Delete>(&sl.id).await?;
                    self.active_order.reset();
                    info!("✅ Take profit filled, canceled stop loss");
                    let pnl = self.take_profit_pnl(
                        &self.active_order.entry,
                        &self.active_order.take_profit,
                    )?;
                    info!("📈 PNL: {}%", pnl);
                }
                if sl.status == Status::Filled && tp.status == Status::New {
                    // stop loss filled, so cancel take profit
                    self.client.issue::<Delete>(&tp.id).await?;
                    self.active_order.reset();
                    info!("❌ Stop loss filled, canceled take profit");
                    let pnl =
                        self.stop_loss_pnl(&self.active_order.entry, &self.active_order.stop_loss)?;
                    info!("📈 PNL: {}%", pnl);
                }
                if tp.status == Status::Filled && sl.status == Status::Filled {
                    error!("Both take profit and stop loss are filled");
                    self.active_order.reset();
                    return Err(AlpacaError::BothExitsFilled);
                }
            }
            _ => {
                return Err(AlpacaError::InvalidActiveOrder);
            }
        }

        Ok(())
    }

    /// Quantity is equal to 1/3 of account cash
    async fn create_entry_order(
        &self,
        candle: &Candle,
        timestamp: String,
        side: Side,
        cash: f64,
    ) -> Result<Order> {
        let entry = OrderReqInit {
            type_: Type::Limit,
            limit_price: Some(f64_to_num!(candle.close)),
            client_order_id: Some(format!("{}-{}", timestamp, "ENTRY")),
            time_in_force: TimeInForce::UntilCanceled,
            extended_hours: true,
            ..Default::default()
        }
        .init(
            &self.ticker,
            side,
            Amount::quantity(f64_to_num!(precise_round!(cash / 3.0 / candle.close, 0))),
        );
        debug!("Entry order: {:?}", entry);
        match self.client.issue::<Post>(&entry).await {
            Ok(res) => {
                info!("Entry order response: {:?}", res);
                Ok(res)
            }
            Err(e) => {
                error!("Failed to create entry order: {:?}", e);
                Err(AlpacaError::from(e))
            }
        }
    }

    async fn create_take_profit_order(&self) -> Result<Order> {
        match &self.active_order.entry {
            None => {
                error!("Entry order should exist before creating take profit order");
                Err(AlpacaError::NoEntryOrder)
            }
            Some(entry) => {
                let tp_side = match entry.side {
                    Side::Buy => Side::Sell,
                    Side::Sell => Side::Buy,
                };
                let tp = OrderReqInit {
                    class: Class::Simple,
                    type_: Type::TrailingStop,
                    client_order_id: Some(format!("{}-{}", order_id_prefix(entry), "TAKE_PROFIT")),
                    trail_price: self.active_order.take_profit_handler.trail_price.clone(),
                    trail_percent: self.active_order.take_profit_handler.trail_percent.clone(),
                    time_in_force: TimeInForce::UntilCanceled,
                    extended_hours: true,
                    ..Default::default()
                }
                .init(
                    &self.ticker,
                    tp_side,
                    Amount::quantity(entry.filled_quantity.clone()),
                );
                debug!("Take profit order: {:?}", tp);
                match self.client.issue::<Post>(&tp).await {
                    Ok(res) => {
                        info!("Take profit order response: {:?}", res);
                        Ok(res)
                    }
                    Err(e) => {
                        error!("Failed to create take profit order: {:?}", e);
                        Err(AlpacaError::from(e))
                    }
                }
            }
        }
    }

    async fn create_stop_loss_order(&self) -> Result<Order> {
        match &self.active_order.entry {
            None => {
                error!("Entry order should exist before creating stop loss order");
                Err(AlpacaError::NoEntryOrder)
            }
            Some(entry) => {
                let sl_side = match entry.side {
                    Side::Buy => Side::Sell,
                    Side::Sell => Side::Buy,
                };
                let entry_price = num_unwrap_f64!(entry.limit_price.clone())?;
                let (stop_price, limit_price) = self
                    .active_order
                    .stop_loss_handler
                    .build(entry_price, entry.side)?;
                let sl = OrderReqInit {
                    class: Class::Simple,
                    type_: Type::Limit,
                    stop_loss: Some(StopLoss::StopLimit(stop_price, limit_price)),
                    client_order_id: Some(format!("{}-{}", order_id_prefix(entry), "STOP_LOSS")),
                    time_in_force: TimeInForce::UntilCanceled,
                    extended_hours: true,
                    ..Default::default()
                }
                .init(
                    &self.ticker,
                    sl_side,
                    Amount::quantity(entry.filled_quantity.clone()),
                );
                debug!("Stop loss order: {:?}", sl);
                match self.client.issue::<Post>(&sl).await {
                    Ok(res) => {
                        info!("Stop loss order response: {:?}", res);
                        Ok(res)
                    }
                    Err(e) => {
                        error!("Failed to create stop loss order: {:?}", e);
                        Err(AlpacaError::from(e))
                    }
                }
            }
        }
    }
}
