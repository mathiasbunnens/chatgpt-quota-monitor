import { getCurrentWindow } from "@tauri-apps/api/window";
import UpdatePopup from "./features/update/UpdatePopup";
import Settings from "./features/settings/Settings";
import Dashboard from "./pages/Dashboard/Dashboard";

export default function App() {
  const label = getCurrentWindow().label;
  if (label === "updater") return <UpdatePopup />;
  if (label === "settings") return <Settings />;
  return <Dashboard />;
}
