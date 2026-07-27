import { FolderOpen } from "lucide-react";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import type { Settings } from "@/types";
import { Section } from "@/pages/settings/shared";

interface StorageSettingsPanelProps {
  settings: Settings;
  migratingStorage: boolean;
  onChangeStorageDir: () => void;
}

export function StorageSettingsPanel({
  settings,
  migratingStorage,
  onChangeStorageDir,
}: StorageSettingsPanelProps) {
  return (
    <div className="settings-panel-stack">
      <Section title="文件存储">
        <Card>
          <CardContent className="flex items-center gap-3 p-4">
            <div className="min-w-0 flex-1">
              <p className="text-[14px] font-medium">文件存储位置</p>
              <p
                className="mt-1 break-all text-[12px] text-muted-foreground"
                title={settings.storage_dir}
              >
                {settings.storage_dir || "默认位置（AppData\\Tempo）"}
              </p>
              <p className="mt-2 text-[11px] leading-relaxed text-muted-foreground">
                包含数据库、剪贴板图片、插件数据与图标缓存。更换目录时会迁移现有文件。
              </p>
            </div>
            <Button
              variant="outline"
              size="sm"
              className="shrink-0"
              disabled={migratingStorage}
              onClick={() => void onChangeStorageDir()}
            >
              <FolderOpen className="h-3.5 w-3.5" />
              {migratingStorage ? "迁移中" : "更换"}
            </Button>
          </CardContent>
        </Card>
      </Section>
    </div>
  );
}
