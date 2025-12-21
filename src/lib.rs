use anyhow::Error;
use log::{info, warn};

// 子模块声明
pub mod cloudflare;
pub mod dnspod;

// 重新导出常用类型
pub use cloudflare::CloudflareProvider;

// 通用的DNS记录结构
#[derive(Clone, Debug)]
pub struct DnsRecord {
    pub id: String,
    pub name: String,
    pub value: String,
    pub record_type: String,
}
pub enum DnsUpdateResult {
    Changed { old_ip: String },
    Created,
    Unchanged,
}

// DNS Provider trait - 所有DNS提供商必须实现这个trait
pub trait DnsProvider {
    fn get_record(&self) -> Result<Option<DnsRecord>, Error>;
    fn modify_record(&self, current_ip: &str, record: &DnsRecord) -> Result<(), Error>;
    fn add_record(&self, current_ip: &str) -> Result<(), Error>;

    fn update_dns_record(&self, current_ip: &str) -> Result<DnsUpdateResult, Error> {
        match self.get_record() {
            Ok(Some(record)) => {
                if current_ip != record.value {
                    info!("ip changed from {} to {}", record.value, current_ip);
                    self.modify_record(current_ip, &record)?;
                    Ok(DnsUpdateResult::Changed {
                        old_ip: record.value,
                    })
                } else {
                    info!("ip not changed");
                    Ok(DnsUpdateResult::Unchanged)
                }
            }
            Ok(None) => {
                info!("no such record, creating new one");
                self.add_record(current_ip)?;
                Ok(DnsUpdateResult::Created)
            }
            Err(e) => {
                warn!("error get record: {e}");
                Err(e)
            }
        }
    }
}
