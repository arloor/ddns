#!/bin/bash

# 测试hook脚本
echo "=== Hook Script Executed ==="
echo "Domain: $DOMAIN"
echo "New IP: $NEW_IP"
echo "Old IP: $OLD_IP"
echo "Timestamp: $(date)"
echo "=== End of Hook Script ==="

# 这里可以添加实际的hook逻辑，比如重启服务
# 例如：ssh root@exampleor.com "systemctl restart wg-quick@wg0"
