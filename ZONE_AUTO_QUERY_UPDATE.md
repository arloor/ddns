# Cloudflare Zone ID 自动查询功能更新

## 更新说明

本次更新移除了手动配置 Cloudflare Zone ID 的要求，程序现在会自动从域名查询对应的 Zone ID 并缓存结果。

## 主要变更

### 1. Zone ID 自动查询

程序会自动从完整域名中提取根域名，然后调用 Cloudflare Zones List API 查询对应的 Zone ID：

- `www.example.com` → 提取 `example.com` → 查询 Zone ID
- `api.v2.example.com` → 提取 `example.com` → 查询 Zone ID
- `example.com` → 直接使用 `example.com` → 查询 Zone ID

### 2. Zone ID 缓存机制

为了避免重复 API 调用，程序会在内存中缓存域名到 Zone ID 的映射：

- 首次查询某个根域名时，调用 Cloudflare API 获取 Zone ID
- 后续对同一根域名的查询直接使用缓存值
- 缓存在程序运行期间一直有效

### 3. 配置文件简化

**之前的配置（已废弃）：**

```toml
default_cloudflare_token = "your_token"
default_cloudflare_zone_id = "your_zone_id"  # 需要手动获取

[[domains]]
domain = "www.example.com"
provider = "cloudflare"
cloudflare_token = "your_token"
cloudflare_zone_id = "your_zone_id"  # 需要手动获取
```

**现在的配置：**

```toml
default_cloudflare_token = "your_token"
# 可选：Account ID 可以加速 Zone 查询
# default_cloudflare_account_id = "your_account_id"

[[domains]]
domain = "www.example.com"
provider = "cloudflare"
# 不需要配置 zone_id，程序会自动查询
# cloudflare_account_id = "your_account_id"  # 可选
```

### 4. 新增配置选项

- `default_cloudflare_account_id`（可选）：全局默认的 Cloudflare Account ID
- `cloudflare_account_id`（可选，域名级别）：特定域名的 Cloudflare Account ID

**Account ID 的作用**：

- 当一个 Cloudflare 账户管理多个 Zone 时，指定 Account ID 可以加快查询速度
- 不是必需的，程序在没有 Account ID 的情况下也能正常工作

## API 调用详情

### List Zones API

**端点**：`GET https://api.cloudflare.com/client/v4/zones?name={domain_name}`

**查询参数**：

- `name`（必需）：要查询的域名（例如：`example.com`）
- `account.id`（可选）：Account ID，用于过滤特定账户下的 Zone

**请求头**：

```
Authorization: Bearer {api_token}
Content-Type: application/json
```

**响应示例**：

```json
{
  "success": true,
  "errors": [],
  "result": [
    {
      "id": "023e105f4ecef8ad9ca31a8372d0c353",
      "name": "example.com"
    }
  ]
}
```

## 代码实现

### CloudflareProvider 结构更新

```rust
pub struct CloudflareProvider {
    api_token: String,
    account_id: Option<String>,  // 新增：可选的 Account ID
    record_name: String,
    zone_cache: Mutex<HashMap<String, String>>,  // 新增：Zone ID 缓存
}
```

### 关键方法

#### 1. 提取根域名

```rust
fn extract_zone_name(record_name: &str) -> String {
    let parts: Vec<&str> = record_name.split('.').collect();
    if parts.len() >= 2 {
        format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1])
    } else {
        record_name.to_string()
    }
}
```

#### 2. 获取 Zone ID（带缓存）

```rust
fn get_zone_id(&self) -> Result<String, Error> {
    let zone_name = Self::extract_zone_name(&self.record_name);

    // 1. 先尝试从缓存读取
    {
        let cache = self.zone_cache.lock().unwrap();
        if let Some(zone_id) = cache.get(&zone_name) {
            return Ok(zone_id.clone());
        }
    }

    // 2. 缓存未命中，调用 API 查询
    let mut url = format!(
        "https://api.cloudflare.com/client/v4/zones?name={}",
        zone_name
    );

    if let Some(ref account_id) = self.account_id {
        url.push_str(&format!("&account.id={}", account_id));
    }

    let response = client.get(&url)
        .header("Authorization", format!("Bearer {}", self.api_token))
        .send()?;

    let zone_list: CloudflareZoneListResponse = response.json()?;
    let zone_id = zone_list.result[0].id.clone();

    // 3. 存入缓存
    {
        let mut cache = self.zone_cache.lock().unwrap();
        cache.insert(zone_name, zone_id.clone());
    }

    Ok(zone_id)
}
```

## 优势

1. **用户体验改善**：

   - 用户不需要手动从 Cloudflare Dashboard 获取 Zone ID
   - 配置更简单，只需要 API Token 即可

2. **自动化程度提高**：

   - 程序自动处理域名到 Zone 的映射
   - 减少人为配置错误的可能性

3. **性能优化**：

   - Zone ID 查询结果被缓存，避免重复 API 调用
   - 可选的 Account ID 参数可以加速查询

4. **灵活性增强**：
   - 支持同一账户下管理多个域名的不同 Zone
   - 自动识别根域名，支持任意级别的子域名

## 向后兼容性

虽然移除了 `cloudflare_zone_id` 配置选项，但这不会影响现有用户：

1. 旧配置文件中的 `cloudflare_zone_id` 字段会被忽略（不会报错）
2. 程序会自动查询正确的 Zone ID，无论配置文件中是否包含该字段
3. 建议用户更新配置文件，移除已废弃的 `cloudflare_zone_id` 配置

## 必需的 API 权限

确保 Cloudflare API Token 具有以下权限：

- **Zone - Zone - Read**：用于查询 Zone 列表
- **Zone - DNS - Edit**：用于管理 DNS 记录

## 日志示例

程序运行时会输出相关日志：

```
[INFO] Querying zone_id for domain: example.com
[INFO] Found zone_id for example.com: 023e105f4ecef8ad9ca31a8372d0c353
[INFO] Using cached zone_id for example.com: 023e105f4ecef8ad9ca31a8372d0c353
```

## 迁移指南

### 从旧配置迁移

1. **删除不需要的配置**：

   ```toml
   # 删除这些行
   default_cloudflare_zone_id = "..."
   cloudflare_zone_id = "..."
   ```

2. **可选：添加 Account ID**（如果你管理多个 Zone）：

   ```toml
   default_cloudflare_account_id = "your_account_id"
   ```

3. **保持其他配置不变**：

   ```toml
   default_cloudflare_token = "your_token"

   [[domains]]
   domain = "www.example.com"
   provider = "cloudflare"
   ```

### 获取 Account ID（可选）

如果需要 Account ID：

1. 登录 [Cloudflare Dashboard](https://dash.cloudflare.com/)
2. 在右侧栏找到 "Account ID"
3. 复制并添加到配置文件

## 错误处理

程序会处理以下错误情况：

1. **Zone 不存在**：

   ```
   Error: No zone found for domain: example.com
   ```

   解决方法：确保域名已添加到 Cloudflare 账户

2. **API 认证失败**：

   ```
   Error: Failed to query zone list: 403 Forbidden
   ```

   解决方法：检查 API Token 是否有效，是否有 Zone Read 权限

3. **网络错误**：
   ```
   Error: Failed to query zone list: connection timeout
   ```
   解决方法：检查网络连接，稍后重试

## 测试

编译并测试新功能：

```bash
# 编译
cargo build --release

# 运行测试
cargo test

# 使用更新后的配置运行
./target/release/ddns -c config.toml -v
```

## 相关文档

- [Cloudflare Zones API 文档](https://developers.cloudflare.com/api/resources/zones/methods/list/)
- [README.md](./README.md) - 完整的使用说明
- [CLOUDFLARE_UPDATE.md](./CLOUDFLARE_UPDATE.md) - Cloudflare 支持详细说明
- [config-example.toml](./config-example.toml) - 配置示例
