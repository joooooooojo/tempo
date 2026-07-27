import { EyeCareReminderPage } from "@/pages/EyeCareReminderPage";
import { ShelfPickerPage } from "@/pages/ShelfPickerPage";
import { PomodoroFloatPage } from "@/pages/PomodoroFloatPage";
import { MainPanelPage } from "@/pages/MainPanelPage";
import { PluginWindowPage } from "@/pages/PluginWindowPage";

function App() {
  const view = new URLSearchParams(window.location.search).get("view");
  if (view === "eye-care") {
    return <EyeCareReminderPage />;
  }

  if (view === "main-panel" || !view) {
    return <MainPanelPage />;
  }

  if (view === "pomodoro-float") {
    return <PomodoroFloatPage />;
  }

  if (view === "shelf-picker" || view === "clipboard-picker" || view === "snippet-picker") {
    return <ShelfPickerPage />;
  }

  if (view === "plugin-window") {
    return <PluginWindowPage />;
  }

  return null;
}

export default App;
