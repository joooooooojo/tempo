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

type DataPermission = "read" | "write";

const DATA_PERMISSION_ITEMS = [
  {
    value: "read",
    label: "读取私有数据目录",
    description: "允许 Deno Runtime 直接读取 $DATA",
  },
  {
    value: "write",
    label: "写入私有数据目录",
    description: "允许 Deno Runtime 直接写入 $DATA",
  },
] as const;

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
                      if (next.permissions) {
                        delete next.permissions.read;
                        delete next.permissions.write;
                        delete next.permissions.env;
                      }
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
          <Field orientation="vertical">
            <FieldLabel htmlFor="manifest-permissions-all">
              {kind === "ui" ? "完全网络访问" : "Deno 完全访问"}
            </FieldLabel>
            <Switch
              id="manifest-permissions-all"
              checked={manifest.permissions?.all === true}
              onCheckedChange={(checked) =>
                onUpdate((next) => {
                  next.permissions = checked ? { all: true } : {};
                })
              }
            />
            <FieldDescription>
              {kind === "ui"
                ? "允许托管 UI 访问任意网络目标"
                : "允许 Deno 读取和写入任意文件、访问网络与环境变量，并使用系统、子进程、FFI 和远程导入能力"}
            </FieldDescription>
          </Field>
          {manifest.permissions?.all !== true && kind !== "ui" ? (
            <>
              <ToggleListField<DataPermission>
                legend="Deno 数据目录权限"
                description="tempo.files 和 tempo.storage 不需要这些权限；仅在 Runtime 直接使用 Deno 文件 API 时开启"
                options={DATA_PERMISSION_ITEMS}
                values={DATA_PERMISSION_ITEMS.flatMap((item) =>
                  manifest.permissions?.[item.value]?.includes("$DATA")
                    ? [item.value]
                    : [],
                )}
                onChange={(values) =>
                  onUpdate((next) => {
                    const permissions = { ...(next.permissions ?? {}) };
                    if (values.includes("read")) permissions.read = ["$DATA"];
                    else delete permissions.read;
                    if (values.includes("write")) permissions.write = ["$DATA"];
                    else delete permissions.write;
                    next.permissions = permissions;
                  })
                }
              />
              <StringListField
                label="环境变量"
                description="只向 Deno Runtime 暴露列出的变量；使用大写变量名"
                itemLabel="环境变量"
                placeholder="OPENAI_API_KEY"
                items={manifest.permissions?.env ?? []}
                onChange={(items) =>
                  onUpdate((next) => {
                    const permissions = { ...(next.permissions ?? {}) };
                    if (items.length > 0) permissions.env = items;
                    else delete permissions.env;
                    next.permissions = permissions;
                  })
                }
              />
            </>
          ) : null}
          {manifest.permissions?.all !== true ? (
            <StringListField
              label="网络端点"
              description={
                kind === "ui"
                  ? "允许托管 UI 访问的精确 host:port"
                  : "允许 Deno Runtime 访问；托管 UI 存在时共用同一列表"
              }
              itemLabel="网络端点"
              placeholder="api.example.com:443"
              items={manifest.permissions?.net ?? []}
              onChange={(items) =>
                onUpdate((next) => {
                  const permissions = { ...(next.permissions ?? {}) };
                  if (items.length > 0) permissions.net = items;
                  else delete permissions.net;
                  next.permissions = permissions;
                })
              }
            />
          ) : null}
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
