#!/bin/bash

# 测试订阅解析
echo "Testing subscription parsing..."

# 单个节点测试
SINGLE_NODE="c3M6Ly9ZV1Z6TFRFeU9DMW5ZMjA2YUdGTlRFMVlhWEpDZVc0MmNrZFdhQUBhMWY2ZGI4NC1iMjM2LTQ5YjItYmU2ZS0wN2QxNzg5NmE1ZjYubWVkaWFjb2Rlcjk1LmNvbToxMjAyMi8/cGx1Z2luPXNpbXBsZS1vYmZzJTNCb2JmcyUzRGh0dHAlM0JvYmZzLWhvc3QlM0RmNTgwMTJkN2E4OTQubWljcm9zb2Z0LmNvbSMlRjAlOUYlODclQUQlRjAlOUYlODclQjAlMjAlRTklQTYlOTklRTYlQjglQUYlMjAwMSVFNCVCOCVBODF4JTIwSEs="

echo "$SINGLE_NODE" | base64 -d

echo ""
echo "Expected output should contain:"
echo "- ss:// URL"
echo "- plugin=simple-obfs"
echo "- obfs=http"
echo "- obfs-host parameter"
