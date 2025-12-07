use anyhow::{anyhow, Error};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;

use crate::{DnsProvider, DnsRecord};

// ========== DNSPod 相关结构 ==========

#[derive(Serialize, Deserialize)]
struct DnspodRes {
    records: Vec<DnspodRecord>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct DnspodRecord {
    id: String,
    name: String,
    value: String,
    updated_on: String,
    line_id: String,
}

// ========== DNSPod Provider 实现 ==========

#[derive(Clone)]
pub struct DnspodProvider {
    token: String,
    domain: String,
    sub_domain: String,
}

impl DnspodProvider {
    pub fn new(token: String, domain: String, sub_domain: String) -> Self {
        DnspodProvider {
            token,
            domain,
            sub_domain,
        }
    }

    /// 判断IP地址类型，返回对应的记录类型
    fn get_record_type(ip: &str) -> &'static str {
        match ip.parse::<IpAddr>() {
            Ok(IpAddr::V4(_)) => "A",
            Ok(IpAddr::V6(_)) => "AAAA",
            Err(_) => "A", // 默认使用A记录
        }
    }
}

impl DnsProvider for DnspodProvider {
    /// 获取DNS记录
    fn get_record(&self) -> Result<Option<DnsRecord>, Error> {
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
        let result: serde_json::Result<DnspodRes> = serde_json::from_str(&text);
        match result {
            Ok(res) => {
                if !res.records.is_empty() {
                    let record = &res.records[0];
                    info!("current record is {:?}", record);
                    Ok(Some(DnsRecord {
                        id: record.id.clone(),
                        name: record.name.clone(),
                        value: record.value.clone(),
                        record_type: "A".to_string(), // DNSPod需要从记录中推断
                    }))
                } else {
                    Ok(None)
                }
            }
            Err(err) => {
                warn!("error parse result: {text}");
                Err(anyhow!(err))
            }
        }
    }

    /// 修改DNS记录
    fn modify_record(&self, current_ip: &str, record: &DnsRecord) -> Result<(), Error> {
        let client = reqwest::blocking::Client::new();
        let mut params: HashMap<&'static str, &str> = HashMap::new();

        // 从DNSPod获取记录时，我们需要line_id，这里我们从原始记录获取
        // 注意：这是个简化实现，实际应该保存完整的DNSPod记录
        let record_id = &record.id;

        params.insert("login_token", &self.token);
        params.insert("format", "json");
        params.insert("error_on_empty", "no");
        params.insert("lang", "en");
        params.insert("domain", &self.domain);
        params.insert("sub_domain", &record.name);
        params.insert("record_id", record_id);
        params.insert("record_line_id", "0"); // 默认线路
        params.insert("record_type", Self::get_record_type(current_ip));
        params.insert("value", current_ip);

        let res = client
            .post("https://dnsapi.cn/Record.Ddns")
            .form(&params)
            .send();

        if let Ok(res) = res {
            let text = res.text();
            if let Ok(text) = text {
                info!("modify result is： {text}");
                return Ok(());
            }
        }

        info!("error modify record");
        Err(anyhow!("Error modify record"))
    }

    /// 添加DNS记录
    fn add_record(&self, current_ip: &str) -> Result<(), Error> {
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
                info!("add result is： {text}");
                return Ok(());
            }
        }

        info!("error add record");
        Err(anyhow!("Error adding record"))
    }
}

// ========== 向后兼容的辅助函数 ==========

#[derive(Clone)]
pub struct DnspodClient {
    provider: DnspodProvider,
}

/// 初始化DNSPod配置并返回一个DnspodClient实例（向后兼容）
pub fn init(token: String, domain: String, sub_domain: String) -> DnspodClient {
    DnspodClient {
        provider: DnspodProvider::new(token, domain, sub_domain),
    }
}

impl DnspodClient {
    /// 获取DNS记录（向后兼容）
    pub fn get_record(&self) -> Result<Option<DnsRecord>, Error> {
        self.provider.get_record()
    }

    /// 修改DNS记录（向后兼容）
    pub fn modify_record(&self, current_ip: &str, record: &DnsRecord) -> Result<(), Error> {
        self.provider.modify_record(current_ip, record)
    }

    /// 添加DNS记录（向后兼容）
    pub fn add_record(&self, current_ip: &str) -> Result<(), Error> {
        self.provider.add_record(current_ip)
    }

    /// 更新DNS记录（包括检查、添加或修改记录）（向后兼容）
    pub fn update_dns_record(&self, current_ip: &str) -> Result<bool, Error> {
        self.provider.update_dns_record(current_ip)
    }
}
