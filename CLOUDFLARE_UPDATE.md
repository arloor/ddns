# Cloudflare DNS API 支持更新

## 更新内容

本次更新为 DDNS 项目添加了 Cloudflare DNS API 支持，现在程序可以同时管理 DNSPod 和 Cloudflare 的 DNS 记录。

## 主要变更

### 1. 新增 DNS Provider 抽象层

- 创建了 `DnsProvider` trait 作为通用接口
- 定义了通用的 `DnsRecord` 结构来表示 DNS 记录
- 所有 DNS 提供商必须实现此 trait

### 2. 重构现有 DNSPod 实现

- 将原有的 `DnspodClient` 重构为实现 `DnsProvider` trait 的 `DnspodProvider`
- 保留 `DnspodClient` 作为向后兼容的包装器
- 旧的 `Record` 和 `Res` 类型标记为 deprecated，但仍可用

### 3. 实现 Cloudflare Provider

新增 `CloudflareProvider` 结构，实现以下 Cloudflare API 调用：

- **List DNS Records** (GET): 查询现有 DNS 记录
- **Create DNS Record** (POST): 创建新的 DNS 记录
- **Update DNS Record** (PATCH): 更新现有 DNS 记录

使用 Cloudflare API v4，通过 Bearer Token 认证。

### 4. 更新配置文件结构

新增配置选项：

```toml
# 全局配置
default_provider = "dnspod"  # 或 "cloudflare"
default_dnspod_token = "..."
default_cloudflare_token = "..."
default_cloudflare_zone_id = "..."

# 域名配置
[[domains]]
provider = "cloudflare"  # 可选，未设置时使用 default_provider
cloudflare_token = "..."  # 可选，未设置时使用 default_cloudflare_token
cloudflare_zone_id = "..."  # 可选，未设置时使用 default_cloudflare_zone_id
domain = "www.example.com"
```

### 5. 代码结构

```
src/lib.rs
├── DnsProvider trait (通用接口)
├── DnsRecord struct (通用DNS记录)
├── DnspodProvider (DNSPod实现)
│   └── impl DnsProvider
├── CloudflareProvider (Cloudflare实现)
│   └── impl DnsProvider
└── DnspodClient (向后兼容包装器)

src/main.rs
├── 更新的 Config 和 DomainConfig 结构
├── 支持多 provider 的 handle_domain 函数
└── 增强的配置验证
```

## 使用方法

### DNSPod 配置示例

```toml
[[domains]]
domain = "blog.example.com"
provider = "dnspod"
dnspod_token = "token_id,token_secret"
```

### Cloudflare 配置示例

```toml
[[domains]]
domain = "www.example.com"
provider = "cloudflare"
cloudflare_token = "your_cloudflare_api_token"
cloudflare_zone_id = "your_zone_id"
```

## 获取 Cloudflare 认证信息

### API Token

1. 登录 [Cloudflare Dashboard](https://dash.cloudflare.com/)
2. 进入 "My Profile" -> "API Tokens"
3. 创建 Token，权限设置：
   - Zone - DNS - Edit
   - Zone - Zone - Read

### Zone ID

1. 在 Cloudflare Dashboard 选择域名
2. 右侧栏找到 "Zone ID" 并复制

## API 实现细节

### Cloudflare API 端点

- **List**: `GET /zones/{zone_id}/dns_records?name={record_name}`
- **Create**: `POST /zones/{zone_id}/dns_records`
- **Update**: `PATCH /zones/{zone_id}/dns_records/{record_id}`

### 请求格式

```json
{
  "type": "A",
  "name": "example.com",
  "content": "192.0.2.1",
  "ttl": 1,
  "proxied": false
}
```

### 认证方式

```
Authorization: Bearer <api_token>
Content-Type: application/json
```

## 注意事项

1. Cloudflare 使用完整的 FQDN，不需要像 DNSPod 那样分割子域名和主域名
2. TTL 设为 1 表示自动 TTL（由 Cloudflare 管理）
3. `proxied` 设为 false，仅更新 DNS 记录，不启用 CDN 代理
4. Cloudflare List API 支持通过 `name` 参数精确过滤记录，无需使用 get API

## 向后兼容性

- 旧的配置文件格式仍然支持（使用 `token` 字段会被识别为 DNSPod）
- `DnspodClient` API 保持不变
- 旧的 `Record` 和 `Res` 类型仍可用（标记为 deprecated）

## 配置文件示例

完整示例请参考：

- `config-example.toml` - 详细的配置说明和多个示例
- `config-mixed-providers.toml` - 同时使用 DNSPod 和 Cloudflare 的实际场景

## 测试

```bash
# 编译
cargo build --release

# 运行测试
cargo test

# 使用指定配置文件运行
./target/release/ddns -c config-example.toml -v
```
