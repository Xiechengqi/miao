import { useState, useEffect, useCallback } from "react";
import { Button, Tabs, Table, Alert, Spin, Space, message } from "antd";

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
  config_content: string;
}

export function App() {
  const [netChecks, setNetChecks] = useState<NetCheck[]>([]);
  const [actionRecords, setActionRecords] = useState<ActionRecord[]>([]);
  const [config, setConfig] = useState<ConfigData | null>(null);
  const [logs, setLogs] = useState<string>("");
  const [loading, setLoading] = useState<boolean>(false);
  const [singRunning, setSingRunning] = useState<boolean>(false);

  const fetchNetChecks = useCallback(async () => {
    try {
      const response = await fetch("/api/net-checks");
      const data: NetCheck[] = await response.json();
      setNetChecks(data);
    } catch (error) {
      console.error("Failed to fetch net checks:", error);
    }
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
        const data: ConfigData = await response.json();
        setConfig(data);
      } else {
        setConfig(null);
      }
    } catch (error) {
      console.error("Failed to fetch config:", error);
      setConfig(null);
    }
  }, []);

  const fetchLogs = useCallback(async () => {
    try {
      const response = await fetch("/api/sing/log-live");
      const data: string = await response.text();
      setLogs(data);
      setSingRunning(response.ok);
    } catch (error) {
      console.error("Failed to fetch logs:", error);
      setLogs("Sing-box 未运行");
      setSingRunning(false);
    }
  }, []);

  const checkSingStatus = useCallback(async () => {
    await fetchLogs(); // 顺便获取日志
  }, [fetchLogs]);

  const genConfig = async () => {
    setLoading(true);
    try {
      await fetch("/api/config/generate", { method: "POST" });
      message.success("配置文件生成成功");
      fetchConfig();
    } catch (error) {
      message.error("生成失败");
    }
    setLoading(false);
  };

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
      render: (status: number) => (status === 1 ? "成功" : "失败"),
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
      render: (type: number) => (type === 1 ? "启动" : "停止"),
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
        <Space direction="vertical" style={{ width: "100%" }}>
          <div>
            <Button type="primary" onClick={genConfig}>
              生成配置文件
            </Button>
          </div>
          <div>
            <Button type="primary" onClick={startSing} disabled={singRunning}>
              启动 Sing-box
            </Button>
            <Button
              danger
              type="primary"
              onClick={stopSing}
              disabled={!singRunning}
              style={{ marginLeft: 10 }}
            >
              停止 Sing-box
            </Button>
            <Button
              type="dashed"
              onClick={restartSing}
              style={{ marginLeft: 10 }}
            >
              重启 Sing-box
            </Button>
          </div>
          <div>
            <Alert
              message={`Sing-box 状态: ${singRunning ? "运行中" : "未运行"}`}
              type={singRunning ? "success" : "warning"}
            />
          </div>
        </Space>
      ),
    },
    {
      key: "status",
      label: "连通性检测",
      children: (
        <>
          <h3>网络连接检查</h3>
          <Button onClick={fetchNetChecks}>刷新</Button>
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
            <pre
              style={{
                whiteSpace: "pre-wrap",
                backgroundColor: "#f5f5f5",
                padding: "10px",
                height: "800px",
                overflowY: "auto",
              }}
            >
              {config.config_content}
            </pre>
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
        <>
          <h3>Sing-box 实时日志</h3>
          <Button onClick={fetchLogs}>刷新日志</Button>
          <pre
            style={{
              whiteSpace: "pre-wrap",
              backgroundColor: "#f5f5f5",
              padding: "10px",
              marginTop: "10px",
            }}
          >
            {logs}
          </pre>
        </>
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
        <Tabs items={items} />
      </Spin>
    </div>
  );
}
