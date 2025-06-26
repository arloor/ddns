use anyhow::{anyhow, Error};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;

#[derive(Serialize, Deserialize)]
struct Res {
    records: Vec<Record>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Record {
    pub id: String,
    pub name: String,
    pub value: String,
    pub updated_on: String,
    pub line_id: String,
}

#[derive(Clone)]
pub struct DnspodClient {
    token: String,
    domain: String,
    sub_domain: String,
}

/// 初始化DNSPod配置并返回一个DnspodClient实例
pub fn init(token: String, domain: String, sub_domain: String) -> DnspodClient {
    info!(
        "初始化DNSPod配置: modify [{}.{}] with token [{}]",
        sub_domain, domain, token
    );

    DnspodClient {
        token,
        domain,
        sub_domain,
    }
}

impl DnspodClient {
    /// 判断IP地址类型，返回对应的记录类型
    fn get_record_type(ip: &str) -> &'static str {
        match ip.parse::<IpAddr>() {
            Ok(IpAddr::V4(_)) => "A",
            Ok(IpAddr::V6(_)) => "AAAA",
            Err(_) => "A", // 默认使用A记录
        }
    }

    /// 获取DNS记录
    pub fn get_record(&self) -> Result<Option<Record>, Error> {
        let mut params: HashMap<&'static str, &str> = HashMap::new();
        params.insert("login_token", &self.token);
        params.insert("format", "json");
        params.insert("error_on_empty", "no");
        params.insert("lang", "en");
        params.insert("domain", &self.domain);
        params.insert("sub_domain", &self.sub_domain);

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

    /// 修改DNS记录
    pub fn modify_record(&self, current_ip: &String, record: &Record) -> Result<(), Error> {
        if &record.value != current_ip {
            let client = reqwest::blocking::Client::new();
            let mut params: HashMap<&'static str, &str> = HashMap::new();
            params.insert("login_token", &self.token);
            params.insert("format", "json");
            params.insert("error_on_empty", "no");
            params.insert("lang", "en");
            params.insert("domain", &self.domain);
            params.insert("sub_domain", &record.name);
            params.insert("record_id", &record.id);
            params.insert("record_line_id", &record.line_id);
            params.insert("record_type", Self::get_record_type(current_ip));
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
                return Err(anyhow!("Error modify record"));
            }
        }
        Ok(())
    }

    /// 添加DNS记录
    pub fn add_record(&self, current_ip: &str) -> Result<(), Error> {
        let client = reqwest::blocking::Client::new();
        let mut params: HashMap<&'static str, &str> = HashMap::new();
        params.insert("login_token", &self.token);
        params.insert("format", "json");
        params.insert("error_on_empty", "no");
        params.insert("lang", "en");
        params.insert("domain", &self.domain);
        params.insert("sub_domain", &self.sub_domain);
        params.insert("record_type", Self::get_record_type(current_ip));
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
                return Ok(());
            }
        }

        info!("error add record");
        Err(anyhow!("Error adding record"))
    }

    /// 更新DNS记录（包括检查、添加或修改记录）
    pub fn update_dns_record(&self, current_ip: &String) -> Result<(), Error> {
        match self.get_record() {
            Ok(Some(record)) => {
                if current_ip != &record.value {
                    info!("ip changed from {} to {}", record.value, current_ip);
                    self.modify_record(current_ip, &record)?;
                } else {
                    info!("ip not changed");
                }
            }
            Ok(None) => {
                info!("no such record: {}.{}", self.sub_domain, self.domain);
                self.add_record(current_ip)?;
            }
            Err(e) => {
                warn!("error get record: {}", e);
                return Err(e);
            }
        }

        Ok(())
    }
}
