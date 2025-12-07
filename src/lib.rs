use anyhow::{anyhow, Error};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;

// 通用的DNS记录结构
#[derive(Clone, Debug)]
pub struct DnsRecord {
    pub id: String,
    pub name: String,
    pub value: String,
    pub record_type: String,
}

// DNS Provider trait - 所有DNS提供商必须实现这个trait
pub trait DnsProvider {
    fn get_record(&self) -> Result<Option<DnsRecord>, Error>;
    fn modify_record(&self, current_ip: &str, record: &DnsRecord) -> Result<(), Error>;
    fn add_record(&self, current_ip: &str) -> Result<(), Error>;

    fn update_dns_record(&self, current_ip: &str) -> Result<bool, Error> {
        match self.get_record() {
            Ok(Some(record)) => {
                if current_ip != &record.value {
                    info!("ip changed from {} to {}", record.value, current_ip);
                    self.modify_record(current_ip, &record)?;
                    Ok(true)
                } else {
                    info!("ip not changed");
                    Ok(false)
                }
            }
            Ok(None) => {
                info!("no such record, creating new one");
                self.add_record(current_ip)?;
                Ok(true)
            }
            Err(e) => {
                warn!("error get record: {e}");
                Err(e)
            }
        }
    }
}

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

// 向后兼容的 Record 类型（已废弃，使用 DnsRecord 代替）
#[derive(Serialize, Deserialize, Clone, Debug)]
#[deprecated(note = "Use DnsRecord instead")]
pub struct Record {
    pub id: String,
    pub name: String,
    pub value: String,
    pub updated_on: String,
    pub line_id: String,
}

// 向后兼容的 Res 类型（已废弃）
#[derive(Serialize, Deserialize)]
#[deprecated(note = "Use DnspodRes instead")]
pub struct Res {
    pub records: Vec<Record>,
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

// ========== Cloudflare Provider 实现 ==========

#[derive(Serialize, Deserialize, Debug)]
struct CloudflareListResponse {
    success: bool,
    errors: Vec<CloudflareError>,
    result: Vec<CloudflareRecord>,
}

#[derive(Serialize, Deserialize, Debug)]
struct CloudflareRecordResponse {
    success: bool,
    errors: Vec<CloudflareError>,
    result: CloudflareRecord,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct CloudflareRecord {
    id: String,
    name: String,
    #[serde(rename = "type")]
    record_type: String,
    content: String,
    ttl: u32,
    proxied: bool,
}

#[derive(Serialize, Deserialize, Debug)]
struct CloudflareError {
    code: i32,
    message: String,
}

#[derive(Serialize, Deserialize)]
struct CloudflareCreateRequest {
    #[serde(rename = "type")]
    record_type: String,
    name: String,
    content: String,
    ttl: u32,
    proxied: bool,
}

#[derive(Serialize, Deserialize)]
struct CloudflareUpdateRequest {
    #[serde(rename = "type")]
    record_type: String,
    name: String,
    content: String,
    ttl: u32,
    proxied: bool,
}

#[derive(Clone)]
pub struct CloudflareProvider {
    api_token: String,
    zone_id: String,
    record_name: String,
}

impl CloudflareProvider {
    pub fn new(api_token: String, zone_id: String, record_name: String) -> Self {
        CloudflareProvider {
            api_token,
            zone_id,
            record_name,
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

impl DnsProvider for CloudflareProvider {
    /// 获取DNS记录
    fn get_record(&self) -> Result<Option<DnsRecord>, Error> {
        let client = reqwest::blocking::Client::new();
        let url = format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records?name={}",
            self.zone_id, self.record_name
        );

        let res = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .header("Content-Type", "application/json")
            .send()?;

        let text = res.text()?;
        let result: serde_json::Result<CloudflareListResponse> = serde_json::from_str(&text);

        match result {
            Ok(response) => {
                if !response.success {
                    let errors: Vec<String> = response
                        .errors
                        .iter()
                        .map(|e| format!("{}: {}", e.code, e.message))
                        .collect();
                    return Err(anyhow!("Cloudflare API error: {}", errors.join(", ")));
                }

                if !response.result.is_empty() {
                    let record = &response.result[0];
                    info!("current cloudflare record is {:?}", record);
                    Ok(Some(DnsRecord {
                        id: record.id.clone(),
                        name: record.name.clone(),
                        value: record.content.clone(),
                        record_type: record.record_type.clone(),
                    }))
                } else {
                    Ok(None)
                }
            }
            Err(err) => {
                warn!("error parse cloudflare result: {text}");
                Err(anyhow!(err))
            }
        }
    }

    /// 修改DNS记录
    fn modify_record(&self, current_ip: &str, record: &DnsRecord) -> Result<(), Error> {
        let client = reqwest::blocking::Client::new();
        let url = format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}",
            self.zone_id, record.id
        );

        let update_request = CloudflareUpdateRequest {
            record_type: Self::get_record_type(current_ip).to_string(),
            name: self.record_name.clone(),
            content: current_ip.to_string(),
            ttl: 1, // 自动TTL
            proxied: false,
        };

        let res = client
            .patch(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .header("Content-Type", "application/json")
            .json(&update_request)
            .send()?;

        let text = res.text()?;
        let result: serde_json::Result<CloudflareRecordResponse> = serde_json::from_str(&text);

        match result {
            Ok(response) => {
                if response.success {
                    info!("cloudflare modify result: success");
                    Ok(())
                } else {
                    let errors: Vec<String> = response
                        .errors
                        .iter()
                        .map(|e| format!("{}: {}", e.code, e.message))
                        .collect();
                    Err(anyhow!("Cloudflare API error: {}", errors.join(", ")))
                }
            }
            Err(err) => {
                warn!("error parse cloudflare modify result: {text}");
                Err(anyhow!(err))
            }
        }
    }

    /// 添加DNS记录
    fn add_record(&self, current_ip: &str) -> Result<(), Error> {
        let client = reqwest::blocking::Client::new();
        let url = format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records",
            self.zone_id
        );

        let create_request = CloudflareCreateRequest {
            record_type: Self::get_record_type(current_ip).to_string(),
            name: self.record_name.clone(),
            content: current_ip.to_string(),
            ttl: 1, // 自动TTL
            proxied: false,
        };

        let res = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .header("Content-Type", "application/json")
            .json(&create_request)
            .send()?;

        let text = res.text()?;
        let result: serde_json::Result<CloudflareRecordResponse> = serde_json::from_str(&text);

        match result {
            Ok(response) => {
                if response.success {
                    info!("cloudflare add result: success");
                    Ok(())
                } else {
                    let errors: Vec<String> = response
                        .errors
                        .iter()
                        .map(|e| format!("{}: {}", e.code, e.message))
                        .collect();
                    Err(anyhow!("Cloudflare API error: {}", errors.join(", ")))
                }
            }
            Err(err) => {
                warn!("error parse cloudflare add result: {text}");
                Err(anyhow!(err))
            }
        }
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
    pub fn modify_record(&self, current_ip: &String, record: &DnsRecord) -> Result<(), Error> {
        self.provider.modify_record(current_ip, record)
    }

    /// 添加DNS记录（向后兼容）
    pub fn add_record(&self, current_ip: &str) -> Result<(), Error> {
        self.provider.add_record(current_ip)
    }

    /// 更新DNS记录（包括检查、添加或修改记录）（向后兼容）
    pub fn update_dns_record(&self, current_ip: &String) -> Result<bool, Error> {
        self.provider.update_dns_record(current_ip)
    }
}
