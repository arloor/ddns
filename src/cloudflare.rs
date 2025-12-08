use anyhow::{anyhow, Error};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::sync::Mutex;
use std::{collections::HashMap, sync::LazyLock};

use crate::{DnsProvider, DnsRecord};

// 全局的 Cloudflare Zone 缓存: api_token -> domain -> zone_id
static CLOUDFLARE_ZONE_CACHE: LazyLock<Mutex<HashMap<String, HashMap<String, String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// ========== Cloudflare 相关结构 ==========

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

#[derive(Serialize, Deserialize, Debug)]
struct CloudflareZoneListResponse {
    success: bool,
    errors: Vec<CloudflareError>,
    result: Vec<CloudflareZone>,
}

#[derive(Serialize, Deserialize, Debug)]
struct CloudflareZone {
    id: String,
    name: String,
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

// ========== Cloudflare Provider 实现 ==========

pub struct CloudflareProvider {
    api_token: String,
    record_name: String,
}

impl CloudflareProvider {
    pub fn new(api_token: String, record_name: String) -> Self {
        CloudflareProvider {
            api_token,
            record_name,
        }
    }

    /// 从完整的记录名称中提取根域名
    /// 例如: "sub.example.com" -> "example.com"
    ///      "example.com" -> "example.com"
    fn extract_zone_name(record_name: &str) -> String {
        let parts: Vec<&str> = record_name.split('.').collect();
        if parts.len() >= 2 {
            format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1])
        } else {
            record_name.to_string()
        }
    }

    /// 获取Zone ID，优先从缓存读取，缓存未命中时调用API查询
    fn get_zone_id(&self) -> Result<String, Error> {
        let zone_name = Self::extract_zone_name(&self.record_name);

        // 先尝试从缓存读取
        {
            let cache = CLOUDFLARE_ZONE_CACHE.lock().unwrap();
            if let Some(token_cache) = cache.get(&self.api_token) {
                if let Some(zone_id) = token_cache.get(&zone_name) {
                    debug!("Using cached zone_id for {}: {}", zone_name, zone_id);
                    return Ok(zone_id.clone());
                }
            }
        }

        // 缓存未命中，调用API查询
        debug!("Querying zone_id for domain: {}", zone_name);
        let client = reqwest::blocking::Client::new();

        let url = format!(
            "https://api.cloudflare.com/client/v4/zones?name={}",
            zone_name
        );

        let response = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .header("Content-Type", "application/json")
            .send()
            .map_err(|e| anyhow!("Failed to query zone list: {}", e))?;

        let zone_list: CloudflareZoneListResponse = response
            .json()
            .map_err(|e| anyhow!("Failed to parse zone list response: {}", e))?;

        if !zone_list.success {
            let error_msgs: Vec<String> = zone_list
                .errors
                .iter()
                .map(|e| format!("{}: {}", e.code, e.message))
                .collect();
            return Err(anyhow!("Cloudflare API error: {}", error_msgs.join(", ")));
        }

        if zone_list.result.is_empty() {
            return Err(anyhow!("No zone found for domain: {}", zone_name));
        }

        let zone_id = zone_list.result[0].id.clone();
        debug!("Found zone_id for {}: {}", zone_name, zone_id);

        // 存入缓存
        {
            let mut cache = CLOUDFLARE_ZONE_CACHE.lock().unwrap();
            cache
                .entry(self.api_token.clone())
                .or_default()
                .insert(zone_name, zone_id.clone());
        }

        Ok(zone_id)
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
        let zone_id = self.get_zone_id()?;
        let client = reqwest::blocking::Client::new();
        let url = format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records?name={}&type=CNAME,A,AAAA",
            zone_id, self.record_name
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
        let zone_id = self.get_zone_id()?;
        let client = reqwest::blocking::Client::new();
        let url = format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}",
            zone_id, record.id
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
                    debug!("cloudflare modify result: success");
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
        let zone_id = self.get_zone_id()?;
        let client = reqwest::blocking::Client::new();
        let url = format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records",
            zone_id
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
                    debug!("cloudflare add result: success");
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
