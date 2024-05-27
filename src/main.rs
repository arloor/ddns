use anyhow::{anyhow, Error};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::thread::sleep;
use std::time::Duration;

// 每隔几次强制从dnspod获取最新的记录
const FORCE_GET_RECORD_INTERVAL: i8 = 5;
// 间隔时间
const SLEEP_SECS: u64 = 120;

#[derive(Serialize, Deserialize)]
struct Res {
    records: Vec<Record>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Record {
    id: String,
    name: String,
    value: String,
    updated_on: String,
    line_id: String,
}

/// 从环境变量中读取domain、sub_domain、token
fn main() -> Result<(), Error> {
    log_x::init_log("log", "ddns.log")?;
    let token = match env::var("dnspod_token") {
        Ok(token) => token,
        Err(_) => {
            match env::var("DNSPOD_TOKEN") {
                Ok(token) => token,
                Err(_) => panic!("dnspod_token/DNSPOD_TOKEN is not set"),
            }
        }
    };  
    let domain = env::var("dnspod_domain").expect("dnspod_domain is not set");
    let sub_domain = env::var("dnspod_subdomain").expect("dnspod_subdomain is not set");
    let ip_url = env::var("dnspod_ip_url").unwrap_or("http://whatismyip.akamai.com".to_string());
    info!(
        "monitor current ip by [{}] and modify [{}.{}] with token [{}]",
        ip_url, sub_domain, domain, token
    );
    let mut latest_ip = "".to_string();

    let mut i = 0;
    loop {
        let current_ip = current_ip(&ip_url);
        if let Ok(current_ip) = current_ip {
            // let current_ip = "127.0.0.1".to_string();
            info!("current ip = {}", current_ip);
            if current_ip != latest_ip || i % FORCE_GET_RECORD_INTERVAL == 0 {
                match get_record(&domain, &sub_domain, &token) {
                    Ok(Some(record)) => {
                        modify_record(&current_ip, &record, &token, &domain);
                    }
                    Ok(None) => {
                        info!("no such record: {}.{}", sub_domain, domain);
                        add_record(&current_ip, &token, &domain, &sub_domain);
                    }
                    Err(e) => {
                        warn!("error get record: {}", e);
                    }
                }
                latest_ip = current_ip;
            }
        } else if let Err(e) = current_ip {
            error!("error fetch current ip: {}", e)
        }
        sleep(Duration::from_secs(SLEEP_SECS));
        i += 1;
    }
}

fn current_ip(ip_url: &str) -> Result<String, Error> {
    let result = reqwest::blocking::get(ip_url);
    match result {
        Ok(ip) => match ip.text() {
            Ok(text) => Ok(text),
            Err(e) => Err(anyhow!(e)),
        },
        Err(e) => Err(anyhow!(e)),
    }
}

fn get_record(domain: &str, sub_domain: &str, token: &str) -> Result<Option<Record>, Error> {
    let mut params = HashMap::new();
    params.insert("login_token", token);
    params.insert("format", "json");
    params.insert("error_on_empty", "no");
    params.insert("lang", "en");
    params.insert("domain", domain);
    params.insert("sub_domain", sub_domain);

    let client = reqwest::blocking::Client::new();
    let res = client
        .post("https://dnsapi.cn/Record.List")
        .form(&params)
        .send();
    let text = res?.text()?;
    let result: serde_json::Result<Res> = serde_json::from_str(&text);
    match result {
        Ok(res) => {
            if !res.records.is_empty() {
                info!("current record is {:?}", res.records[0]);
                Ok(Some(res.records[0].clone()))
            } else {
                Ok(None)
            }
        }
        Err(err) => {
            warn!("error parse result: {}", text);
            Err(anyhow!(err))
        }
    }
}

fn modify_record(current_ip: &String, record: &Record, token: &str, domain: &str) {
    if &record.value != current_ip {
        let client = reqwest::blocking::Client::new();
        let mut params = HashMap::new();
        params.insert("login_token", token);
        params.insert("format", "json");
        params.insert("error_on_empty", "no");
        params.insert("lang", "en");
        params.insert("domain", domain);
        params.insert("sub_domain", &record.name);
        params.insert("record_id", &record.id);
        params.insert("record_line_id", &record.line_id);
        params.insert("value", current_ip);
        let res = client
            .post("https://dnsapi.cn/Record.Ddns")
            .form(&params)
            .send();
        if let Ok(res) = res {
            let text = res.text();
            if let Ok(text) = text {
                info!("modify result is： {}", text);
            }
        } else {
            info!("error modify record");
        }
    }
}

fn add_record(current_ip: &String, token: &str, domain: &str, sub_domain: &str) {
    let client = reqwest::blocking::Client::new();
    let mut params = HashMap::new();
    params.insert("login_token", token);
    params.insert("format", "json");
    params.insert("error_on_empty", "no");
    params.insert("lang", "en");
    params.insert("domain", domain);
    params.insert("sub_domain", sub_domain);
    params.insert("record_type", "A");
    params.insert("record_line", "默认");
    params.insert("value", current_ip);
    let res = client
        .post("https://dnsapi.cn/Record.Create")
        .form(&params)
        .send();
    if let Ok(res) = res {
        let text = res.text();
        if let Ok(text) = text {
            info!("add result is： {}", text);
        }
    } else {
        info!("error add record");
    }
}
