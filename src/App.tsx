import { getCurrentWindow } from "@tauri-apps/api/window";
import UpdatePopup from "./features/update/UpdatePopup";
import Dashboard from "./pages/Dashboard/Dashboard";

export default function App() {
  return getCurrentWindow().label === "updater" ? <UpdatePopup /> : <Dashboard />;
}
