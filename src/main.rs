#![cfg_attr(windows_subsystem, windows_subsystem = "windows")]
use anyhow::{Error, anyhow};
use askama::Template;
use clap::Parser;
use dns_lib::CloudflareProvider;
use dns_lib::DnsProvider;
use dns_lib::DnsUpdateResult;
use dns_lib::dnspod::DnspodProvider;
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::fs;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{LazyLock, OnceLock};
use std::thread::sleep;
use std::time::Duration;
use telegram_bot_send::{DynError, TelegramBot, TelegramBotBuilder};
use tokio::runtime::Runtime;

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
enum Provider {
    Dnspod,
    #[default]
    Cloudflare,
}

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
    #[arg(long)]
    tg_bot_token: Option<String>,
    #[arg(long)]
    tg_chat_id: Option<String>,
    #[arg(long)]
    tg_http_proxy: Option<String>,
}

#[derive(Deserialize, Serialize, Debug)]
struct Config {
    /// 间隔时间（秒）
    #[serde(default = "default_sleep_secs")]
    sleep_secs: u64,

    /// 每隔几次强制从dnspod获取最新的记录
    #[serde(default = "default_force_interval")]
    force_get_record_interval: i8,

    /// 默认DNS Provider类型 ("dnspod" 或 "cloudflare")
    #[serde(default)]
    default_provider: Provider,

    /// 默认DNSPod Token
    #[serde(default)]
    default_dnspod_token: Option<String>,

    /// 默认Cloudflare API Token
    #[serde(default)]
    default_cloudflare_token: Option<String>,

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
    /// DNS Provider类型 (可选，未设置时使用default_provider)
    /// 支持: "dnspod" 或 "cloudflare"
    provider: Option<Provider>,

    /// DNSPod Token (可选，provider为dnspod时使用，未设置时使用default_dnspod_token)
    dnspod_token: Option<String>,

    /// Cloudflare API Token (可选，provider为cloudflare时使用，未设置时使用default_cloudflare_token)
    cloudflare_token: Option<String>,

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
    info!("Executing hook command for domain {domain}: {hook_command}");

    // 设置环境变量
    #[cfg(windows)]
    let mut cmd = {
        let mut cmd = Command::new("powershell");
        cmd.creation_flags(0x08000000)
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-Command")
            .arg(hook_command);
        cmd
    };

    #[cfg(not(windows))]
    let mut cmd = {
        let mut cmd = Command::new("bash");
        cmd.arg("-c").arg(hook_command);
        cmd
    };

    cmd.env("DOMAIN", domain)
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
                info!("Hook command executed successfully for domain {domain}");
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let error_msg = format!(
                    "Hook command failed with exit code {}: {}",
                    output.status.code().unwrap_or(-1),
                    stderr.trim()
                );
                error!("{error_msg}");
                Err(anyhow!(error_msg))
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to execute hook command: {e}");
            error!("{error_msg}");
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
        let provider = domain_config.provider.unwrap_or(config.default_provider);

        // 检查DNSPod配置
        if provider == Provider::Dnspod
            && domain_config.dnspod_token.is_none()
            && config.default_dnspod_token.is_none()
        {
            return Err(anyhow!(
                "Domain {} uses DNSPod but has no dnspod_token and no default_dnspod_token is configured",
                i + 1
            ));
        }

        // 检查Cloudflare配置
        if provider == Provider::Cloudflare
            && domain_config.cloudflare_token.is_none()
            && config.default_cloudflare_token.is_none()
        {
            return Err(anyhow!(
                "Domain {} uses Cloudflare but has no cloudflare_token and no default_cloudflare_token is configured",
                i + 1
            ));
        }

        // 验证域名格式（仅DNSPod需要分割域名）
        if provider == Provider::Dnspod
            && let Err(e) = parse_domain(&domain_config.domain)
        {
            return Err(anyhow!("Domain {} has invalid format: {}", i + 1, e));
        }
    }

    Ok(config)
}

struct DomainUpdateResult {
    domain: String,
    new_ip: String,
    old_ip: String,
}

/// 处理单个域名的DDNS更新
fn update_record_if_need(
    domain_config: &DomainConfig,
    config: &Config,
    current_ip: &str,
    old_ip: &str,
    get_current_record_from_authority: bool,
) -> Result<DnsUpdateResult, Error> {
    let domain = domain_config.domain.clone();
    if current_ip != old_ip || get_current_record_from_authority {
        // 获取provider类型
        let provider = domain_config.provider.unwrap_or(config.default_provider);
        match provider {
            Provider::Dnspod => {
                // DNSPod provider
                let (subdomain, main_domain) = parse_domain(&domain_config.domain)?;
                let token = domain_config
                    .dnspod_token
                    .as_ref()
                    .or(config.default_dnspod_token.as_ref())
                    .ok_or_else(|| anyhow!("No DNSPod token available for domain {}", domain))?;

                let provider: DnspodProvider =
                    DnspodProvider::new(token.clone(), main_domain, subdomain);
                Ok(provider.update_dns_record(current_ip)?)
            }
            Provider::Cloudflare => {
                // Cloudflare provider
                let token = domain_config
                    .cloudflare_token
                    .as_ref()
                    .or(config.default_cloudflare_token.as_ref())
                    .ok_or_else(|| {
                        anyhow!("No Cloudflare token available for domain {}", domain)
                    })?;

                let provider = CloudflareProvider::new(token.clone(), domain_config.domain.clone());
                Ok(provider.update_dns_record(current_ip)?)
            }
        }
    } else {
        info!("IP for {domain} unchanged: {current_ip}");
        Ok(DnsUpdateResult::Unchanged)
    }
}

pub(crate) static TG_BOT: OnceLock<Result<TelegramBot, DynError>> = OnceLock::new();

// 全局 Tokio runtime，用于异步操作
static TOKIO_RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime")
});

fn main() -> Result<(), Error> {
    let args = Args::parse();

    // 初始化日志
    log_x::init_log("log", "dnspod.log", "info")?;

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
        let get_current_record_from_authority = iteration % config.force_get_record_interval == 0;

        // 处理每个域名配置
        for domain_config in &config.domains {
            let domain = &domain_config.domain;

            // 获取IP查询URL，优先使用域名配置中的ip_url
            let ip_url = domain_config
                .ip_url
                .as_ref()
                .unwrap_or(&config.default_ip_url);

            // 获取当前IP
            match current_ip(ip_url) {
                Ok(current_ip) => {
                    info!("Current IP for {domain} from {ip_url}: {current_ip}");
                    let old_ip = latest_ips.get(domain).cloned().unwrap_or_default();

                    match update_record_if_need(
                        domain_config,
                        &config,
                        &current_ip,
                        &old_ip,
                        get_current_record_from_authority,
                    ) {
                        Ok(result) => match result {
                            DnsUpdateResult::Changed { old_ip } => {
                                latest_ips.insert(domain.clone(), current_ip.clone());
                                let result = DomainUpdateResult {
                                    domain: domain.clone(),
                                    new_ip: current_ip.clone(),
                                    old_ip: old_ip.clone(),
                                };

                                send_tg(&args, &result);
                                exec_hook_if_present(&config, domain_config, domain, result);
                            }
                            DnsUpdateResult::Created => {
                                latest_ips.insert(domain.clone(), current_ip.clone());
                                let result = DomainUpdateResult {
                                    domain: domain.clone(),
                                    new_ip: current_ip.clone(),
                                    old_ip: "".to_string(),
                                };

                                send_tg(&args, &result);
                                exec_hook_if_present(&config, domain_config, domain, result);
                            }
                            DnsUpdateResult::Unchanged => {}
                        },
                        Err(e) => {
                            error!("Error updating domain {}: {}", domain, e);
                        }
                    }
                }
                Err(e) => {
                    error!("Error fetching current IP for {domain} from {ip_url}: {e}");
                }
            }
        }

        info!("Sleeping for {} seconds...", config.sleep_secs);
        sleep(Duration::from_secs(config.sleep_secs));
        iteration += 1;
    }
}

fn exec_hook_if_present(
    config: &Config,
    domain_config: &DomainConfig,
    domain_key: &String,
    result: DomainUpdateResult,
) {
    // 如果IP发生变化，执行hook指令
    if let Some(hook_command) = domain_config
        .hook_command
        .as_ref()
        .or(config.default_hook_command.as_ref())
        && let Err(e) = execute_hook_command(
            hook_command,
            &result.domain,
            result.new_ip.as_str(),
            result.old_ip.as_str(),
        )
    {
        error!("Hook command execution failed for {domain_key}: {e}");
        // 不返回错误，让程序继续运行
    }
}

fn send_tg(args: &Args, result: &DomainUpdateResult) {
    if let Some(tg_bot_token) = &args.tg_bot_token
        && let Some(tg_chat_id) = &args.tg_chat_id
    {
        let message = TelegramMessage {
            domain: result.domain.clone(),
            new_ip: result.new_ip.clone(),
            old_ip: result.old_ip.clone(),
        };
        message
            .render()
            .map_err(|e| {
                error!("Failed to render Telegram message template: {}", e);
            })
            .map(|msg| {
                let bot = TG_BOT.get_or_init(|| {
                    let mut builder = TelegramBotBuilder::new(tg_bot_token.clone());
                    if let Some(proxy) = &args.tg_http_proxy {
                        builder = builder.http_proxy(proxy.clone());
                    }
                    builder.build()
                });
                match bot {
                    Ok(bot) => {
                        TOKIO_RUNTIME.block_on(async {
                            if let Err(e) =
                                bot.send_message(tg_chat_id.clone(), format_md2(&msg)).await
                            {
                                error!(
                                    "Failed to send Telegram message for {}: {:?}",
                                    result.domain, e
                                );
                            } else {
                                info!("Sent Telegram message for {}", result.domain);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Failed to initialize Telegram bot: {}", e);
                    }
                }
            })
            .ok();
    }
}

/// 格式化所有特殊字符为 MarkdownV2 格式
fn format_md2(text: &str) -> String {
    text.replace("_", r"\_")
        .replace("[", r"\[")
        .replace("]", r"\]")
        .replace("(", r"\(")
        .replace(")", r"\)")
        .replace("-", r"\-")
        .replace(".", r"\.")
        .replace("!", r"\!")
}

#[derive(Template)]
#[template(path = "tg_bot_message_template")]
#[allow(dead_code)]
struct TelegramMessage {
    domain: String,
    new_ip: String,
    old_ip: String,
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
