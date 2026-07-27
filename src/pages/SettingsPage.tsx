import { useEffect, useState } from "react";
import { toast } from "sonner";
import { HardDrive, Puzzle, SlidersHorizontal } from "lucide-react";
import { api } from "@/lib/api";
import { openNativeFileDialog } from "@/lib/nativeFileDialog";
import { emitThemeChange } from "@/lib/theme";
import { getAppVersion } from "@/lib/update";
import { useUpdateStore, runCheckUpdate, runInstallUpdate } from "@/lib/updateStore";
import type { Settings } from "@/types";
import { GeneralSettingsPanel } from "@/pages/settings/GeneralSettingsPanel";
import { PluginsSettingsPanel } from "@/pages/settings/PluginsSettingsPanel";
import { StorageSettingsPanel } from "@/pages/settings/StorageSettingsPanel";
import {
  parseSettingsSectionId,
  SETTINGS_SECTIONS,
  type SettingsSectionId,
} from "@/pages/settings/shared";
import { cn } from "@/lib/utils";

const SECTION_ICONS = {
  general: SlidersHorizontal,
  plugins: Puzzle,
  storage: HardDrive,
} as const;

function readInitialSection(): SettingsSectionId {
  try {
    const params = new URLSearchParams(window.location.search);
    const fromQuery = parseSettingsSectionId(params.get("settings"));
    if (fromQuery) return fromQuery;

    const stored = window.sessionStorage.getItem("tempo.settings.section");
    const fromSession = parseSettingsSectionId(stored);
    if (fromSession) return fromSession;
  } catch {
    // ignore
  }
  return "general";
}

export function SettingsPage() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [section, setSection] = useState<SettingsSectionId>(readInitialSection);
  const [migratingStorage, setMigratingStorage] = useState(false);
  const [appVersion, setAppVersion] = useState("");
  const {
    checking: checkingUpdate,
    applying: applyingUpdate,
    progress: updateProgress,
    pendingUpdate,
    pendingVersion,
  } = useUpdateStore();

  const load = async () => {
    const next = await api.getSettings();
    setSettings(next);
  };

  useEffect(() => {
    load().catch(console.error);
    getAppVersion().then(setAppVersion).catch(console.error);
  }, []);

  useEffect(() => {
    try {
      window.sessionStorage.setItem("tempo.settings.section", section);
      const url = new URL(window.location.href);
      url.searchParams.set("settings", section);
      window.history.replaceState(null, "", `${url.pathname}${url.search}${url.hash}`);
    } catch {
      // ignore
    }
  }, [section]);

  const update = async (patch: Partial<Settings>) => {
    if (!settings) return;
    const previous = settings;
    setSettings({ ...settings, ...patch });
    try {
      await api.updateSettings(patch);
      if (patch.theme !== undefined) {
        await emitThemeChange(patch.theme);
      }
      toast.success("已保存");
    } catch (error) {
      setSettings(previous);
      toast.error(error instanceof Error ? error.message : String(error));
      throw error;
    }
  };

  const changeStorageDir = async () => {
    if (migratingStorage) return;

    try {
      const selected = await openNativeFileDialog({
        directory: true,
        multiple: false,
        title: "选择文件存储位置",
      });
      if (!selected || Array.isArray(selected)) return;

      setMigratingStorage(true);
      const nextSettings = await api.setStorageDir(selected);
      setSettings(nextSettings);
      await load();
      toast.success("文件已迁移");
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setMigratingStorage(false);
    }
  };

  const handleCheckUpdate = async () => {
    if (checkingUpdate || applyingUpdate || pendingUpdate || pendingVersion) return;

    try {
      const result = await runCheckUpdate();
      if (result.status === "busy") return;
      if (result.status === "latest") {
        toast.success("已是最新版本");
        return;
      }
      toast.success(`v${result.version} 已下载，点击「安装更新」完成更新`);
    } catch (error) {
      console.error("check update failed", error);
      toast.error("检查更新失败，请检查网络后重试");
    }
  };

  const handleInstallUpdate = async () => {
    if ((!pendingUpdate && !pendingVersion) || applyingUpdate || checkingUpdate) return;

    const needsRedownload = !pendingUpdate;
    toast.info(
      needsRedownload
        ? "正在确认并安装更新，安装完成后 Tempo 会重启。"
        : "正在安装更新，安装完成后 Tempo 会重启。",
      { duration: 8000 }
    );
    try {
      const result = await runInstallUpdate();
      if (result === "latest") {
        toast.success("已是最新版本");
      }
    } catch (error) {
      console.error("install update failed", error);
      toast.error("安装更新失败，请稍后重试");
    }
  };

  const updatePercent = updateProgress?.total
    ? Math.min(100, Math.round((updateProgress.downloaded / updateProgress.total) * 100))
    : updateProgress?.phase === "installing" || updateProgress?.phase === "ready"
      ? 100
      : 0;

  if (!settings) {
    return <p className="p-6 text-sm text-muted-foreground">加载中...</p>;
  }

  return (
    <div className="settings-shell">
      <aside className="settings-nav" aria-label="设置分类">
        <nav className="settings-nav__list">
          {SETTINGS_SECTIONS.map((item) => {
            const Icon = SECTION_ICONS[item.id];
            const selected = item.id === section;
            return (
              <button
                key={item.id}
                type="button"
                className={cn("settings-nav__item", selected && "settings-nav__item--active")}
                aria-current={selected ? "page" : undefined}
                onClick={() => setSection(item.id)}
              >
                <Icon className="settings-nav__icon" aria-hidden="true" />
                <span className="settings-nav__label">{item.label}</span>
              </button>
            );
          })}
        </nav>
      </aside>

      <div className="settings-main">
        <div className="settings-main__body" key={section}>
          {section === "general" ? (
            <GeneralSettingsPanel
              settings={settings}
              update={update}
              onSettingsChange={setSettings}
              appVersion={appVersion}
              checkingUpdate={checkingUpdate}
              applyingUpdate={applyingUpdate}
              updatePercent={updatePercent}
              pendingUpdate={Boolean(pendingUpdate)}
              pendingVersion={pendingVersion}
              updatePhase={updateProgress?.phase}
              updateDownloadVersion={updateProgress?.version}
              onCheckUpdate={handleCheckUpdate}
              onInstallUpdate={handleInstallUpdate}
            />
          ) : null}
          {section === "plugins" ? <PluginsSettingsPanel /> : null}
          {section === "storage" ? (
            <StorageSettingsPanel
              settings={settings}
              migratingStorage={migratingStorage}
              onChangeStorageDir={changeStorageDir}
            />
          ) : null}
        </div>
      </div>
    </div>
  );
}
