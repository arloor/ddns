## 调用Dnspod的api来做ddns

从环境变量中读取dnspod的token和域名

```shell
export dnspod_token="xxxxxx,594xxxxxxxxxxxxxxxxxxxx73"
export dnspod_domain="example.com"
export dnspod_subdomain="www"
export dnspod_ip_url="https://bwg.arloor.dev:444/ip" ## 查询ip的url，可以不指定
```

## Systemd配置

```shell
cat > /lib/systemd/system/ddns.service <<EOF
[Unit]
Description=forwardproxy-Http代理
After=network-online.target
Wants=network-online.target

[Service]
WorkingDirectory=/opt/ddns
EnvironmentFile=/opt/ddns/env
ExecStart=/usr/local/bin/ddns
LimitNOFILE=100000
Restart=always
RestartSec=30

[Install]
WantedBy=multi-user.target
EOF

mkdir /opt/ddns
cat > /opt/ddns/env <<EOF
dnspod_token="xxxxxx,594xxxxxxxxxxxxxxxxxxxx73"
dnspod_domain="example.com"
dnspod_subdomain="www"
## 查询ip的url，可以不指定
dnspod_ip_url="http://whatismyip.akamai.com"
EOF
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
