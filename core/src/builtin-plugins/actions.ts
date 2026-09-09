import { Calculator, CheckSquare2, Languages } from "lucide-react";
import { lucideIcon, type QuickAction } from "@/apps/types";
import { openLinkAction } from "@/builtin-plugins/clipboard/openLink";
import { calculateExpression } from "@/lib/calculator";
import { api } from "@/lib/api";

export const TODO_TITLE_LIMIT = 120;

const calculateAction: QuickAction = {
  id: "calculate",
  name: "复制计算结果",
  keywords: ["calculator", "calculate", "计算", "算式"],
  icon: lucideIcon(Calculator),
  source: "builtin",
  accepts: ["text"],
  priority: 200,
  exclusive: true,
  isVisible: (input) =>
    input.kind === "text" && calculateExpression(input.text) !== null,
  title: (query) => {
    const calculation = calculateExpression(query);
    return calculation ? `复制计算结果：${calculation.result}` : "复制计算结果";
  },
  async run({ query, hideAndReset }) {
    const calculation = calculateExpression(query);
    if (!calculation) throw new Error("算式无效");
    await navigator.clipboard.writeText(calculation.result);
    await hideAndReset();
  },
};

const createTodoAction: QuickAction = {
  id: "create-todo",
  name: "创建待办",
  keywords: ["todo", "待办", "任务"],
  icon: lucideIcon(CheckSquare2),
  source: "builtin",
  accepts: ["text"],
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
  accepts: ["text"],
  title: (query) => `翻译：${query}`,
  run({ query, openApp }) {
    openApp("translate", { initialTranslateText: query });
  },
};

/** Built-in quick actions. Plugins can call `registerQuickAction` later. */
export const BUILTIN_QUICK_ACTIONS: QuickAction[] = [
  calculateAction,
  openLinkAction,
  createTodoAction,
  translateAction,
];
