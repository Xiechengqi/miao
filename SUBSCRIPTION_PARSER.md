# Shadowsocks 订阅解析功能

## 功能说明

自动解析 SIP002 格式的 Shadowsocks 订阅，支持 simple-obfs 插件参数，转换为 sing-box 兼容的 JSON 格式。

## API 端点

```
POST /api/subscription/parse
```

## 请求格式

```json
{
  "content": "base64编码的订阅内容"
}
```

## 响应格式

```json
{
  "success": true,
  "message": "Parsed successfully",
  "data": [
    {
      "type": "shadowsocks",
      "tag": "香港 01",
      "server": "example.com",
      "server_port": 12022,
      "method": "aes-128-gcm",
      "password": "password",
      "plugin": "obfs-local",
      "plugin_opts": "obfs=http;obfs-host=example.com"
    }
  ]
}
```

## 使用示例

```bash
curl -X POST http://localhost:3000/api/subscription/parse \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -d '{"content": "BASE64_CONTENT"}'
```

## 支持的格式

- SIP002 URL 格式: `ss://base64(method:password)@server:port/?plugin=...#tag`
- simple-obfs 插件参数自动转换为 sing-box 格式
- 自动解码 URL 编码的参数和标签
