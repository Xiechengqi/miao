import Config from "./Config";
import Control from "./Control";

export default function () {
  return (
    <div className="h-full w-full flex flex-col gap-4 p-20">
      <Control />
      <Config />
    </div>
  );
}
