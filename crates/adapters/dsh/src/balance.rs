use std::time::Duration;

use wanxiang_core::{parse_minor_units, BalanceAmount, Currency, UsageError};

pub fn parse_balance_json(text: &str) -> Result<Vec<BalanceAmount>, UsageError> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| UsageError::QueryFailed)?;
    let infos = value
        .get("balance_infos")
        .and_then(|item| item.as_array())
        .ok_or(UsageError::QueryFailed)?;
    if infos.is_empty() {
        return Err(UsageError::QueryFailed);
    }
    let mut balances = Vec::new();
    for info in infos {
        let currency = info
            .get("currency")
            .and_then(|item| item.as_str())
            .and_then(Currency::parse)
            .ok_or(UsageError::QueryFailed)?;
        let total_minor = minor(info, "total_balance")?;
        let granted_minor = minor(info, "granted_balance")?;
        let topped_up_minor = minor(info, "topped_up_balance")?;
        balances.push(BalanceAmount {
            currency,
            total_minor,
            granted_minor,
            topped_up_minor,
        });
    }
    Ok(balances)
}

fn minor(info: &serde_json::Value, key: &str) -> Result<i64, UsageError> {
    info.get(key)
        .and_then(|item| item.as_str())
        .and_then(parse_minor_units)
        .ok_or(UsageError::QueryFailed)
}

pub fn fetch_summary(base: &str, key: &str, body: &str) -> Result<String, UsageError> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .redirects(0)
        .build();
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));
    let header = format!("Bearer {key}");
    let response = agent
        .post(&url)
        .set("Authorization", &header)
        .set("Content-Type", "application/json")
        .send_string(body);
    drop(header);
    match response {
        Ok(response) => {
            let text = response.into_string().map_err(|_| UsageError::QueryFailed)?;
            summary_content(&text)
        }
        Err(ureq::Error::Status(401 | 403, response)) => {
            let _ = response.into_string();
            Err(UsageError::InvalidKey)
        }
        Err(ureq::Error::Status(_, response)) => {
            let _ = response.into_string();
            Err(UsageError::QueryFailed)
        }
        Err(_) => Err(UsageError::Network),
    }
}

fn summary_content(text: &str) -> Result<String, UsageError> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| UsageError::QueryFailed)?;
    value
        .pointer("/choices/0/message/content")
        .and_then(|item| item.as_str())
        .map(str::to_string)
        .ok_or(UsageError::QueryFailed)
}

pub fn fetch_balance(base: &str, key: &str) -> Result<Vec<BalanceAmount>, UsageError> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .redirects(0)
        .build();
    let url = format!("{}/user/balance", base.trim_end_matches('/'));
    let header = format!("Bearer {key}");
    let response = agent.get(&url).set("Authorization", &header).call();
    drop(header);
    match response {
        Ok(response) => {
            let text = response.into_string().map_err(|_| UsageError::QueryFailed)?;
            parse_balance_json(&text)
        }
        Err(ureq::Error::Status(401 | 403, response)) => {
            let _ = response.into_string();
            Err(UsageError::InvalidKey)
        }
        Err(ureq::Error::Status(_, response)) => {
            let _ = response.into_string();
            Err(UsageError::QueryFailed)
        }
        Err(_) => Err(UsageError::Network),
    }
}
