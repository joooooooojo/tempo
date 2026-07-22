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
import type { BuiltinApp, BuiltinAppProps } from "@/apps/types";
import { ClipboardPage } from "@/pages/ClipboardPage";
import { PomodoroPage } from "@/pages/PomodoroPage";
import { ReportsPage } from "@/pages/ReportsPage";
import { SettingsPage } from "@/pages/SettingsPage";
import { SnippetsPage } from "@/pages/SnippetsPage";
import { TodoPage } from "@/pages/TodoPage";
import { HostsPage } from "@/pages/tools/hosts/HostsPage";
import { PortManagerPage } from "@/pages/tools/port-manager/PortManagerPage";
import { TranslatePage } from "@/pages/tools/translate/TranslatePage";

function wrapPage(Page: ComponentType): ComponentType<BuiltinAppProps> {
  return function BuiltinAppPage(_props: BuiltinAppProps) {
    return <Page />;
  };
}

export const BUILTIN_APPS: BuiltinApp[] = [
  {
    id: "todo",
    name: "待办事项",
    keywords: ["todo", "任务", "待办", "todos"],
    icon: ListTodo,
    component: wrapPage(TodoPage),
    source: "builtin",
    defaultSize: { width: 920, height: 720 },
  },
  {
    id: "pomodoro",
    name: "番茄时钟",
    keywords: ["pomodoro", "番茄", "专注", "计时"],
    icon: Timer,
    component: wrapPage(PomodoroPage),
    source: "builtin",
    defaultSize: { width: 920, height: 640 },
  },
  {
    id: "reports",
    name: "屏幕使用时间",
    keywords: ["screen", "报告", "屏幕", "使用时间", "reports"],
    icon: BarChart3,
    component: wrapPage(ReportsPage),
    source: "builtin",
    defaultSize: { width: 960, height: 720 },
  },
  {
    id: "clipboard",
    name: "剪贴板",
    keywords: ["clipboard", "剪贴板", "复制"],
    icon: ClipboardList,
    component: wrapPage(ClipboardPage),
    source: "builtin",
    defaultSize: { width: 920, height: 680 },
  },
  {
    id: "snippets",
    name: "快捷短语",
    keywords: ["snippet", "短语", "快捷短语", "snippets"],
    icon: TextQuote,
    component: SnippetsPage,
    source: "builtin",
    defaultSize: { width: 920, height: 680 },
  },
  {
    id: "hosts",
    name: "Hosts",
    keywords: ["hosts", "host", "域名"],
    icon: FileCode2,
    component: wrapPage(HostsPage),
    source: "builtin",
    defaultSize: { width: 920, height: 720 },
  },
  {
    id: "translate",
    name: "聚合翻译",
    keywords: ["translate", "翻译", "有道", "deepl"],
    icon: Languages,
    component: TranslatePage,
    source: "builtin",
    defaultSize: { width: 920, height: 680 },
    persistSession: true,
  },
  {
    id: "port-manager",
    name: "端口管理器",
    keywords: ["port", "端口", "进程", "tcp", "udp"],
    icon: Cable,
    component: wrapPage(PortManagerPage),
    source: "builtin",
    defaultSize: { width: 920, height: 720 },
  },
  {
    id: "settings",
    name: "设置",
    keywords: ["settings", "设置", "偏好", "配置"],
    icon: Settings,
    component: wrapPage(SettingsPage),
    source: "builtin",
    defaultSize: { width: 900, height: 700 },
  },
];

const BY_ID = new Map(BUILTIN_APPS.map((app) => [app.id, app]));

export function getBuiltinApp(id: string): BuiltinApp | undefined {
  return BY_ID.get(id);
}

export function listBuiltinApps(): BuiltinApp[] {
  return BUILTIN_APPS;
}
