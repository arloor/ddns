#![cfg_attr(windows_subsystem, windows_subsystem = "windows")]
use anyhow::{anyhow, Error};
use clap::Parser;
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::LazyLock;
use std::thread::sleep;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "ddns")]
#[command(about = "A DNSPod DDNS client that supports multiple domains")]
struct Args {
    /// Path to the configuration file
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Deserialize, Serialize, Debug)]
struct Config {
    /// 间隔时间（秒）
    #[serde(default = "default_sleep_secs")]
    sleep_secs: u64,

    /// 每隔几次强制从dnspod获取最新的记录
    #[serde(default = "default_force_interval")]
    force_get_record_interval: i8,

    /// 默认DNSPod Token
    #[serde(default)]
    default_token: Option<String>,

    /// 默认查询IP的URL
    #[serde(default = "default_ip_url")]
    default_ip_url: String,

    /// 默认IP变化时执行的hook指令
    #[serde(default)]
    default_hook_command: Option<String>,

    /// 域名配置列表
    domains: Vec<DomainConfig>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct DomainConfig {
    /// DNSPod Token (可选，未设置时使用default_token)
    token: Option<String>,

    /// 完整域名 (如: "sub.example.com" 或 "@.example.com" 表示根域名)
    domain: String,

    /// 查询IP的URL (可选，未设置时使用default_ip_url)
    ip_url: Option<String>,

    /// IP变化时执行的hook指令 (可选，未设置时使用default_hook_command)
    hook_command: Option<String>,
}

fn default_sleep_secs() -> u64 {
    120
}

fn default_force_interval() -> i8 {
    5
}

fn default_ip_url() -> String {
    "http://whatismyip.akamai.com".to_string()
}

// 全局静态HTTP客户端，禁用代理
static HTTP_CLIENT: LazyLock<reqwest::blocking::Client> = LazyLock::new(|| {
    reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .expect("Failed to create HTTP client")
});

/// 解析完整域名，返回(子域名, 主域名)
/// 支持多级子域名：
/// 例如: "sub.example.com" -> ("sub", "example.com")
///      "api.v2.example.com" -> ("api.v2", "example.com")
///      "deep.nested.sub.example.com" -> ("deep.nested.sub", "example.com")
///      "@.example.com" -> ("@", "example.com")
///      "example.com" -> ("@", "example.com")
fn parse_domain(full_domain: &str) -> Result<(String, String), Error> {
    let parts: Vec<&str> = full_domain.split('.').collect();

    if parts.len() < 2 {
        return Err(anyhow!("Invalid domain format: {}", full_domain));
    }

    if let Some(main_domain) = full_domain.strip_prefix("@.") {
        // @.example.com -> ("@", "example.com")
        Ok(("@".to_string(), main_domain.to_string()))
    } else if parts.len() == 2 {
        // example.com -> ("@", "example.com")
        Ok(("@".to_string(), full_domain.to_string()))
    } else {
        // 对于多级域名，假设最后两个部分是主域名，其余为子域名
        // sub.example.com -> ("sub", "example.com")
        // api.v2.example.com -> ("api.v2", "example.com")
        // deep.nested.sub.example.com -> ("deep.nested.sub", "example.com")
        let main_domain_parts = &parts[parts.len() - 2..];
        let subdomain_parts = &parts[..parts.len() - 2];

        let main_domain = main_domain_parts.join(".");
        let subdomain = subdomain_parts.join(".");

        Ok((subdomain, main_domain))
    }
}

/// 执行hook指令
fn execute_hook_command(
    hook_command: &str,
    domain: &str,
    new_ip: &str,
    old_ip: &str,
) -> Result<(), Error> {
    info!(
        "Executing hook command for domain {}: {}",
        domain, hook_command
    );

    // 设置环境变量
    let mut cmd = Command::new("bash");
    cmd.arg("-c")
        .arg(hook_command)
        .env("DOMAIN", domain)
        .env("NEW_IP", new_ip)
        .env("OLD_IP", old_ip);

    match cmd.output() {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stdout.is_empty() {
                    info!("Hook command stdout: {}", stdout.trim());
                }
                if !stderr.is_empty() {
                    info!("Hook command stderr: {}", stderr.trim());
                }
                info!("Hook command executed successfully for domain {}", domain);
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let error_msg = format!(
                    "Hook command failed with exit code {}: {}",
                    output.status.code().unwrap_or(-1),
                    stderr.trim()
                );
                error!("{}", error_msg);
                Err(anyhow!(error_msg))
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to execute hook command: {}", e);
            error!("{}", error_msg);
            Err(anyhow!(error_msg))
        }
    }
}

/// 获取当前IP地址
fn current_ip(ip_url: &str) -> Result<String, Error> {
    let result = HTTP_CLIENT.get(ip_url).send();
    match result {
        Ok(ip) => match ip.text() {
            Ok(text) => Ok(text.trim().to_string()),
            Err(e) => Err(anyhow!(e)),
        },
        Err(e) => Err(anyhow!(e)),
    }
}

/// 读取配置文件
fn load_config(config_path: &PathBuf) -> Result<Config, Error> {
    let config_content = fs::read_to_string(config_path)
        .map_err(|e| anyhow!("Failed to read config file {:?}: {}", config_path, e))?;

    let config: Config = toml::from_str(&config_content)
        .map_err(|e| anyhow!("Failed to parse config file: {}", e))?;

    if config.domains.is_empty() {
        return Err(anyhow!("No domains configured"));
    }

    // 验证每个域名配置
    for (i, domain_config) in config.domains.iter().enumerate() {
        // 检查是否有token（要么在域名配置中，要么在默认配置中）
        if domain_config.token.is_none() && config.default_token.is_none() {
            return Err(anyhow!(
                "Domain {} has no token and no default_token is configured",
                i + 1
            ));
        }

        // 验证域名格式
        if let Err(e) = parse_domain(&domain_config.domain) {
            return Err(anyhow!("Domain {} has invalid format: {}", i + 1, e));
        }
    }

    Ok(config)
}

/// 处理单个域名的DDNS更新
fn handle_domain(
    domain_config: &DomainConfig,
    config: &Config,
    current_ip: &str,
    latest_ips: &mut std::collections::HashMap<String, String>,
    force_update: bool,
) -> Result<(), Error> {
    // 解析域名
    let (subdomain, main_domain) = parse_domain(&domain_config.domain)?;
    let domain_key = domain_config.domain.clone();

    // 获取token，优先使用域名配置中的token
    let token = domain_config
        .token
        .as_ref()
        .or(config.default_token.as_ref())
        .ok_or_else(|| anyhow!("No token available for domain {}", domain_key))?;

    let latest_ip = latest_ips.get(&domain_key).cloned().unwrap_or_default();
    let ip_changed = current_ip != latest_ip;

    if ip_changed || force_update {
        let client = dnspod::init(token.clone(), main_domain, subdomain);

        match client.update_dns_record(&current_ip.to_string()) {
            Ok(_) => {
                info!(
                    "Successfully updated DNS record for {}: {}",
                    domain_key, current_ip
                );

                // 如果IP发生变化，执行hook指令
                if ip_changed {
                    // 获取hook指令，优先使用域名配置中的hook_command
                    if let Some(hook_command) = domain_config
                        .hook_command
                        .as_ref()
                        .or(config.default_hook_command.as_ref())
                    {
                        if let Err(e) =
                            execute_hook_command(hook_command, &domain_key, current_ip, &latest_ip)
                        {
                            error!("Hook command execution failed for {}: {}", domain_key, e);
                            // 不返回错误，让程序继续运行
                        }
                    }
                }

                latest_ips.insert(domain_key, current_ip.to_string());
            }
            Err(e) => {
                error!("Failed to update DNS record for {}: {}", domain_key, e);
                return Err(e);
            }
        }
    } else {
        info!("IP for {} unchanged: {}", domain_key, current_ip);
    }

    Ok(())
}

fn main() -> Result<(), Error> {
    let args = Args::parse();

    // 初始化日志
    log_x::init_log("log", "dnspod.log")?;

    if args.verbose {
        info!("Verbose logging enabled");
    }

    // 加载配置文件
    let config = load_config(&args.config)?;
    info!("Loaded configuration with {} domains", config.domains.len());

    // 为每个域名存储最新的IP
    let mut latest_ips: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    let mut iteration = 0;

    loop {
        let force_update = iteration % config.force_get_record_interval == 0;

        if args.verbose {
            info!(
                "Starting iteration {}, force_update: {}",
                iteration, force_update
            );
        }

        // 处理每个域名配置
        for domain_config in &config.domains {
            let domain_key = &domain_config.domain;

            // 获取IP查询URL，优先使用域名配置中的ip_url
            let ip_url = domain_config
                .ip_url
                .as_ref()
                .unwrap_or(&config.default_ip_url);

            // 获取当前IP
            match current_ip(ip_url) {
                Ok(ip) => {
                    info!("Current IP for {} from {}: {}", domain_key, ip_url, ip);

                    if let Err(e) =
                        handle_domain(domain_config, &config, &ip, &mut latest_ips, force_update)
                    {
                        error!("Error handling domain {}: {}", domain_key, e);
                    }
                }
                Err(e) => {
                    error!(
                        "Error fetching current IP for {} from {}: {}",
                        domain_key, ip_url, e
                    );
                }
            }
        }

        info!("Sleeping for {} seconds...", config.sleep_secs);
        sleep(Duration::from_secs(config.sleep_secs));
        iteration += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_domain() {
        // 测试根域名
        assert_eq!(
            parse_domain("example.com").unwrap(),
            ("@".to_string(), "example.com".to_string())
        );
        assert_eq!(
            parse_domain("@.example.com").unwrap(),
            ("@".to_string(), "example.com".to_string())
        );

        // 测试单级子域名
        assert_eq!(
            parse_domain("www.example.com").unwrap(),
            ("www".to_string(), "example.com".to_string())
        );
        assert_eq!(
            parse_domain("blog.example.com").unwrap(),
            ("blog".to_string(), "example.com".to_string())
        );

        // 测试多级子域名
        assert_eq!(
            parse_domain("api.v2.example.com").unwrap(),
            ("api.v2".to_string(), "example.com".to_string())
        );
        assert_eq!(
            parse_domain("deep.nested.sub.example.com").unwrap(),
            ("deep.nested.sub".to_string(), "example.com".to_string())
        );
        assert_eq!(
            parse_domain("a.b.c.d.example.com").unwrap(),
            ("a.b.c.d".to_string(), "example.com".to_string())
        );

        // 测试错误情况
        assert!(parse_domain("invalid").is_err());
        assert!(parse_domain("").is_err());

        // 测试特殊情况
        assert_eq!(
            parse_domain("test.co.uk").unwrap(),
            ("test".to_string(), "co.uk".to_string())
        );
    }
}
