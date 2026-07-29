import {
  Field,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import {
  StringListField,
  ToggleListField,
} from "@/builtin-plugins/plugin-dev-assistant/components/ManifestControls";
import { PluginDevSection } from "@/builtin-plugins/plugin-dev-assistant/components/PluginDevSection";
import {
  resolvedManifestKind,
  type EditablePluginManifest,
  type PluginCapability,
  type PluginKind,
} from "@/builtin-plugins/plugin-dev-assistant/pages/manifest";
import { KIND_ITEMS } from "@/builtin-plugins/plugin-dev-assistant/pages/shared";

const CAPABILITY_ITEMS = [
  { value: "filesystem", label: "文件系统" },
  { value: "network", label: "网络" },
  { value: "process", label: "进程" },
  { value: "clipboard", label: "剪贴板" },
  { value: "system", label: "系统" },
] as const;

function setOptionalString(
  target: Record<string, unknown>,
  key: string,
  value: string,
) {
  if (value === "") delete target[key];
  else target[key] = value;
}

export function ManifestRootEditor({
  manifest,
  onUpdate,
}: {
  manifest: EditablePluginManifest;
  onUpdate: (mutate: (next: EditablePluginManifest) => void) => void;
}) {
  const kind = resolvedManifestKind(manifest);

  return (
    <>
      <PluginDevSection title="基础信息">
        <FieldGroup>
          <div className="plugin-dev-form-grid plugin-dev-form-grid--3">
            <Field>
              <FieldLabel htmlFor="manifest-id">插件 ID</FieldLabel>
              <Input
                id="manifest-id"
                value={manifest.id ?? ""}
                onChange={(event) =>
                  onUpdate((next) => {
                    next.id = event.target.value;
                  })
                }
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="manifest-name">名称</FieldLabel>
              <Input
                id="manifest-name"
                value={manifest.name ?? ""}
                onChange={(event) =>
                  onUpdate((next) => {
                    next.name = event.target.value;
                  })
                }
              />
            </Field>
            <Field>
              <FieldLabel>插件类型</FieldLabel>
              <Select
                items={KIND_ITEMS}
                value={kind}
                onValueChange={(value) =>
                  value &&
                  onUpdate((next) => {
                    const nextKind = value as PluginKind;
                    next.kind = nextKind;
                    if (nextKind === "ui") {
                      delete next.main;
                      delete next.activationEvents;
                      if (next.contributes.apps.length === 0) {
                        next.contributes.apps.push({
                          id: "main",
                          name: next.name || "Main",
                          entry: "index.html",
                          keywords: [],
                          windowMode: "normal",
                        });
                      }
                    } else if (nextKind === "headless") {
                      next.main = next.main || "main.mjs";
                      next.contributes.apps = [];
                    } else {
                      next.main = next.main || "main.mjs";
                      if (next.contributes.apps.length === 0) {
                        next.contributes.apps.push({
                          id: "main",
                          name: next.name || "Main",
                          entry: "index.html",
                          keywords: [],
                          windowMode: "normal",
                        });
                      }
                    }
                  })
                }
              >
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {KIND_ITEMS.map((item) => (
                      <SelectItem key={item.value} value={item.value}>
                        {item.label}
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
            </Field>
          </div>
          <div className="plugin-dev-form-grid plugin-dev-form-grid--3">
            <Field>
              <FieldLabel htmlFor="manifest-version">版本</FieldLabel>
              <Input
                id="manifest-version"
                value={manifest.version ?? ""}
                onChange={(event) =>
                  onUpdate((next) => {
                    next.version = event.target.value;
                  })
                }
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="manifest-tempo">Tempo</FieldLabel>
              <Input
                id="manifest-tempo"
                value={manifest.engines.tempo ?? ""}
                onChange={(event) =>
                  onUpdate((next) => {
                    next.engines.tempo = event.target.value;
                  })
                }
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="manifest-api">Plugin API</FieldLabel>
              <Input
                id="manifest-api"
                value={manifest.engines.pluginApi ?? ""}
                onChange={(event) =>
                  onUpdate((next) => {
                    next.engines.pluginApi = event.target.value;
                  })
                }
              />
            </Field>
          </div>
          <Field>
            <FieldLabel htmlFor="manifest-description">描述</FieldLabel>
            <Textarea
              id="manifest-description"
              rows={3}
              value={manifest.description ?? ""}
              onChange={(event) =>
                onUpdate((next) =>
                  setOptionalString(next, "description", event.target.value),
                )
              }
            />
          </Field>
          <div className="plugin-dev-form-grid plugin-dev-form-grid--2">
            <Field data-disabled>
              <FieldLabel htmlFor="manifest-format-version">Manifest 版本</FieldLabel>
              <Input
                id="manifest-format-version"
                value={manifest.manifestVersion ?? 1}
                disabled
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="manifest-schema">JSON Schema</FieldLabel>
              <Input
                id="manifest-schema"
                value={manifest.$schema ?? ""}
                onChange={(event) =>
                  onUpdate((next) =>
                    setOptionalString(next, "$schema", event.target.value),
                  )
                }
              />
            </Field>
          </div>
        </FieldGroup>
      </PluginDevSection>

      <PluginDevSection title="发布信息">
        <FieldGroup>
          <div className="plugin-dev-form-grid plugin-dev-form-grid--3">
            {[
              ["author", "作者"],
              ["publisher", "发布者"],
              ["license", "许可证"],
            ].map(([key, label]) => (
              <Field key={key}>
                <FieldLabel htmlFor={`manifest-${key}`}>{label}</FieldLabel>
                <Input
                  id={`manifest-${key}`}
                  value={String(manifest[key] ?? "")}
                  onChange={(event) =>
                    onUpdate((next) =>
                      setOptionalString(next, key, event.target.value),
                    )
                  }
                />
              </Field>
            ))}
          </div>
          <div className="plugin-dev-form-grid plugin-dev-form-grid--2">
            {[
              ["homepage", "主页"],
              ["repository", "代码仓库"],
            ].map(([key, label]) => (
              <Field key={key}>
                <FieldLabel htmlFor={`manifest-${key}`}>{label}</FieldLabel>
                <Input
                  id={`manifest-${key}`}
                  value={String(manifest[key] ?? "")}
                  onChange={(event) =>
                    onUpdate((next) =>
                      setOptionalString(next, key, event.target.value),
                    )
                  }
                />
              </Field>
            ))}
          </div>
          <StringListField
            label="分类"
            itemLabel="分类"
            placeholder="tools"
            items={manifest.categories ?? []}
            onChange={(items) =>
              onUpdate((next) => {
                if (items.length > 0) next.categories = items;
                else delete next.categories;
              })
            }
          />
        </FieldGroup>
      </PluginDevSection>

      <PluginDevSection title="运行与权限">
        <FieldGroup>
          {kind !== "ui" ? (
            <div className="plugin-dev-form-grid plugin-dev-form-grid--2">
              <Field>
                <FieldLabel htmlFor="manifest-main">Runtime 入口</FieldLabel>
                <Input
                  id="manifest-main"
                  value={manifest.main ?? ""}
                  placeholder="main.mjs"
                  spellCheck={false}
                  onChange={(event) =>
                    onUpdate((next) => {
                      const value = event.target.value.trim();
                      if (value) next.main = value;
                      else delete next.main;
                    })
                  }
                />
              </Field>
              <Field orientation="vertical">
                <FieldLabel htmlFor="manifest-startup">立即激活</FieldLabel>
                <Switch
                  id="manifest-startup"
                  checked={(manifest.activationEvents ?? []).includes("onStartup")}
                  onCheckedChange={(checked) =>
                    onUpdate((next) => {
                      if (checked) next.activationEvents = ["onStartup"];
                      else delete next.activationEvents;
                    })
                  }
                />
              </Field>
            </div>
          ) : null}
        </FieldGroup>
      </PluginDevSection>

      <PluginDevSection
        title="能力声明"
        description="用于安装和授权界面的能力披露"
      >
        <ToggleListField<PluginCapability>
          options={CAPABILITY_ITEMS}
          values={manifest.capabilities ?? []}
          onChange={(values) =>
            onUpdate((next) => {
              if (values.length > 0) next.capabilities = values;
              else delete next.capabilities;
            })
          }
        />
      </PluginDevSection>
    </>
  );
}
