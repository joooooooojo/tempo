import type { ComponentType } from "react";
import {
  BarChart3,
  Cable,
  ClipboardList,
  FileCode2,
  Languages,
  ListTodo,
  Settings,
  TextQuote,
  Timer,
} from "lucide-react";
import {
  lucideIcon,
  type Registration,
  type TempoApp,
  type TempoAppProps,
} from "@/apps/types";
import { BUILTIN_OWNER } from "@/apps/constants";
import { ClipboardPage } from "@/pages/ClipboardPage";
import { PomodoroPage } from "@/pages/PomodoroPage";
import { ReportsPage } from "@/pages/ReportsPage";
import { SettingsPage } from "@/pages/SettingsPage";
import { SnippetsPage } from "@/pages/SnippetsPage";
import { TodoPage } from "@/pages/TodoPage";
import { HostsPage } from "@/pages/tools/hosts/HostsPage";
import { PortManagerPage } from "@/pages/tools/port-manager/PortManagerPage";
import { TranslatePage } from "@/pages/tools/translate/TranslatePage";

export { BUILTIN_OWNER } from "@/apps/constants";

function wrapPage(Page: ComponentType): ComponentType<TempoAppProps> {
  return function BuiltinAppPage(_props: TempoAppProps) {
    return <Page />;
  };
}

function reactApp(
  partial: Omit<TempoApp, "source" | "ui"> & {
    component: ComponentType<TempoAppProps>;
  }
): TempoApp {
  const { component, ...rest } = partial;
  return {
    ...rest,
    source: "builtin",
    ui: { type: "react", component },
  };
}

const BUILTIN_APP_DEFS: TempoApp[] = [
  reactApp({
    id: "todo",
    name: "待办事项",
    keywords: ["todo", "任务", "待办", "todos"],
    icon: lucideIcon(ListTodo),
    component: wrapPage(TodoPage),
    defaultSize: { width: 920, height: 720 },
  }),
  reactApp({
    id: "pomodoro",
    name: "番茄时钟",
    keywords: ["pomodoro", "番茄", "专注", "计时"],
    icon: lucideIcon(Timer),
    component: wrapPage(PomodoroPage),
    defaultSize: { width: 920, height: 640 },
  }),
  reactApp({
    id: "reports",
    name: "屏幕使用时间",
    keywords: ["screen", "报告", "屏幕", "使用时间", "reports"],
    icon: lucideIcon(BarChart3),
    component: wrapPage(ReportsPage),
    defaultSize: { width: 960, height: 720 },
  }),
  reactApp({
    id: "clipboard",
    name: "剪贴板",
    keywords: ["clipboard", "剪贴板", "复制"],
    icon: lucideIcon(ClipboardList),
    component: wrapPage(ClipboardPage),
    defaultSize: { width: 920, height: 680 },
  }),
  reactApp({
    id: "snippets",
    name: "快捷短语",
    keywords: ["snippet", "短语", "快捷短语", "snippets"],
    icon: lucideIcon(TextQuote),
    component: SnippetsPage,
    defaultSize: { width: 920, height: 680 },
  }),
  reactApp({
    id: "hosts",
    name: "Hosts",
    keywords: ["hosts", "host", "域名"],
    icon: lucideIcon(FileCode2),
    component: wrapPage(HostsPage),
    defaultSize: { width: 920, height: 720 },
  }),
  reactApp({
    id: "translate",
    name: "聚合翻译",
    keywords: ["translate", "翻译", "有道", "deepl"],
    icon: lucideIcon(Languages),
    component: TranslatePage,
    defaultSize: { width: 920, height: 680 },
    persistSession: true,
  }),
  reactApp({
    id: "port-manager",
    name: "端口管理器",
    keywords: ["port", "端口", "进程", "tcp", "udp"],
    icon: lucideIcon(Cable),
    component: wrapPage(PortManagerPage),
    defaultSize: { width: 920, height: 720 },
  }),
  reactApp({
    id: "settings",
    name: "设置",
    keywords: ["settings", "设置", "偏好", "配置"],
    icon: lucideIcon(Settings),
    component: wrapPage(SettingsPage),
    defaultSize: { width: 900, height: 700 },
  }),
];

type AppListener = () => void;

const apps: TempoApp[] = [];
const byId = new Map<string, TempoApp>();
const ownerById = new Map<string, string>();
const listeners = new Set<AppListener>();

function emit() {
  for (const listener of listeners) listener();
}

function assertOwner(ownerPluginId: string, appId: string) {
  const existingOwner = ownerById.get(appId);
  if (existingOwner && existingOwner !== ownerPluginId) {
    throw new Error(
      `App id "${appId}" is already owned by "${existingOwner}"; cannot register as "${ownerPluginId}"`
    );
  }
}

export function registerApp(ownerPluginId: string, app: TempoApp): Registration {
  assertOwner(ownerPluginId, app.id);
  if (byId.has(app.id) && ownerById.get(app.id) !== ownerPluginId) {
    throw new Error(`Duplicate app id "${app.id}"`);
  }

  const existingIndex = apps.findIndex((item) => item.id === app.id);
  if (existingIndex >= 0) {
    if (ownerById.get(app.id) !== ownerPluginId) {
      throw new Error(`Cannot replace app "${app.id}" owned by another registrant`);
    }
    apps[existingIndex] = app;
  } else {
    apps.push(app);
  }
  byId.set(app.id, app);
  ownerById.set(app.id, ownerPluginId);
  emit();

  return {
    dispose() {
      if (ownerById.get(app.id) !== ownerPluginId) return;
      const index = apps.findIndex((item) => item.id === app.id);
      if (index >= 0) apps.splice(index, 1);
      byId.delete(app.id);
      ownerById.delete(app.id);
      emit();
    },
  };
}

export function unregisterAll(ownerPluginId: string): void {
  const removeIds = [...ownerById.entries()]
    .filter(([, owner]) => owner === ownerPluginId)
    .map(([id]) => id);
  if (removeIds.length === 0) return;
  for (const id of removeIds) {
    const index = apps.findIndex((item) => item.id === id);
    if (index >= 0) apps.splice(index, 1);
    byId.delete(id);
    ownerById.delete(id);
  }
  emit();
}

export function getApp(id: string): TempoApp | undefined {
  return byId.get(id);
}

export function listApps(): TempoApp[] {
  return apps.slice();
}

export function subscribeApps(listener: AppListener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** @deprecated Prefer getApp */
export function getBuiltinApp(id: string): TempoApp | undefined {
  return getApp(id);
}

/** @deprecated Prefer listApps */
export function listBuiltinApps(): TempoApp[] {
  return listApps().filter((app) => app.source === "builtin");
}

/** Snapshot of builtin definitions (also registered into the dynamic registry). */
export const BUILTIN_APPS: TempoApp[] = BUILTIN_APP_DEFS;

for (const app of BUILTIN_APP_DEFS) {
  registerApp(BUILTIN_OWNER, app);
}
