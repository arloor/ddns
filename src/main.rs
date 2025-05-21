use anyhow::{anyhow, Error};
use log::{error, info};
use std::env;
use std::thread::sleep;
use std::time::Duration;

// 每隔几次强制从dnspod获取最新的记录
const FORCE_GET_RECORD_INTERVAL: i8 = 5;
// 间隔时间
const SLEEP_SECS: u64 = 120;

/// 获取当前IP地址
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

/// 从环境变量中读取domain、sub_domain、token
fn main() -> Result<(), Error> {
    log_x::init_log("log", "dnspod.log")?;
    let token = match env::var("dnspod_token") {
        Ok(token) => token,
        Err(_) => match env::var("DNSPOD_TOKEN") {
            Ok(token) => token,
            Err(_) => panic!("dnspod_token/DNSPOD_TOKEN is not set"),
        },
    };
    let domain = env::var("dnspod_domain").expect("dnspod_domain is not set");
    let sub_domain = env::var("dnspod_subdomain").expect("dnspod_subdomain is not set");
    let ip_url = env::var("dnspod_ip_url").unwrap_or("http://whatismyip.akamai.com".to_string());

    // 初始化DNSPod配置
    let client = dnspod::init(token, domain, sub_domain);

    let mut latest_ip = "".to_string();

    let mut i = 0;
    loop {
        let current_ip = current_ip(&ip_url);
        if let Ok(current_ip) = current_ip {
            // let current_ip = "127.0.0.1".to_string();
            info!("current ip = {}", current_ip);
            if current_ip != latest_ip || i % FORCE_GET_RECORD_INTERVAL == 0 {
                if let Err(e) = client.update_dns_record(&current_ip) {
                    error!("Failed to update DNS record: {}", e);
                } else {
                    latest_ip = current_ip;
                }
            }
        } else if let Err(e) = current_ip {
            error!("error fetch current ip: {}", e)
        }
        sleep(Duration::from_secs(SLEEP_SECS));
        i += 1;
    }
}
