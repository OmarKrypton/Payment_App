import React from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import { updateFromNative, updateFromPlugin } from "./update";

const logFrontend = (message: string) => {
  invoke("frontend_log", { message }).catch(() => {});
};

window.addEventListener("error", (event) => {
  logFrontend(`error: ${event.message}\n${event.error?.stack || ""}`);
});
window.addEventListener("unhandledrejection", (event) => {
  logFrontend(`rejection: ${String(event.reason?.stack || event.reason)}`);
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

invoke("frontend_ready").catch(() => {});

(async () => {
  const updated = await updateFromNative();
  if (!updated) {
    await updateFromPlugin();
  }
})();
