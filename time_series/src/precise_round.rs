#[macro_export]
macro_rules! precise_round {
    ($num:expr, $decimals:expr) => {{
        let factor = 10.0_f64.powi($decimals);
        ($num * factor).round() / factor
    }};
}

#[macro_export]
macro_rules! f64_to_num {
    ($num:expr) => {{
        let decimals = $num.to_string().split('.').last().unwrap().len();
        let denominator = 10_f64.powi(decimals as i32);
        let numerator = $num * denominator;
        Num::new(numerator as i64, denominator as u64)
    }};
}

#[macro_export]
macro_rules! num_to_f64 {
    ($num:expr) => {{
        $num.to_f64().ok_or(AlpacaError::NumCastF64)
    }};
}

#[macro_export]
macro_rules! num_unwrap_f64 {
    ($num:expr) => {{
        match $num {
            Some(v) => v.to_f64().ok_or(AlpacaError::NumCastF64),
            None => Err(AlpacaError::NumUnwrap),
        }
    }};
}

#[macro_export]
macro_rules! num_unwrap {
    ($num:expr) => {{
        match $num {
            Some(v) => Ok(v),
            None => Err(AlpacaError::NumUnwrap),
        }
    }};
}
