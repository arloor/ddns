use anyhow::Error;
use log::{info, warn};

// 子模块声明
pub mod cloudflare;
pub mod dnspod;

// 重新导出常用类型
pub use cloudflare::CloudflareProvider;
pub use dnspod::{DnspodClient, DnspodProvider};

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
                if current_ip != record.value {
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

// ========== 向后兼容的辅助函数 ==========

/// 初始化DNSPod配置并返回一个DnspodClient实例（向后兼容）
pub fn init(token: String, domain: String, sub_domain: String) -> DnspodClient {
    dnspod::init(token, domain, sub_domain)
}
