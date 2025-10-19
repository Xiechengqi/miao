import { useState, useEffect, useCallback } from "react";
import {
  Button,
  Tabs,
  Table,
  Alert,
  Spin,
  Space,
  message,
  Tag,
  Card,
  Row,
  Col,
  Descriptions,
  List,
} from "antd";
import { LoadingOutlined } from "@ant-design/icons";

const { TabPane } = Tabs;

interface NetCheck {
  id: number;
  status: number;
  time: string;
}

interface ActionRecord {
  id: number;
  type: number;
  time: string;
}

interface ConfigData {
  config_stat: any;
  config: SingBoxConfig;
}

interface SingBoxConfig {
  log: {
    disabled: boolean;
    output: string;
    timestamp: boolean;
    level: string;
  };
  experimental: {
    clash_api: {
      external_controller: string;
      external_ui: string;
    };
  };
  dns: {
    final: string;
    strategy: string;
    independent_cache: boolean;
    servers: Array<{
      type: string;
      tag: string;
      server: string;
      detour?: string;
    }>;
    rules: Array<{
      rule_set?: string[];
      action: string;
      server?: string;
    }>;
  };
  inbounds: Array<{
    type: string;
    tag: string;
    interface_name?: string;
    address?: string[];
    mtu?: number;
    auto_route?: boolean;
    strict_route?: boolean;
    auto_redirect?: boolean;
  }>;
  outbounds: Array<{
    type?: string;
    tag: string;
    outbounds?: string[];
    server?: string;
    server_port?: number;
    password?: string;
    up_mbps?: number;
    down_mbps?: number;
    tls?: {
      enabled: boolean;
      insecure: boolean;
      server_name: string;
    };
  }>;
  route: {
    final: string;
    auto_detect_interface: boolean;
    default_domain_resolver: string;
    rules: Array<{
      action?: string;
      protocol?: string;
      ip_is_private?: boolean;
      process_path?: string[];
      rule_set?: string[];
      outbound?: string;
    }>;
    rule_set: Array<{
      type: string;
      tag: string;
      format: string;
      path: string;
    }>;
  };
}

function ConfigDisplay({ config, configStat }: { config: SingBoxConfig, configStat: any }) {
  return (
    <>
      <Descriptions
        title="配置文件信息"
        bordered
        style={{ marginBottom: "20px" }}
        column={2}
      >
        <Descriptions.Item label="文件大小">
          {configStat.size} 字节
        </Descriptions.Item>
        <Descriptions.Item label="最后修改时间">
          {new Date(configStat.mtimeMs).toLocaleString()}
        </Descriptions.Item>
        <Descriptions.Item label="创建时间">
          {new Date(configStat.birthtimeMs).toLocaleString()}
        </Descriptions.Item>
        <Descriptions.Item label="访问时间">
          {new Date(configStat.atimeMs).toLocaleString()}
        </Descriptions.Item>
      </Descriptions>
      <Tabs>
        <TabPane tab="日志" key="log">
        <Descriptions bordered column={1}>
          <Descriptions.Item label="禁用">
            {config.log.disabled ? "是" : "否"}
          </Descriptions.Item>
          <Descriptions.Item label="输出">
            {config.log.output}
          </Descriptions.Item>
          <Descriptions.Item label="时间戳">
            {config.log.timestamp ? "是" : "否"}
          </Descriptions.Item>
          <Descriptions.Item label="级别">{config.log.level}</Descriptions.Item>
        </Descriptions>
      </TabPane>
      <TabPane tab="实验性" key="experimental">
        <Descriptions bordered column={1}>
          <Descriptions.Item label="外部控制器">
            {config.experimental.clash_api.external_controller}
          </Descriptions.Item>
          <Descriptions.Item label="外部 UI">
            {config.experimental.clash_api.external_ui}
          </Descriptions.Item>
        </Descriptions>
      </TabPane>
      <TabPane tab="DNS" key="dns">
        <Descriptions
          bordered
          column={1}
          style={{ marginBottom: "20px" }}
          labelStyle={{ fontWeight: "bold" }}
        >
          <Descriptions.Item label="最终服务器">
            <Tag color="blue">{config.dns.final}</Tag>
          </Descriptions.Item>
          <Descriptions.Item label="策略">
            <Tag>{config.dns.strategy}</Tag>
          </Descriptions.Item>
          <Descriptions.Item label="独立缓存">
            {config.dns.independent_cache ? (
              <Tag color="green">启用</Tag>
            ) : (
              <Tag color="red">禁用</Tag>
            )}
          </Descriptions.Item>
        </Descriptions>
        <h4 style={{ marginBottom: "16px" }}>DNS 服务器</h4>
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: "16px",
          }}
        >
          {config.dns.servers.map((server, index) => (
            <Card
              key={index}
              style={{
                width: "350px",
                borderRadius: "8px",
                boxShadow: "0 4px 12px rgba(0, 0, 0, 0.15)",
              }}
              hoverable
            >
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  marginBottom: "12px",
                }}
              >
                <Tag
                  color="geekblue"
                  style={{ fontSize: "14px", fontWeight: "bold" }}
                >
                  {server.type?.toUpperCase()}
                </Tag>
              </div>
              <Descriptions
                column={1}
                size="small"
                bordered={false}
                labelStyle={{ fontWeight: "bold", color: "#666" }}
                contentStyle={{ color: "#333" }}
              >
                <Descriptions.Item label="标签">
                  <Tag color="orange">{server.tag}</Tag>
                </Descriptions.Item>
                <Descriptions.Item label="服务器">
                  <code style={{ fontSize: "12px" }}>{server.server}</code>
                </Descriptions.Item>
                {server.detour && (
                  <Descriptions.Item label="绕行">
                    <Tag color="purple">{server.detour}</Tag>
                  </Descriptions.Item>
                )}
              </Descriptions>
            </Card>
          ))}
        </div>
        <h4 style={{ marginTop: "32px", marginBottom: "16px" }}>DNS 规则</h4>
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: "16px",
          }}
        >
          {config.dns.rules.map((rule, index) => (
            <Card
              key={index}
              style={{
                width: "350px",
                borderRadius: "8px",
                boxShadow: "0 4px 12px rgba(0, 0, 0, 0.15)",
              }}
              hoverable
            >
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  marginBottom: "12px",
                }}
              >
                <Tag
                  color={
                    rule.action === "route"
                      ? "blue"
                      : rule.action === "hijack-dns"
                        ? "orange"
                        : "gray"
                  }
                  style={{
                    fontSize: "14px",
                    fontWeight: "bold",
                  }}
                >
                  {rule.action?.toUpperCase()}
                </Tag>
              </div>
              <Descriptions
                column={1}
                size="small"
                bordered={false}
                labelStyle={{ fontWeight: "bold", color: "#666" }}
                contentStyle={{ color: "#333" }}
              >
                {rule.rule_set && (
                  <Descriptions.Item label="规则集">
                    {rule.rule_set.map((r) => (
                      <Tag key={r} color="blue">
                        {r}
                      </Tag>
                    ))}
                  </Descriptions.Item>
                )}
                {rule.server && (
                  <Descriptions.Item label="服务器">
                    <Tag color="green">{rule.server}</Tag>
                  </Descriptions.Item>
                )}
              </Descriptions>
            </Card>
          ))}
        </div>
      </TabPane>
      <TabPane tab="入站" key="inbounds">
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: "16px",
          }}
        >
          {config.inbounds.map((inbound, index) => (
            <Card
              key={index}
              style={{
                width: "400px",
                borderRadius: "8px",
                boxShadow: "0 4px 12px rgba(0, 0, 0, 0.15)",
              }}
              hoverable
            >
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  marginBottom: "12px",
                }}
              >
                <Tag
                  color="cyan"
                  style={{ fontSize: "14px", fontWeight: "bold" }}
                >
                  {inbound.type?.toUpperCase()}
                </Tag>
              </div>
              <Descriptions
                column={1}
                size="small"
                bordered={false}
                labelStyle={{ fontWeight: "bold", color: "#666" }}
                contentStyle={{ color: "#333" }}
              >
                <Descriptions.Item label="标签">
                  <Tag color="purple">{inbound.tag}</Tag>
                </Descriptions.Item>
                {inbound.interface_name && (
                  <Descriptions.Item label="接口">
                    <code style={{ fontSize: "12px" }}>
                      {inbound.interface_name}
                    </code>
                  </Descriptions.Item>
                )}
                {inbound.address && (
                  <Descriptions.Item label="地址">
                    {inbound.address.map((a) => (
                      <Tag key={a}>{a}</Tag>
                    ))}
                  </Descriptions.Item>
                )}
                {inbound.mtu && (
                  <Descriptions.Item label="MTU">
                    {inbound.mtu}
                  </Descriptions.Item>
                )}
                {inbound.auto_route !== undefined && (
                  <Descriptions.Item label="自动路由">
                    {inbound.auto_route ? (
                      <Tag color="green">启用</Tag>
                    ) : (
                      <Tag color="red">禁用</Tag>
                    )}
                  </Descriptions.Item>
                )}
                {inbound.strict_route !== undefined && (
                  <Descriptions.Item label="严格路由">
                    {inbound.strict_route ? (
                      <Tag color="green">启用</Tag>
                    ) : (
                      <Tag color="red">禁用</Tag>
                    )}
                  </Descriptions.Item>
                )}
                {inbound.auto_redirect !== undefined && (
                  <Descriptions.Item label="自动重定向">
                    {inbound.auto_redirect ? (
                      <Tag color="green">启用</Tag>
                    ) : (
                      <Tag color="red">禁用</Tag>
                    )}
                  </Descriptions.Item>
                )}
              </Descriptions>
            </Card>
          ))}
        </div>
      </TabPane>
      <TabPane tab="出站" key="outbounds">
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: "16px",
          }}
        >
          {config.outbounds
            .filter((o) => o.type !== "urltest")
            .map((outbound) => (
              <Card
                key={outbound.tag}
                style={{
                  width: "450px",
                  borderRadius: "8px",
                  boxShadow: "0 4px 12px rgba(0, 0, 0, 0.15)",
                }}
                bodyStyle={{ padding: "24px 24px 16px 24px" }}
                hoverable
              >
                <div
                  style={{
                    display: "flex",
                    justifyContent: "space-between",
                    alignItems: "center",
                    marginBottom: "8px",
                  }}
                >
                  <h4 style={{ margin: 0 }}>
                    <Tag
                      color={
                        outbound.type === "hysteria2" ? "#1890ff" : "#52c41a"
                      }
                      style={{
                        fontSize: "14px",
                        fontWeight: "bold",
                        marginRight: "8px",
                      }}
                    >
                      {outbound.type?.toUpperCase()}
                    </Tag>
                    {outbound.tag}
                  </h4>
                  {outbound.type === "direct" && (
                    <Tag color="orange">直接连接</Tag>
                  )}
                </div>
                <Descriptions
                  column={1}
                  size="small"
                  bordered={false}
                  labelStyle={{ fontWeight: "bold", color: "#666" }}
                  contentStyle={{ color: "#333" }}
                >
                  {outbound.server && (
                    <Descriptions.Item label="服务器">
                      {outbound.server}
                    </Descriptions.Item>
                  )}
                  {outbound.server_port && (
                    <Descriptions.Item label="端口">
                      {outbound.server_port}
                    </Descriptions.Item>
                  )}

                  {outbound.outbounds && outbound.outbounds.length > 0 && (
                    <Descriptions.Item label="子节点" span={2}>
                      <div
                        style={{
                          display: "flex",
                          flexWrap: "wrap",
                          gap: "4px",
                        }}
                      >
                        {outbound.outbounds.slice(0, 5).map((sub) => (
                          <Tag key={sub} size="small">
                            {sub}
                          </Tag>
                        ))}
                        {outbound.outbounds.length > 5 && (
                          <Tag size="small" color="gray">
                            ...等 {outbound.outbounds.length - 5} 个
                          </Tag>
                        )}
                      </div>
                    </Descriptions.Item>
                  )}
                </Descriptions>
              </Card>
            ))}
        </div>
      </TabPane>
      <TabPane tab="路由" key="route">
        <Descriptions
          bordered
          column={1}
          style={{ marginBottom: "20px" }}
          labelStyle={{ fontWeight: "bold" }}
        >
          <Descriptions.Item label="最终出站">
            <Tag color="blue">{config.route.final}</Tag>
          </Descriptions.Item>
          <Descriptions.Item label="自动检测接口">
            {config.route.auto_detect_interface ? (
              <Tag color="green">启用</Tag>
            ) : (
              <Tag color="red">禁用</Tag>
            )}
          </Descriptions.Item>
          <Descriptions.Item label="默认域名解析器">
            {config.route.default_domain_resolver}
          </Descriptions.Item>
        </Descriptions>
        <h4 style={{ marginBottom: "16px" }}>路由规则</h4>
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: "16px",
          }}
        >
          {config.route.rules.map((rule, index) => (
            <Card
              key={index}
              style={{
                width: "400px",
                borderRadius: "8px",
                boxShadow: "0 4px 12px rgba(0, 0, 0, 0.15)",
              }}
              hoverable
            >
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  marginBottom: "12px",
                }}
              >
                <Tag
                  color={
                    rule.action === "route"
                      ? "blue"
                      : rule.action === "sniff"
                        ? "green"
                        : rule.action === "hijack-dns"
                          ? "orange"
                          : "gray"
                  }
                  style={{
                    fontSize: "14px",
                    fontWeight: "bold",
                  }}
                >
                  {rule.action?.toUpperCase()}
                </Tag>
              </div>
              <Descriptions
                column={1}
                size="small"
                bordered={false}
                labelStyle={{ fontWeight: "bold", color: "#666" }}
                contentStyle={{ color: "#333" }}
              >
                {rule.protocol && (
                  <Descriptions.Item label="协议">
                    <Tag>{rule.protocol}</Tag>
                  </Descriptions.Item>
                )}
                {rule.ip_is_private !== undefined && (
                  <Descriptions.Item label="私有 IP">
                    {rule.ip_is_private ? (
                      <Tag color="green">是</Tag>
                    ) : (
                      <Tag color="red">否</Tag>
                    )}
                  </Descriptions.Item>
                )}
                {rule.process_path && (
                  <Descriptions.Item label="进程路径">
                    {rule.process_path.map((p) => (
                      <Tag key={p}>{p}</Tag>
                    ))}
                  </Descriptions.Item>
                )}
                {rule.rule_set && (
                  <Descriptions.Item label="规则集">
                    {rule.rule_set.map((r) => (
                      <Tag key={r} color="blue">
                        {r}
                      </Tag>
                    ))}
                  </Descriptions.Item>
                )}
                {rule.outbound && (
                  <Descriptions.Item label="出站">
                    <Tag color="purple">{rule.outbound}</Tag>
                  </Descriptions.Item>
                )}
              </Descriptions>
            </Card>
          ))}
        </div>
        <h4 style={{ marginTop: "32px", marginBottom: "16px" }}>规则集</h4>
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: "16px",
          }}
        >
          {config.route.rule_set.map((ruleSet, index) => (
            <Card
              key={index}
              style={{
                width: "350px",
                borderRadius: "8px",
                boxShadow: "0 4px 12px rgba(0, 0, 0, 0.15)",
              }}
              hoverable
            >
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  marginBottom: "12px",
                }}
              >
                <Tag
                  color="geekblue"
                  style={{ fontSize: "14px", fontWeight: "bold" }}
                >
                  {ruleSet.type?.toUpperCase()}
                </Tag>
              </div>
              <Descriptions
                column={1}
                size="small"
                bordered={false}
                labelStyle={{ fontWeight: "bold", color: "#666" }}
                contentStyle={{ color: "#333" }}
              >
                <Descriptions.Item label="标签">
                  <Tag color="orange">{ruleSet.tag}</Tag>
                </Descriptions.Item>
                <Descriptions.Item label="格式">
                  <Tag>{ruleSet.format}</Tag>
                </Descriptions.Item>
                <Descriptions.Item label="路径">
                  <code style={{ fontSize: "12px" }}>{ruleSet.path}</code>
                </Descriptions.Item>
              </Descriptions>
            </Card>
          ))}
        </div>
      />
    </TabPane>
  </Tabs>
  </>
);
}

export function App() {
  const [netChecks, setNetChecks] = useState<NetCheck[]>([]);
  const [actionRecords, setActionRecords] = useState<ActionRecord[]>([]);
  const [config, setConfig] = useState<ConfigData | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [statusChecking, setStatusChecking] = useState<boolean>(false);
  const [dnsAnalysis, setDnsAnalysis] = useState<{
    [domain: string]: string[];
  }>({});

  const highlightLogLine = (line: string): string => {
    // 高亮时间戳
    line = line.replace(
      /(\+\d{4} \d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})/g,
      '<span style="color: darkgray; font-weight: bold;">$1</span>',
    );
    // 高亮日志级别
    line = line.replace(/\b(INFO)\b/g, '<span style="color: green;">$1</span>');
    line = line.replace(
      /\b(WARN)\b/g,
      '<span style="color: orange;">$1</span>',
    );
    line = line.replace(/\b(ERROR)\b/g, '<span style="color: red;">$1</span>');
    // 高亮括号内容 (ID 和时间)
    line = line.replace(
      /(\[[^\]]+\])/g,
      '<span style="color: purple;">$1</span>',
    );
    // 高亮 IP 地址
    line = line.replace(
      /(\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b)/g,
      '<span style="color: teal;">$1</span>',
    );
    // 高亮 IPv6
    line = line.replace(
      /(\b[0-9a-fA-F:]+::?[0-9a-fA-F:]*(?::[0-9a-fA-F]+)?\b)/g,
      '<span style="color: teal;">$1</span>',
    );
    // 高亮域名
    line = line.replace(
      /(\b[a-zA-Z0-9-]+\.[a-zA-Z]{2,}\b)/g,
      '<span style="color: navy;">$1</span>',
    );
    // 高亮端口
    line = line.replace(/(:(\d+))/g, '<span style="color: magenta;">$1</span>');
    return line;
  };

  const updateDnsAnalysis = (logLines: string[]) => {
    const analysis: { [domain: string]: string[] } = {};
    logLines.forEach((line) => {
      const dnsMatch = line.match(
        /dns: (exchanged|cached|resolved) ([A|AAAA|CNAME]+) ([^\s.]+(?:\.[^\s.]+)*)\./,
      );
      if (dnsMatch) {
        const [, , type, domain] = dnsMatch;
        const ipMatch = line.match(/IN [A|AAAA]+ ([^\s]+)/);
        if (ipMatch && ipMatch[1]) {
          if (!analysis[domain]) analysis[domain] = [];
          if (!analysis[domain].includes(ipMatch[1])) {
            analysis[domain].push(ipMatch[1]);
          }
        }
      }
    });
    setDnsAnalysis(analysis);
  };

  const [loading, setLoading] = useState<boolean>(false);
  const [singRunning, setSingRunning] = useState<boolean>(false);
  const [activeTab, setActiveTab] = useState<string>("control");

  const fetchNetChecks = useCallback(async () => {
    try {
      const response = await fetch("/api/net-checks");
      const data: NetCheck[] = await response.json();
      setNetChecks(data);
    } catch (error) {
      console.error("Failed to fetch net checks:", error);
    }
  }, []);

  const manualCheckConnection = useCallback(async () => {
    setLoading(true);
    try {
      const response = await fetch("/api/net-checks/manual", {
        method: "POST",
      });
      const data: NetCheck[] = await response.json();
      setNetChecks(data);
      message.success("连通性检测完成");
    } catch (error) {
      message.error("检测失败");
    }
    setLoading(false);
  }, []);

  const fetchActionRecords = useCallback(async () => {
    try {
      const response = await fetch("/api/sing/action-records");
      const data: ActionRecord[] = await response.json();
      setActionRecords(data);
    } catch (error) {
      console.error("Failed to fetch action records:", error);
    }
  }, []);

  const fetchConfig = useCallback(async () => {
    try {
      const response = await fetch("/api/config");
      if (response.ok) {
        const { config_stat, config_content } = await response.json();
        const parsedConfig = JSON.parse(config_content) as SingBoxConfig;
        setConfig({ config_stat, config: parsedConfig });
      } else {
        setConfig(null);
      }
    } catch (error) {
      console.error("Failed to fetch config:", error);
      setConfig(null);
    }
  }, []);

  const genConfig = useCallback(async () => {
    setLoading(true);
    try {
      const response = await fetch("/api/config/generate", { method: "POST" });
      if (response.ok) {
        message.success("配置文件生成成功");
        fetchConfig();
      } else {
        message.error("生成失败");
      }
    } catch (error) {
      message.error("网络错误");
    }
    setLoading(false);
  }, [fetchConfig]);

  const fetchLogs = useCallback(async () => {
    setStatusChecking(true);
    const startTime = Date.now();
    try {
      const response = await fetch("/api/sing/log-live");
      const data: string = await response.text();
      const lines = data.split("\n");
      lines.reverse();
      setLogs(lines);
      updateDnsAnalysis(lines);
      setSingRunning(response.ok);
    } catch (error) {
      console.error("Failed to fetch logs:", error);
      setLogs(["Sing-box 未运行"]);
      setSingRunning(false);
    } finally {
      const elapsed = Date.now() - startTime;
      const remaining = Math.max(0, 1000 - elapsed);
      setTimeout(() => setStatusChecking(false), remaining);
    }
  }, []);

  const checkSingStatus = useCallback(async () => {
    await fetchLogs(); // 顺便获取日志
  }, [fetchLogs]);

  const startSing = async () => {
    setLoading(true);
    try {
      await fetch("/api/sing/start", { method: "POST" });
      message.success("Sing-box 启动成功");
      fetchActionRecords();
      checkSingStatus();
    } catch (error) {
      message.error("启动失败");
    }
    setLoading(false);
  };

  const stopSing = async () => {
    setLoading(true);
    try {
      await fetch("/api/sing/stop", { method: "POST" });
      message.success("Sing-box 停止成功");
      fetchActionRecords();
      checkSingStatus();
    } catch (error) {
      message.error("停止失败");
    }
    setLoading(false);
  };

  const restartSing = async () => {
    setLoading(true);
    try {
      await fetch("/api/sing/restart", { method: "POST" });
      message.success("Sing-box 重启成功");
      fetchActionRecords();
      checkSingStatus();
    } catch (error) {
      message.error("重启失败");
    }
    setLoading(false);
  };

  useEffect(() => {
    fetchNetChecks();
    fetchActionRecords();
    fetchConfig();
    checkSingStatus();
  }, [fetchNetChecks, fetchActionRecords, fetchConfig, checkSingStatus]);

  const fetchStatus = useCallback(async () => {
    setStatusChecking(true);
    try {
      const response = await fetch("/api/sing/status");
      const data = await response.json();
      setSingRunning(data.running);
    } catch (error) {
      setSingRunning(false);
    } finally {
      setTimeout(() => setStatusChecking(false), 1000);
    }
  }, []);

  useEffect(() => {
    if (activeTab !== "logs") return;
    const interval = setInterval(() => {
      fetchLogs();
    }, 3500);
    return () => clearInterval(interval);
  }, [activeTab, fetchLogs]);

  useEffect(() => {
    if (activeTab !== "control") return;
    const interval = setInterval(() => {
      fetchStatus();
    }, 3000);
    return () => clearInterval(interval);
  }, [activeTab, fetchStatus]);

  const netChecksColumns = [
    {
      title: "ID",
      dataIndex: "id",
      key: "id",
    },
    {
      title: "状态",
      dataIndex: "status",
      key: "status",
      render: (status: number) =>
        status === 1 ? (
          <Tag color="green">成功</Tag>
        ) : (
          <Tag color="red">失败</Tag>
        ),
    },
    {
      title: "时间",
      dataIndex: "time",
      key: "time",
      render: (text: string) => new Date(text).toLocaleString(),
    },
  ];

  const actionRecordsColumns = [
    {
      title: "ID",
      dataIndex: "id",
      key: "id",
    },
    {
      title: "操作",
      dataIndex: "type",
      key: "type",
      render: (type: number) =>
        type === 1 ? (
          <Tag color="green">启动</Tag>
        ) : (
          <Tag color="red">停止</Tag>
        ),
    },
    {
      title: "时间",
      dataIndex: "time",
      key: "time",
      render: (text: string) => new Date(text).toLocaleString(),
    },
  ];

  const items = [
    {
      key: "control",
      label: "控制面板",
      children: (
        <div style={{ maxWidth: "800px", margin: "0 auto" }}>
          <Row gutter={[24, 24]}>
            <Col span={24}>
              <Card
                title="配置文件管理"
                bordered={false}
                style={{ boxShadow: "0 4px 12px rgba(0, 0, 0, 0.15)" }}
              >
                <Button type="primary" onClick={genConfig}>
                  生成配置文件
                </Button>
              </Card>
            </Col>
            <Col span={24}>
              <Card
                title="服务控制"
                bordered={false}
                style={{ boxShadow: "0 4px 12px rgba(0, 0, 0, 0.15)" }}
              >
                <Space>
                  <Button
                    type="primary"
                    onClick={startSing}
                    disabled={singRunning}
                  >
                    启动 Sing-box
                  </Button>
                  <Button danger onClick={stopSing} disabled={!singRunning}>
                    停止 Sing-box
                  </Button>
                  <Button type="default" onClick={restartSing}>
                    重启 Sing-box
                  </Button>
                </Space>
              </Card>
            </Col>
            <Col span={24}>
              <Card
                title="服务状态"
                bordered={false}
                style={{ boxShadow: "0 4px 12px rgba(0, 0, 0, 0.15)" }}
              >
                <div
                  style={{ display: "flex", alignItems: "center", gap: "8px" }}
                >
                  {statusChecking && <LoadingOutlined spin />}
                  <Alert
                    message={`Sing-box 状态: ${singRunning ? "运行中" : "未运行"}`}
                    type={singRunning ? "success" : "warning"}
                    showIcon
                    style={{ flex: 1 }}
                  />
                </div>
              </Card>
            </Col>
          </Row>
        </div>
      ),
    },
    {
      key: "status",
      label: "连通性检测",
      children: (
        <>
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              marginBottom: "16px",
            }}
          >
            <h3 style={{ margin: 0 }}>网络连接检查</h3>
            <Button
              type="primary"
              onClick={manualCheckConnection}
              loading={loading}
              style={{ marginLeft: "16px" }}
            >
              手动检测
            </Button>
          </div>
          <Table
            dataSource={netChecks}
            columns={netChecksColumns}
            rowKey="id"
            pagination={false}
          />
        </>
      ),
    },
    {
      key: "config",
      label: "配置文件",
      children: (
        <>
          <h3>Sing-box 配置文件</h3>
          {config ? (
            <ConfigDisplay config={config.config} configStat={config.config_stat} />
          ) : (
            <p>配置文件不存在</p>
          )}
        </>
      ),
    },
    {
      key: "logs",
      label: "实时日志",
      children: (
        <div
          style={{
            display: "flex",
            height: "calc(100vh - 200px)",
            gap: "16px",
          }}
        >
          <div style={{ flex: 1, display: "flex", flexDirection: "column" }}>
            <h3>分析组件容器</h3>
            <div style={{ flex: 1, overflowY: "auto" }}>
              <div style={{ marginBottom: "16px" }}>
                <h4>DNS 解析分析 (最近)</h4>
                {Object.keys(dnsAnalysis).length > 0 ? (
                  <div
                    style={{
                      display: "flex",
                      flexWrap: "wrap",
                      gap: "8px",
                    }}
                  >
                    {Object.entries(dnsAnalysis).map(([domain, ips]) => (
                      <Card
                        key={domain}
                        size="small"
                        style={{
                          width: "300px",
                          borderRadius: "8px",
                          boxShadow: "0 2px 8px rgba(0, 0, 0, 0.1)",
                        }}
                      >
                        <Card.Meta
                          title={
                            <span
                              style={{
                                fontSize: "14px",
                                fontWeight: "bold",
                                color: "#1890ff",
                              }}
                            >
                              {domain}
                            </span>
                          }
                          description={
                            <div
                              style={{
                                display: "flex",
                                flexWrap: "wrap",
                                gap: "4px",
                                maxHeight: "80px",
                                overflowY: "auto",
                              }}
                            >
                              {ips.map((ip) => (
                                <Tag
                                  key={ip}
                                  color="blue"
                                  style={{
                                    fontSize: "12px",
                                    margin: "0",
                                    borderRadius: "4px",
                                  }}
                                >
                                  {ip}
                                </Tag>
                              ))}
                            </div>
                          }
                        />
                      </Card>
                    ))}
                  </div>
                ) : (
                  <div style={{ fontSize: "14px", color: "gray" }}>
                    暂无 DNS 解析记录
                  </div>
                )}
              </div>
            </div>
          </div>
          <div style={{ flex: 2, display: "flex", flexDirection: "column" }}>
            <h3>Sing-box 实时日志</h3>
            <pre
              style={{
                whiteSpace: "pre-wrap",
                backgroundColor: "#f5f5f5",
                padding: "10px",
                marginTop: "10px",
                fontFamily: "monospace",
                fontSize: "18px",
                flex: 1,
                overflow: "auto",
              }}
              dangerouslySetInnerHTML={{
                __html: logs.map(highlightLogLine).join("<br>"),
              }}
            ></pre>
          </div>
        </div>
      ),
    },
    {
      key: "records",
      label: "操作记录",
      children: (
        <>
          <h3>Sing-box 操作记录</h3>
          <Table
            dataSource={actionRecords}
            columns={actionRecordsColumns}
            rowKey="id"
            pagination={false}
          />
        </>
      ),
    },
  ];

  return (
    <div style={{ padding: "20px" }}>
      <Spin spinning={loading}>
        <Tabs items={items} onChange={setActiveTab} />
      </Spin>
    </div>
  );
}
