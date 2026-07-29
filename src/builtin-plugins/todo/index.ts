import { ListTodo } from "lucide-react";
import { lucideIcon, type TempoApp } from "@/apps/types";
import { reactApp, wrapPage } from "@/builtin-plugins/reactApp";
import { TodoPage } from "@/builtin-plugins/todo/pages/TodoPage";

export { TodoPage } from "@/builtin-plugins/todo/pages/TodoPage";
export { ReminderDialog } from "@/builtin-plugins/todo/components/ReminderDialog";

export const todoApp: TempoApp = reactApp({
  id: "todo",
  name: "待办事项",
  keywords: ["todo", "任务", "待办", "todos"],
  icon: lucideIcon(ListTodo),
  component: wrapPage(TodoPage),
});
