import { Button } from "@/components/ui/button";
import { useState } from "react";

export default function () {
  const [is_restarting, set_is_restarting] = useState(false);
  const [error, setError] = useState<boolean | null>(null);
  const [success, setSuccess] = useState<boolean | null>(null);

  const restart = async () => {
    setError(null);
    setSuccess(null);
    set_is_restarting(true);
    const res = await fetch("/api/sing/restart");
    set_is_restarting(false);
    if (!res.ok) {
      setError(true);
    } else {
      setSuccess(true);
    }
  };

  return (
    <div className="flex items-center gap-8">
      <Button size={"lg"} disabled={is_restarting} onClick={restart}>
        重新启动
      </Button>
      {error && <p className="text-red-500">重启失败</p>}
      {success && <p className="text-green-500">重启成功</p>}
    </div>
  );
}
