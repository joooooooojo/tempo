import {
  Field,
  FieldDescription,
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
  type PluginKind,
  type PluginPlatform,
} from "@/builtin-plugins/plugin-dev-assistant/pages/manifest";
import { KIND_ITEMS } from "@/builtin-plugins/plugin-dev-assistant/pages/shared";
import type { PluginPermission } from "@/types";

const PERMISSION_ITEMS: Array<{
  value: PluginPermission;
  label: string;
  description: string;
}> = [
  {
    value: "read",
    label: "文件读取",
    description: "读取任意文件和目录",
  },
  {
    value: "write",
    label: "文件写入",
    description: "写入任意文件和目录",
  },
  { value: "net", label: "网络访问", description: "访问任意网络目标，并开放托管 UI 网络" },
  { value: "env", label: "环境变量", description: "读取全部宿主环境变量" },
  { value: "sys", label: "系统信息", description: "读取操作系统和硬件信息" },
  { value: "run", label: "子进程", description: "启动和控制外部进程" },
  { value: "ffi", label: "动态库", description: "加载并调用本地动态库" },
  { value: "import", label: "远程导入", description: "加载远程模块和运行时 npm 包" },
];

const PLATFORM_ITEMS = [
  { value: "macos", label: "macOS" },
  { value: "windows", label: "Windows" },
  {
    value: "linux",
    label: "Linux",
    disabled: true,
    disabledHint: "Tempo 尚未支持 Linux 宿主",
  },
] as const;

const DEFAULT_PLATFORMS: PluginPlatform[] = ["macos", "windows"];

function selectablePlatforms(values: readonly PluginPlatform[]): PluginPlatform[] {
  return values.filter((value) => value === "macos" || value === "windows");
}

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
                      next.permissions = (next.permissions ?? []).filter(
                        (permission) => permission === "net",
                      );
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
                value={manifest.manifestVersion ?? 2}
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
            <>
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
            </>
          ) : null}
          <Field>
            <FieldLabel>敏感权限</FieldLabel>
            <Select
              multiple
              items={(kind === "ui"
                ? PERMISSION_ITEMS.filter((item) => item.value === "net")
                : PERMISSION_ITEMS
              ).map((item) => ({ value: item.value, label: item.label }))}
              value={manifest.permissions ?? []}
              onValueChange={(value) =>
                onUpdate((next) => {
                  next.permissions = Array.isArray(value)
                    ? value.filter((permission): permission is PluginPermission =>
                        PERMISSION_ITEMS.some((item) => item.value === permission),
                      )
                    : [];
                })
              }
            >
              <SelectTrigger className="w-full max-w-md">
                <SelectValue>
                  {(() => {
                    const selected = PERMISSION_ITEMS.filter((item) =>
                      (manifest.permissions ?? []).includes(item.value),
                    );
                    if (selected.length === 0) return "未选择敏感权限";
                    if (selected.length <= 2) return selected.map((item) => item.label).join("、");
                    return `已选择 ${selected.length} 项`;
                  })()}
                </SelectValue>
              </SelectTrigger>
              <SelectContent align="start" alignItemWithTrigger={false}>
                <SelectGroup>
                  {(kind === "ui"
                    ? PERMISSION_ITEMS.filter((item) => item.value === "net")
                    : PERMISSION_ITEMS
                  ).map((item) => (
                    <SelectItem key={item.value} value={item.value} title={item.description}>
                      {item.label}（全局）
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
            <FieldDescription>
              {kind === "ui"
                ? "网络权限同时控制托管 UI 的 HTTP、WebSocket、图片、媒体和字体访问"
                : "默认全部关闭；tempo.files 始终可以读写当前插件的私有数据目录"}
            </FieldDescription>
          </Field>
        </FieldGroup>
      </PluginDevSection>

      <PluginDevSection
        title="适用平台"
        description="声明插件可运行的宿主操作系统；未选择时视为 macOS 与 Windows"
      >
        <ToggleListField<PluginPlatform>
          options={PLATFORM_ITEMS}
          values={
            manifest.platforms?.length
              ? selectablePlatforms(manifest.platforms)
              : DEFAULT_PLATFORMS
          }
          requireOne
          onChange={(values) =>
            onUpdate((next) => {
              const nextPlatforms = selectablePlatforms(values);
              if (
                nextPlatforms.length === DEFAULT_PLATFORMS.length &&
                DEFAULT_PLATFORMS.every((platform) =>
                  nextPlatforms.includes(platform),
                )
              ) {
                delete next.platforms;
              } else if (nextPlatforms.length > 0) {
                next.platforms = nextPlatforms;
              } else {
                delete next.platforms;
              }
            })
          }
        />
      </PluginDevSection>
    </>
  );
}
