import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { updateFromNative, updateFromPlugin } from "./update";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

(async () => {
  const updated = await updateFromNative();
  if (!updated) {
    await updateFromPlugin();
  }
})();
