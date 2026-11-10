import { mount } from "svelte";
import "./controls.css";
import { installDragScroll } from "./dragScroll";

// Keep the app surface quiet while preserving native copy/paste in editable text.
window.addEventListener(
  "contextmenu",
  (e) => {
    const target = e.target instanceof Element ? e.target : null;
    const editable = target?.closest('input:not([type="range"]):not([type="checkbox"]):not([type="radio"]), textarea, [contenteditable="true"], [data-selectable], .error, pre, code');
    if (!editable) e.preventDefault();
  },
  { capture: true }
);

const params = new URLSearchParams(window.location.search);
if (!params.has("mock") && params.get("view") !== "tray") installDragScroll();
const app = (params.has("mock")
  ? import("./mock/MockConsole.svelte")
  : params.get("view") === "tray"
    ? import("./TrayMenu.svelte")
    : import("./App.svelte")
).then(({ default: App }) => mount(App, {
  target: document.getElementById("app")!,
}));

export default app;
