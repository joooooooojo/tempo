import { useState } from "react";
import { toast } from "sonner";
import { api } from "@/lib/api";
import { openNativeFileDialog } from "@/lib/nativeFileDialog";
import type { Settings } from "@/types";
import {
  Row,
  type SettingsUpdater,
} from "@/builtin-plugins/settings/pages/shared";

interface MainPanelIconSettingProps {
  dataUrl: Settings["main_panel_icon_data_url"];
  update: SettingsUpdater;
}

export function MainPanelIconSetting({ dataUrl, update }: MainPanelIconSettingProps) {
  const [processing, setProcessing] = useState(false);
  const iconSrc = dataUrl || "/favicon.png";

  const selectIcon = async () => {
    setProcessing(true);
    try {
      const selected = await openNativeFileDialog({
        multiple: false,
        title: "选择主面板图标",
        filters: [
          {
            name: "图片",
            extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp", "ico"],
          },
        ],
      });
      if (!selected || Array.isArray(selected)) return;

      const nextDataUrl = await api.createMainPanelIconDataUrl(selected);
      await update({ main_panel_icon_data_url: nextDataUrl }).catch(() => undefined);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setProcessing(false);
    }
  };

  return (
    <Row label="主面板图标" desc="显示在搜索栏右侧，点击后仍会打开设置">
      <button
        type="button"
        className="main-panel-icon-picker"
        disabled={processing}
        aria-label="更换主面板图标"
        title="更换主面板图标"
        onClick={() => void selectIcon()}
      >
        <img
          src={iconSrc}
          alt=""
          className="main-panel-icon-picker__image"
          aria-hidden="true"
        />
      </button>
    </Row>
  );
}
