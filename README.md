# DDNS - DNSPod Dynamic DNS Client

A Rust-based DNSPod DDNS client that supports multiple domains and TOML configuration.

## Features

- 支持多个域名配置
- 基于 TOML 配置文件
- 命令行参数支持
- 自动 IP 变化检测
- 强制更新机制
- 详细的日志记录
- 可选的无控制台窗口模式（Windows）

## 安装和使用

### 1. 编译程序

```bash
# 标准编译（显示控制台窗口）
cargo build --release

# 无控制台窗口编译（适用于 Windows 后台运行）
cargo build --release --features no-console
```

### 2. 配置文件

创建 `config.toml` 配置文件（或使用 `-c` 参数指定其他路径）：

```toml
# DDNS配置文件
# 间隔时间（秒）
sleep_secs = 120
# 每隔几次强制从dnspod获取最新的记录
force_get_record_interval = 5

# 默认配置（可选）
# 当域名配置中没有指定token或ip_url时，会使用这些默认值
default_token = "your_default_token_id,your_default_token_secret"
default_ip_url = "https://api.ipify.org"

# 域名配置列表
# 支持多级子域名格式：
# - "sub.example.com" 表示一级子域名记录
# - "api.v2.example.com" 表示二级子域名记录
# - "deep.nested.sub.example.com" 表示多级子域名记录
# - "@.example.com" 或 "example.com" 表示根域名记录

# 示例1：使用默认token和ip_url的一级子域名
[[domains]]
domain = "blog.example.com"

# 示例2：使用自定义token的根域名
[[domains]]
domain = "@.mysite.org"  # 等同于 "mysite.org"
token = "custom_token_id,custom_token_secret"

# 示例3：二级子域名 - API版本控制
[[domains]]
domain = "api.v2.example.com"
token = "another_token_id,another_token_secret"
ip_url = "https://www.arloor.com/ip"

# 示例4：三级子域名 - 微服务架构
[[domains]]
domain = "auth.service.k8s.example.com"
```

### 3. 运行程序

```bash
# 使用默认配置文件 config.toml
./target/release/ddns

# 指定配置文件路径
./target/release/ddns -c /path/to/your/config.toml

# 启用详细日志
./target/release/ddns -v

# 查看帮助
./target/release/ddns --help
```

## 命令行参数

- `-c, --config <FILE>`: 指定配置文件路径（默认：config.toml）
- `-v, --verbose`: 启用详细日志
- `-h, --help`: 显示帮助信息

## 配置说明

### 全局配置

- `sleep_secs`: 检查间隔时间（秒），默认 120 秒
- `force_get_record_interval`: 强制更新间隔次数，默认每 5 次检查强制更新一次
- `default_token`: 默认 DNSPod Token（可选），当域名配置中未指定 token 时使用
- `default_ip_url`: 默认 IP 查询 URL（可选），当域名配置中未指定 ip_url 时使用，默认为"http://whatismyip.akamai.com"

### 域名配置

每个 `[[domains]]` 块代表一个域名配置：

- `domain`: 完整域名，支持多级子域名
  - 一级子域名：`"sub.example.com"`（如 blog.example.com）
  - 二级子域名：`"api.v2.example.com"`（如 api 版本控制）
  - 多级子域名：`"auth.service.k8s.example.com"`（如 微服务架构）
  - 根域名格式：`"@.example.com"` 或 `"example.com"`
- `token`: DNSPod API Token（可选），格式为 "token_id,token_secret"，未指定时使用 `default_token`
- `ip_url`: 获取当前 IP 的 URL（可选），未指定时使用 `default_ip_url`

## 获取 DNSPod Token

1. 登录 [DNSPod 控制台](https://console.dnspod.cn/)
2. 进入 "用户中心" -> "安全设置" -> "API Token"
3. 创建新的 Token，获得 token_id 和 token_secret
4. 在配置文件中使用格式: "token_id,token_secret"

## 日志

程序会在 `log` 目录下生成日志文件 `dnspod.log`，记录所有操作和错误信息。

## 从环境变量迁移

如果您之前使用环境变量方式，可以按如下方式迁移到配置文件：

环境变量 -> 配置文件字段：

- `dnspod_token` -> `default_token` 或 `domains[].token`
- `dnspod_domain` + `dnspod_subdomain` -> `domains[].domain`
  - 原来的 `dnspod_domain="example.com"` + `dnspod_subdomain="www"` -> `domain="www.example.com"`
  - 原来的 `dnspod_domain="example.com"` + `dnspod_subdomain="@"` -> `domain="@.example.com"`
- `dnspod_ip_url` -> `default_ip_url` 或 `domains[].ip_url`

### 迁移示例

原环境变量配置：

```bash
export dnspod_token="12345,abcdef"
export dnspod_domain="example.com"
export dnspod_subdomain="www"
export dnspod_ip_url="https://api.ipify.org"
```

新配置文件：

```toml
default_token = "12345,abcdef"
default_ip_url = "https://api.ipify.org"

[[domains]]
domain = "www.example.com"
```

## 示例

假设您要为以下域名配置 DDNS：

- `blog.example.com`（一级子域名，使用默认 token）
- `api.v2.mysite.org`（二级子域名，使用自定义 token）
- `auth.service.k8s.example.com`（多级子域名，微服务架构）
- `example.com` 根域名（使用默认 token 和自定义 IP 查询）

配置文件示例：

```toml
sleep_secs = 300  # 5分钟检查一次
force_get_record_interval = 3

# 默认配置
default_token = "12345,abcdef123456"
default_ip_url = "https://api.ipify.org"

[[domains]]
domain = "blog.example.com"
# 使用默认token和default_ip_url

[[domains]]
domain = "api.v2.mysite.org"
token = "67890,ghijkl789012"
# 使用自定义token但默认ip_url

[[domains]]
domain = "auth.service.k8s.example.com"
ip_url = "https://ip.seeip.org"
# 使用默认token但自定义ip_url

[[domains]]
domain = "@.example.com"  # 或者直接写 "example.com"
ip_url = "https://ip.seeip.org"
# 根域名，使用默认token但自定义ip_url
```

## Systemd 服务配置

### 1. 安装二进制文件

```bash
# 编译release版本
cargo build --release

# 复制到系统路径
sudo cp target/release/ddns /usr/local/bin/ddns
sudo chmod +x /usr/local/bin/ddns
```

### 2. 创建配置目录和文件

```bash
# 创建配置目录
sudo mkdir -p /opt/ddns

# 复制配置文件
sudo cp config.toml /opt/ddns/
# 或者直接创建配置文件
sudo tee /opt/ddns/config.toml <<EOF
sleep_secs = 120
force_get_record_interval = 5

[[domains]]
token = "your_token_id,your_token_secret"
domain = "your-domain.com"
subdomain = "your-subdomain"
ip_url = "https://api.ipify.org"
EOF
```

### 3. 创建 systemd 服务

```bash
sudo tee /lib/systemd/system/ddns.service <<EOF
[Unit]
Description=DNSPod DDNS Client
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
WorkingDirectory=/opt/ddns
ExecStart=/usr/local/bin/ddns -c /opt/ddns/config.toml
LimitNOFILE=100000
Restart=always
RestartSec=30

[Install]
WantedBy=multi-user.target
EOF
```

### 4. 启用和启动服务

```bash
sudo systemctl daemon-reload
sudo systemctl enable ddns
sudo systemctl start ddns
```

### 5. 查看服务状态和日志

```bash
# 查看服务状态
sudo systemctl status ddns

# 查看系统日志
sudo journalctl -u ddns -f

# 查看应用日志
tail -f /opt/ddns/log/dnspod.log
```

## 可执行文件

```bash
curl -sSLf https://us.arloor.dev/https://github.com/arloor/ddns/releases/download/v1.0.0/ddns -o /tmp/ddns
install /tmp/ddns /usr/local/bin/ddns
```

## 日志

```shell
tailf -fn 100 /opt/ddns/log/ddns.log
```

## Features 配置

### no-console Feature

在 Windows 系统上，你可以选择编译无控制台窗口的版本，适合作为后台服务运行：

```bash
# 标准编译（会显示控制台窗口）
cargo build --release

# 无控制台窗口编译
cargo build --release --features no-console
```

启用 `no-console` feature 后：

- 程序运行时不会显示控制台窗口
- 适合作为 Windows 服务或后台任务运行
- 所有日志仍会正常输出到日志文件
- 只在 Windows 平台生效，其他平台无影响

### 在 Cargo.toml 中配置

你也可以在 `Cargo.toml` 中将 `no-console` 设为默认 feature：

```toml
[features]
default = ["no-console"]
no-console = []
```
