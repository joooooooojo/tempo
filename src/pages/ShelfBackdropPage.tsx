import { useEffect } from "react";
import { api } from "@/lib/api";

export function ShelfBackdropPage() {
  useEffect(() => {
    document.getElementById("boot-splash")?.remove();
    document.documentElement.style.background = "transparent";
    document.body.style.background = "transparent";
    document.body.style.overflow = "hidden";
  }, []);

  return (
    <button
      type="button"
      aria-label="关闭剪贴板"
      className="fixed inset-0 h-full w-full cursor-default border-0 bg-transparent p-0"
      onClick={() => void api.hideShelfPicker()}
    />
  );
}
