#!/bin/bash

# 测试订阅解析API
SUBSCRIPTION_CONTENT="c3M6Ly9ZV1Z6TFRFeU9DMW5ZMjA2YUdGTlRFMVlhWEpDZVc0MmNrZFdhQUBhMWY2ZGI4NC1iMjM2LTQ5YjItYmU2ZS0wN2QxNzg5NmE1ZjYubWVkaWFjb2Rlcjk1LmNvbToxMjAyMi8/cGx1Z2luPXNpbXBsZS1vYmZzJTNCb2JmcyUzRGh0dHAlM0JvYmZzLWhvc3QlM0RmNTgwMTJkN2E4OTQubWljcm9zb2Z0LmNvbSMlRjAlOUYlODclQUQlRjAlOUYlODclQjAlMjAlRTklQTYlOTklRTYlQjglQUYlMjAwMSVFNCVCOCVBODF4JTIwSEs="

curl -X POST http://localhost:3000/api/subscription/parse \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -d "{\"content\": \"$SUBSCRIPTION_CONTENT\"}" | jq .
