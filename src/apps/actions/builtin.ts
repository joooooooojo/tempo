import { CheckSquare2, Languages } from "lucide-react";
import { lucideIcon, type QuickAction } from "@/apps/types";
import { api } from "@/lib/api";

export const TODO_TITLE_LIMIT = 120;

const createTodoAction: QuickAction = {
  id: "create-todo",
  name: "创建待办",
  keywords: ["todo", "待办", "任务"],
  icon: lucideIcon(CheckSquare2),
  source: "builtin",
  requiresQuery: true,
  validate: (query) =>
    query.length > TODO_TITLE_LIMIT ? `待办标题不能超过 ${TODO_TITLE_LIMIT} 个字` : null,
  title: (query) => `创建待办：${query}`,
  async run({ query, hideAndReset }) {
    await api.addTodo(query, "", null);
    await hideAndReset();
  },
};

const translateAction: QuickAction = {
  id: "translate",
  name: "聚合翻译",
  keywords: ["translate", "翻译"],
  icon: lucideIcon(Languages),
  source: "builtin",
  requiresQuery: true,
  title: (query) => `翻译：${query}`,
  run({ query, openApp }) {
    openApp("translate", { initialTranslateText: query });
  },
};

/** Built-in quick actions. Plugins can call `registerQuickAction` later. */
export const BUILTIN_QUICK_ACTIONS: QuickAction[] = [createTodoAction, translateAction];
