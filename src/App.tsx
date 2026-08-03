import { ShelfPickerPage } from "@/builtin-plugins/clipboard";
import { LauncherContextMenuPage } from "@/pages/LauncherContextMenuPage";
import { MainPanelPage } from "@/pages/MainPanelPage";
import { PluginWindowPage } from "@/pages/PluginWindowPage";

function App() {
  const view = new URLSearchParams(window.location.search).get("view");

  if (view === "main-panel" || !view) {
    return <MainPanelPage />;
  }

  if (view === "shelf-picker" || view === "clipboard-picker" || view === "snippet-picker") {
    return <ShelfPickerPage />;
  }

  if (view === "launcher-context-menu") {
    return <LauncherContextMenuPage />;
  }

  if (view === "plugin-window") {
    return <PluginWindowPage />;
  }

  return null;
}

export default App;
