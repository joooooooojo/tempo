import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogPanel,
  DialogTitle,
} from "@/components/ui/dialog";
import { Card } from "@/components/ui/card";
import { api } from "@/lib/api";
import type {
  BuiltinMcpStatus,
  PluginMcpToolInfo,
  PluginSettingField,
  PluginSettingsBundle,
} from "@/types";
import { Row, Section } from "@/builtin-plugins/settings/pages/shared";
import { getBuiltinConfigPanel } from "@/builtin-plugins/settings/pages/config-registry";

export type PluginConfigTarget =
  | { source: "plugin"; id: string; name: string }
  | { source: "builtin"; id: string; name: string };

type Props = {
  target: PluginConfigTarget | null;
  busy: boolean;
  onBusyChange: (busy: boolean) => void;
  onOpenChange: (open: boolean) => void;
  onPluginMcpChanged?: () => void;
};

export function PluginConfigDialog({
  target,
  busy,
  onBusyChange,
  onOpenChange,
  onPluginMcpChanged,
}: Props) {
  const [bundle, setBundle] = useState<PluginSettingsBundle | null>(null);
  const [values, setValues] = useState<Record<string, unknown>>({});
  const [mcpTools, setMcpTools] = useState<PluginMcpToolInfo[]>([]);
  const [mcpExposed, setMcpExposed] = useState(false);
  const [loading, setLoading] = useState(false);

  const onOpenChangeRef = useRef(onOpenChange);
  onOpenChangeRef.current = onOpenChange;

  const targetKey = target ? `${target.source}:${target.id}` : null;

  const load = useCallback(async (next: PluginConfigTarget) => {
    setLoading(true);
    try {
      if (next.source === "plugin") {
        const nextBundle = await api.getPluginSettingsBundle(next.id);
        setBundle(nextBundle);
        setValues({ ...(nextBundle.values ?? {}) });
        setMcpExposed(nextBundle.mcpExposed);
        if (nextBundle.mcpToolCount > 0) {
          setMcpTools(await api.listPluginMcpTools(next.id));
        } else {
          setMcpTools([]);
        }
      } else {
        setBundle(null);
        setValues({});
        const status: BuiltinMcpStatus = await api.getBuiltinMcpStatus(next.id);
        setMcpExposed(status.exposed);
        if (status.toolCount > 0) {
          setMcpTools(await api.listBuiltinMcpTools(next.id));
        } else {
          setMcpTools([]);
        }
      }
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
      onOpenChangeRef.current(false);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!targetKey || !target) {
      setBundle(null);
      setValues({});
      setMcpTools([]);
      setMcpExposed(false);
      return;
    }
    void load(target);
    // Depend on targetKey only — parent re-renders must not remount dialog content.
    // eslint-disable-next-line react-hooks/exhaustive-deps -- target follows targetKey
  }, [targetKey, load]);

  const setLocalValue = (fieldId: string, value: unknown) => {
    setValues((prev) => ({ ...prev, [fieldId]: value }));
  };

  const patchSetting = async (field: PluginSettingField, value: unknown) => {
    if (!target || target.source !== "plugin") return;
    const previous = values;
    setValues((prev) => ({ ...prev, [field.id]: value }));
    try {
      const saved = await api.setPluginSettingsValues(target.id, { [field.id]: value });
      setValues(saved);
    } catch (error) {
      setValues(previous);
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };

  const setExposure = async (exposed: boolean) => {
    if (!target) return;
    const previous = mcpExposed;
    setMcpExposed(exposed);
    try {
      if (target.source === "plugin") {
        await api.setPluginMcpExposed(target.id, exposed);
      } else {
        await api.setBuiltinMcpExposed(target.id, exposed);
      }
      onPluginMcpChanged?.();
    } catch (error) {
      setMcpExposed(previous);
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };

  const setToolEnabled = async (toolName: string, enabled: boolean) => {
    if (!target) return;
    const previous = mcpTools;
    setMcpTools((tools) =>
      tools.map((tool) => (tool.name === toolName ? { ...tool, enabled } : tool))
    );
    try {
      if (target.source === "plugin") {
        await api.setPluginMcpToolEnabled(target.id, toolName, enabled);
      } else {
        await api.setBuiltinMcpToolEnabled(target.id, toolName, enabled);
      }
      onPluginMcpChanged?.();
    } catch (error) {
      setMcpTools(previous);
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };

  const titleName = target?.name ?? "";
  const settings = bundle?.settings ?? [];
  const showSettings = target?.source === "plugin" && settings.length > 0;
  const showMcp = mcpTools.length > 0;
  const BuiltinConfig =
    target?.source === "builtin" ? getBuiltinConfigPanel(target.id) : null;
  const showBuiltinConfig = BuiltinConfig != null;

  return (
    <Dialog open={target != null} onOpenChange={onOpenChange}>
      <DialogPanel
        height="90vh"
        className="plugin-config-dialog flex w-[90vw] max-w-[90vw] flex-col overflow-hidden sm:max-w-[90vw]"
      >
        <DialogHeader>
          <DialogTitle>插件配置{titleName ? ` · ${titleName}` : ""}</DialogTitle>
        </DialogHeader>
        <DialogContent
          className="plugin-config-dialog__body space-y-6 !px-5 !py-4"
          scrollAreaLabel="插件配置"
        >
          {loading ? (
            <p className="py-10 text-center text-[13px] text-muted-foreground">加载中…</p>
          ) : (
            <>
              {showBuiltinConfig && BuiltinConfig ? (
                <BuiltinConfig busy={busy} onBusyChange={onBusyChange} />
              ) : null}

              {showSettings ? (
                <Section title="插件设置">
                  <Card>
                    {settings.map((field) => (
                      <SettingFieldRow
                        key={field.id}
                        field={field}
                        value={values[field.id] ?? field.default}
                        onLocalChange={(value) => setLocalValue(field.id, value)}
                        onCommit={(value) => void patchSetting(field, value)}
                      />
                    ))}
                  </Card>
                </Section>
              ) : null}

              {showMcp ? (
                <Section title="MCP">
                  <Card>
                    <Row label="启用 MCP">
                      <Switch
                        checked={mcpExposed}
                        onCheckedChange={(checked) => void setExposure(checked)}
                      />
                    </Row>
                    {mcpTools.map((tool) => (
                      <Row
                        key={tool.name}
                        label={tool.name}
                        desc={tool.description || undefined}
                      >
                        <Switch
                          checked={tool.enabled}
                          disabled={!mcpExposed}
                          onCheckedChange={(enabled) =>
                            void setToolEnabled(tool.name, enabled)
                          }
                        />
                      </Row>
                    ))}
                  </Card>
                </Section>
              ) : null}
            </>
          )}
        </DialogContent>
      </DialogPanel>
    </Dialog>
  );
}

function settingOptions(
  field: PluginSettingField
): { value: string; label: string }[] {
  return (field.options ?? []).map((option) => ({
    value: option.value,
    label: option.label?.trim() || option.value,
  }));
}

function SettingFieldRow({
  field,
  value,
  onLocalChange,
  onCommit,
}: {
  field: PluginSettingField;
  value: unknown;
  onLocalChange: (value: unknown) => void;
  onCommit: (value: unknown) => void;
}) {
  if (field.type === "switch") {
    return (
      <Row label={field.title} desc={field.description ?? undefined}>
        <Switch
          checked={Boolean(value)}
          onCheckedChange={(checked) => onCommit(checked)}
        />
      </Row>
    );
  }

  if (field.type === "select") {
    const items = settingOptions(field);
    const current =
      typeof value === "string" ? value : String(field.default ?? "");
    return (
      <Row label={field.title} desc={field.description ?? undefined}>
        <Select
          items={items}
          value={current}
          onValueChange={(next) => {
            if (next != null) onCommit(next);
          }}
        >
          <SelectTrigger className="h-9 min-w-[9rem] text-[13px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent overlayLayer>
            <SelectGroup>
              {items.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
      </Row>
    );
  }

  if (field.type === "multiselect") {
    const items = settingOptions(field);
    const current = Array.isArray(value)
      ? value.filter((item): item is string => typeof item === "string")
      : Array.isArray(field.default)
        ? field.default.filter((item): item is string => typeof item === "string")
        : [];
    const selectedLabels = items
      .filter((item) => current.includes(item.value))
      .map((item) => item.label);
    return (
      <Row label={field.title} desc={field.description ?? undefined}>
        <Select
          multiple
          items={items}
          value={current}
          onValueChange={(next) => {
            onCommit(Array.isArray(next) ? next : []);
          }}
        >
          <SelectTrigger className="h-9 min-w-[9rem] max-w-[16rem] text-[13px]">
            <SelectValue>
              {selectedLabels.length > 0
                ? selectedLabels.join("、")
                : "未选择"}
            </SelectValue>
          </SelectTrigger>
          <SelectContent overlayLayer>
            <SelectGroup>
              {items.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
      </Row>
    );
  }

  const current = typeof value === "string" ? value : String(field.default ?? "");
  return (
    <Row label={field.title} desc={field.description ?? undefined}>
      <Input
        className="h-9 w-48 text-[13px]"
        value={current}
        placeholder={field.placeholder ?? undefined}
        onChange={(event) => onLocalChange(event.target.value)}
        onBlur={(event) => onCommit(event.target.value)}
      />
    </Row>
  );
}
